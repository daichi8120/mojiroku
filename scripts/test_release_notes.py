"""Exercise release-note selection in isolated repositories without network access."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().with_name("release-notes.sh")


class ReleaseNotesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="mojiroku-release-notes-")
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.env = {
            **os.environ,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_AUTHOR_NAME": "Release test",
            "GIT_AUTHOR_EMAIL": "release-test@example.invalid",
            "GIT_COMMITTER_NAME": "Release test",
            "GIT_COMMITTER_EMAIL": "release-test@example.invalid",
        }
        self.git("init", "--quiet")
        self.write_config("0.5.7")
        self.base = self.commit("chore: baseline")

    def git(self, *args):
        return subprocess.run(
            ["git", *args], cwd=self.repo, env=self.env,
            check=True, text=True, capture_output=True,
        ).stdout.strip()

    def write(self, path, text):
        file = self.repo / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(text, encoding="utf-8")

    def write_config(self, version):
        self.write("src-tauri/tauri.conf.json", json.dumps({"version": version}))

    def commit(self, subject):
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "--quiet", "--allow-empty", "-m", subject)
        return self.git("rev-parse", "HEAD")

    def notes(self, revision_range):
        return subprocess.run(
            ["bash", str(SCRIPT), revision_range], cwd=self.repo, env=self.env,
            check=True, text=True, capture_output=True,
        ).stdout

    def test_curated_notes_replace_automatic_history(self):
        self.commit("feat: previously shipped history")
        self.write_config("0.6.0")
        expected = "# Release notes 0.6.0\n\nReviewed content.\n"
        self.write("docs/release-notes-v0.6.0.md", expected)
        target = self.commit("chore(release): v0.6.0")
        self.assertEqual(self.notes(f"{self.base}..{target}"), expected)

    def test_target_commit_controls_version_and_notes_not_head_or_worktree(self):
        self.write_config("0.6.0")
        expected = "Candidate version 0.6.0.\n"
        self.write("docs/release-notes-v0.6.0.md", expected)
        target = self.commit("feat: candidate")
        self.write_config("0.7.0")
        self.write("docs/release-notes-v0.6.0.md", "Wrong newer content.\n")
        self.write("docs/release-notes-v0.7.0.md", "Wrong newer version.\n")
        self.commit("chore: next release")
        self.write_config("0.8.0")
        self.write("docs/release-notes-v0.6.0.md", "Wrong dirty content.\n")
        self.write("docs/release-notes-v0.8.0.md", "Wrong dirty version.\n")
        self.assertEqual(self.notes(f"{self.base}..{target}"), expected)

    def test_missing_curated_notes_preserves_automatic_sections(self):
        self.write_config("0.6.0")
        self.commit("feat(core): add feature")
        self.commit("fix(core): repair behavior")
        target = self.commit("docs: describe behavior")
        # An uncommitted note must not accidentally override the committed release.
        self.write("docs/release-notes-v0.6.0.md", "Unreviewed working-tree notes.\n")
        output = self.notes(f"{self.base}..{target}")
        self.assertIn("- feat(core): add feature\n", output)
        self.assertIn("- fix(core): repair behavior\n", output)
        self.assertIn("- docs: describe behavior\n", output)
        self.assertEqual(output.count("### "), 3)
        self.assertNotIn("Unreviewed", output)

    def test_missing_version_metadata_still_falls_back(self):
        (self.repo / "src-tauri/tauri.conf.json").unlink()
        target = self.commit("fix: restore old metadata-free release support")
        self.assertIn("- fix: restore old metadata-free release support", self.notes(f"{self.base}..{target}"))

    def test_non_endpoint_revision_selector_keeps_automatic_fallback(self):
        self.commit("feat: selected commit")
        self.assertIn("- feat: selected commit", self.notes("HEAD^!"))

    def test_three_dot_and_omitted_endpoint_resolve_to_target(self):
        self.write_config("0.6.0")
        expected = "Reviewed target notes.\n"
        self.write("docs/release-notes-v0.6.0.md", expected)
        self.commit("feat: candidate")
        for revision_range in (f"{self.base}...HEAD", f"{self.base}..", "HEAD"):
            with self.subTest(revision_range=revision_range):
                self.assertEqual(self.notes(revision_range), expected)


if __name__ == "__main__":
    unittest.main()
