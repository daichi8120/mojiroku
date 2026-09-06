"""Regression checks for Project automation side effects; no GitHub writes."""

import copy
import unittest
from unittest.mock import patch

import sync_issue_project as sync


def connection(values):
    return {"nodes": values, "pageInfo": {"hasNextPage": False}}


def fixture(state="OPEN", status="Todo", linked=True):
    links = [{"url": "https://github.com/example/repo/pull/12"}] if linked else []
    return {
        "number": 7, "url": "https://github.com/example/repo/issues/7", "state": state,
        "closedByPullRequestsReferences": connection(links),
        "projectItems": connection([{
            "id": "item", "isArchived": False,
            "project": {"id": "project", "title": "Work", "fields": connection([{
                "id": "status", "name": "Status", "options": [
                    {"id": s, "name": s} for s in ["Todo", "In Progress", "Done"]
                ],
            }])},
            "fieldValues": connection([
                {"name": status, "field": {"name": "Status"}},
                {"pullRequests": connection(links)},
            ]),
        }]),
    }


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.print_patch = patch("builtins.print")
        self.print_patch.start()
        self.addCleanup(self.print_patch.stop)

    def test_historical_link_on_closed_issue_restores_done(self):
        plan = sync.assess(fixture("CLOSED", "In Progress"), open_status="In Progress")
        self.assertEqual(plan["target"], "Done")

    def test_merged_link_does_not_complete_open_acceptance_work(self):
        self.assertEqual(sync.assess(fixture(status="In Progress"))["target"], "In Progress")

    def test_deferred_work_can_be_restored_after_link_automation(self):
        self.assertEqual(sync.assess(fixture(status="In Progress"), open_status="Todo")["target"], "Todo")

    def test_explicit_start_and_default_preservation(self):
        self.assertEqual(sync.assess(fixture())["target"], "Todo")
        self.assertEqual(sync.assess(fixture(), open_status="In Progress")["target"], "In Progress")

    def test_open_done_requires_an_explicit_nonclosing_choice(self):
        with self.assertRaisesRegex(RuntimeError, "Open issue"):
            sync.assess(fixture(status="Done"))

    def test_both_native_and_project_links_are_required(self):
        issue = fixture()
        issue["projectItems"]["nodes"][0]["fieldValues"]["nodes"].pop()
        url = "https://github.com/example/repo/pull/12"
        plan = sync.assess(issue, pr_urls=[url])
        self.assertFalse(plan["missing_native"])
        self.assertEqual(plan["missing_project"], [url])
        self.assertEqual(sync.assess(fixture(linked=False), pr_urls=[url])["missing_native"], [url])

    def test_ambiguous_and_archived_projects_are_not_changed(self):
        issue = fixture()
        other = copy.deepcopy(issue["projectItems"]["nodes"][0])
        other["project"]["id"] = "another-project"
        issue["projectItems"]["nodes"].append(other)
        with self.assertRaisesRegex(RuntimeError, "exactly one"):
            sync.assess(issue)
        self.assertEqual(sync.assess(issue, "project")["project"], "project")
        issue["projectItems"]["nodes"][0]["isArchived"] = True
        with self.assertRaisesRegex(RuntimeError, "archived"):
            sync.assess(issue, "project")

    def test_truncated_data_is_not_treated_as_missing_links(self):
        issue = fixture()
        issue["closedByPullRequestsReferences"]["pageInfo"]["hasNextPage"] = True
        with self.assertRaisesRegex(RuntimeError, "partial list"):
            sync.assess(issue)

    def test_missing_status_and_missing_project_are_distinct(self):
        issue = fixture()
        issue["projectItems"]["nodes"][0]["fieldValues"]["nodes"].pop(0)
        self.assertEqual(sync.assess(issue)["target"], "Todo")
        issue["projectItems"] = connection([])
        with self.assertRaisesRegex(RuntimeError, "existing Project"):
            sync.assess(issue)

    @patch.object(sync, "gh_json")
    @patch.object(sync, "read_issue")
    def test_dry_run_and_missing_link_never_mutate(self, read, gh):
        read.return_value = fixture("CLOSED", "In Progress")
        self.assertEqual(sync.reconcile("example/repo", 7), 1)
        read.return_value = fixture("CLOSED", "In Progress", linked=False)
        self.assertEqual(sync.reconcile("example/repo", 7, pr_urls=["missing"], apply=True), 1)
        gh.assert_not_called()

    @patch.object(sync, "read_issue")
    def test_success_message_names_only_the_requested_checks(self, read):
        read.return_value = fixture()
        for urls, message in [
            ([], "Verified: issue state and Status only (no PR links requested)."),
            (["https://github.com/example/repo/pull/12"],
             "Verified: native links, Project links, issue state, and Status."),
        ]:
            with self.subTest(pr_urls=urls), patch("builtins.print") as output:
                self.assertEqual(sync.reconcile("example/repo", 7, pr_urls=urls), 0)
                self.assertEqual(output.call_args.args, (message,))

    @patch.object(sync, "gh_json")
    @patch.object(sync, "read_issue")
    def test_repair_then_second_run_is_a_noop(self, read, gh):
        broken = fixture("CLOSED", "In Progress")
        correct = fixture("CLOSED", "Done")
        read.side_effect = [broken, broken, correct, correct, correct]
        self.assertEqual(sync.reconcile("example/repo", 7, apply=True), 0)
        self.assertEqual(sync.reconcile("example/repo", 7, apply=True), 0)
        self.assertEqual(gh.call_count, 1)
        self.assertIn("query=" + sync.MUTATION, gh.call_args.args)

    @patch.object(sync, "gh_json")
    @patch.object(sync, "read_issue")
    def test_concurrent_closure_is_not_overwritten(self, read, gh):
        read.side_effect = [fixture(), fixture("CLOSED", "Done")]
        with self.assertRaisesRegex(RuntimeError, "changed during planning"):
            sync.reconcile("example/repo", 7, open_status="In Progress", apply=True)
        gh.assert_not_called()

    @patch.object(sync, "gh_json")
    @patch.object(sync, "read_issue")
    def test_automation_overwriting_repair_is_reported(self, read, gh):
        read.return_value = fixture("CLOSED", "In Progress")
        with self.assertRaisesRegex(RuntimeError, "Read-back differs"):
            sync.reconcile("example/repo", 7, apply=True)


if __name__ == "__main__":
    unittest.main()
