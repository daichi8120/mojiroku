#!/usr/bin/env python3
"""Verify native PR links and reconcile one issue's existing Project status.

Read-only by default. Uses the caller's gh login; never closes an issue, creates
a link, adds a Project, or stores credentials. See docs/CONTRIBUTING.md.
"""

import argparse
import json
import subprocess
import sys


QUERY = """query($owner:String!,$repo:String!,$number:Int!) {
  repository(owner:$owner,name:$repo) {
    issue(number:$number) {
      number state url
      closedByPullRequestsReferences(first:100,includeClosedPrs:true) {
        pageInfo { hasNextPage }
        nodes { url }
      }
      projectItems(first:10,includeArchived:true) {
        pageInfo { hasNextPage }
        nodes {
          id isArchived
          project {
            id title
            fields(first:100) {
              pageInfo { hasNextPage }
              nodes { ... on ProjectV2SingleSelectField { id name options { id name } } }
            }
          }
          fieldValues(first:100) {
            pageInfo { hasNextPage }
            nodes {
              ... on ProjectV2ItemFieldSingleSelectValue {
                name field { ... on ProjectV2SingleSelectField { name } }
              }
              ... on ProjectV2ItemFieldPullRequestValue {
                pullRequests(first:100) {
                  pageInfo { hasNextPage }
                  nodes { url }
                }
              }
            }
          }
        }
      }
    }
  }
}"""

MUTATION = """mutation($project:ID!,$item:ID!,$field:ID!,$option:String!) {
  updateProjectV2ItemFieldValue(input:{projectId:$project,itemId:$item,
    fieldId:$field,value:{singleSelectOptionId:$option}}) {
    projectV2Item { id }
  }
}"""


def gh_json(*args):
    result = subprocess.run(["gh", *args], text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "gh command failed")
    data = json.loads(result.stdout)
    if isinstance(data, dict) and data.get("errors"):
        raise RuntimeError("GitHub returned GraphQL errors; no partial data used")
    return data


def nodes(connection):
    if connection["pageInfo"]["hasNextPage"]:
        raise RuntimeError("API results are truncated; refusing to act on a partial list")
    return connection["nodes"]


def read_issue(repo, number):
    owner, name = repo.split("/")
    result = gh_json("api", "graphql", "-f", "query=" + QUERY,
                     "-f", "owner=" + owner, "-f", "repo=" + name,
                     "-F", "number=" + str(number))
    issue = (result["data"].get("repository") or {}).get("issue")
    if not issue:
        raise RuntimeError("Issue not found or not accessible")
    return issue


def assess(issue, project_id=None, pr_urls=(), open_status=None):
    """Return a status plan and missing links without changing anything."""
    items = nodes(issue["projectItems"])
    if project_id:
        items = [i for i in items if i["project"]["id"] == project_id]
    if len(items) != 1:
        raise RuntimeError("Select exactly one existing Project with --project-id")
    item = items[0]
    if item["isArchived"]:
        raise RuntimeError("Project item is archived; restore it explicitly first")
    fields = nodes(item["project"]["fields"])
    status_fields = [f for f in fields if f.get("name") == "Status"]
    if len(status_fields) != 1:
        raise RuntimeError("Project must have exactly one single-select Status field")
    field = status_fields[0]
    values = nodes(item["fieldValues"])
    current = next((v["name"] for v in values
                    if v.get("field", {}).get("name") == "Status"), None)
    if issue["state"] == "CLOSED":
        target = "Done"
    elif issue["state"] == "OPEN":
        target = open_status or current or "Todo"
        if target == "Done":
            raise RuntimeError("Open issue has Done status; choose --open-status explicitly")
    else:
        raise RuntimeError("Unknown issue state")
    options = [o for o in field["options"] if o["name"] == target]
    if len(options) != 1:
        raise RuntimeError("Project has no unique Status option: " + target)
    native = {p["url"] for p in nodes(issue["closedByPullRequestsReferences"])}
    project_links = {p["url"] for v in values if "pullRequests" in v
                     for p in nodes(v["pullRequests"])}
    return {
        "project": item["project"]["id"], "item": item["id"],
        "field": field["id"], "option": options[0]["id"],
        "state": issue["state"], "current": current, "target": target,
        "missing_native": sorted(set(pr_urls) - native),
        "missing_project": sorted(set(pr_urls) - project_links),
    }


def reconcile(repo, number, project_id=None, pr_urls=(), open_status=None, apply=False):
    issue = read_issue(repo, number)
    plan = assess(issue, project_id, pr_urls, open_status)
    print(f"{issue['url']}: {plan['current'] or '(unset)'} -> {plan['target']}")
    for url in plan["missing_native"]:
        print(f"Missing native link: {url}\n  Add its exact URL in the issue's Development selector.")
    for url in plan["missing_project"]:
        print(f"Missing Project Linked pull requests value: {url}")
    missing = plan["missing_native"] or plan["missing_project"]
    if missing:
        print("Link through the browser, wait for it to appear, then rerun. No status written.")
        return 1
    if plan["current"] != plan["target"]:
        if not apply:
            print("Status differs. Rerun with --apply after authorization.")
            return 1
        # Recheck after planning: do not overwrite a concurrent closure or status edit.
        fresh = assess(read_issue(repo, number), project_id, pr_urls, open_status)
        if fresh != plan:
            raise RuntimeError("Issue/Project changed during planning; inspect and rerun")
        gh_json("api", "graphql", "-f", "query=" + MUTATION,
                *[arg for key in ("project", "item", "field", "option")
                  for arg in ("-f", key + "=" + plan[key])])
    # Verify even no-op runs. Project automations can otherwise undo a status edit.
    final = assess(read_issue(repo, number), project_id, pr_urls, open_status)
    if (final["state"] != plan["state"] or final["current"] != plan["target"]
            or final["missing_native"] or final["missing_project"]):
        raise RuntimeError("Read-back differs from the plan; inspect Project automations")
    if pr_urls:
        print("Verified: native links, Project links, issue state, and Status.")
    else:
        print("Verified: issue state and Status only (no PR links requested).")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="OWNER/REPO; defaults to the current gh repository")
    parser.add_argument("--issue", type=int, required=True)
    parser.add_argument("--pr", type=int, action="append", default=[],
                        help="Same-repository PR to verify; repeat for multiple PRs; omit for status-only checks")
    parser.add_argument("--project-id", help="Required if the issue belongs to multiple Projects")
    parser.add_argument("--open-status", choices=["Todo", "In Progress"],
                        help="Explicit intent for open work; otherwise preserve current status")
    parser.add_argument("--apply", action="store_true", help="Write only the planned Status correction")
    args = parser.parse_args()
    try:
        repo = args.repo or gh_json("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
        if len(repo.split("/")) != 2 or not all(repo.split("/")):
            raise RuntimeError("--repo must be OWNER/REPO")
        if args.issue <= 0 or any(p <= 0 for p in args.pr):
            raise RuntimeError("Issue and PR numbers must be positive")
        urls = [f"https://github.com/{repo}/pull/{p}" for p in args.pr]
        return reconcile(repo, args.issue, args.project_id, urls, args.open_status, args.apply)
    except (RuntimeError, ValueError, KeyError, OSError) as error:
        print("Error: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
