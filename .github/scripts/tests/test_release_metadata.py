"""Release authorization must exclude mixed updates and unrelated PRs."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import release_metadata


REPOSITORY = "Technosorcery/qso-io"
PREFIX = "release-plz-"


def merged_pr(number, sha, branch="feature", head_repository=REPOSITORY):
    return {
        "number": number,
        "merged_at": "2026-11-16T00:00:00Z",
        "merge_commit_sha": sha,
        "base": {"ref": "main", "repo": {"full_name": REPOSITORY}},
        "head": {"ref": branch, "repo": {"full_name": head_repository}},
    }


class AuthorizationTests(unittest.TestCase):
    def classify(self, commits, pulls):
        return release_metadata.classify_update(commits, pulls, REPOSITORY, PREFIX)

    def test_ordinary_update_does_not_authorize_publication(self):
        self.assertIsNone(self.classify(["ordinary"], {"ordinary": [merged_pr(5, "ordinary")]}))

    def test_isolated_release_returns_its_authorizing_pr(self):
        self.assertEqual(
            self.classify(["release"], {"release": [merged_pr(6, "release", "release-plz-main")]}),
            6,
        )

    def test_release_in_either_position_of_mixed_update_fails(self):
        pulls = {
            "release": [merged_pr(6, "release", "release-plz-main")],
            "ordinary": [merged_pr(7, "ordinary")],
        }
        for commits in (["release", "ordinary"], ["ordinary", "release"]):
            with self.subTest(commits=commits):
                with self.assertRaisesRegex(ValueError, r"mixed.*#6.*#7|mixed.*#7.*#6"):
                    self.classify(commits, pulls)

    def test_two_release_prs_fail(self):
        with self.assertRaisesRegex(ValueError, r"mixed.*#6.*#7"):
            self.classify(["one", "two"], {
                "one": [merged_pr(6, "one", "release-plz-main")],
                "two": [merged_pr(7, "two", "release-plz-another")],
            })

    def test_release_with_unattributed_addition_fails(self):
        with self.assertRaisesRegex(ValueError, r"mixed.*unattributed"):
            self.classify(["release", "direct"], {
                "release": [merged_pr(6, "release", "release-plz-main")], "direct": [],
            })

    def test_association_alone_does_not_establish_authorization(self):
        wrong_sha = merged_pr(6, "another-sha", "release-plz-main")
        fork = merged_pr(6, "commit", "release-plz-main", "someone/qso-io")
        wrong_base = merged_pr(6, "commit", "release-plz-main")
        wrong_base["base"]["ref"] = "other"
        unmerged = merged_pr(6, "commit", "release-plz-main")
        unmerged["merged_at"] = None
        for pr in (wrong_sha, fork, wrong_base, unmerged):
            with self.subTest(pr=pr):
                self.assertIsNone(self.classify(["commit"], {"commit": [pr]}))

    def test_ambiguous_merge_attribution_fails(self):
        with self.assertRaisesRegex(ValueError, r"ambiguous.*commit.*#6.*#7"):
            self.classify(["commit"], {"commit": [
                merged_pr(6, "commit", "release-plz-main"), merged_pr(7, "commit"),
            ]})


class UpdateBoundaryTests(unittest.TestCase):
    def test_nested_feature_merges_are_not_counted_as_queue_entries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            def git(*args):
                return subprocess.run(
                    ["git", "-C", str(root), *args], check=True,
                    capture_output=True, text=True,
                ).stdout.strip()

            git("init", "--initial-branch=main")
            git("config", "user.name", "Release test")
            git("config", "user.email", "release-test@example.invalid")
            git("config", "commit.gpgsign", "false")
            git("commit", "--allow-empty", "-m", "Base")
            before = git("rev-parse", "HEAD")
            git("switch", "-c", "feature")
            git("commit", "--allow-empty", "-m", "Feature")
            git("switch", "-c", "nested")
            git("commit", "--allow-empty", "-m", "Nested change")
            git("switch", "feature")
            git("merge", "--no-ff", "nested", "-m", "Internal feature merge")
            git("switch", "main")
            git("merge", "--no-ff", "feature", "-m", "Merge release PR")
            after = git("rev-parse", "HEAD")

            commits = release_metadata.first_parent_commits(root, before, after)
            self.assertEqual(commits, [after], "Only the merge onto main belongs to this update")
            self.assertEqual(release_metadata.classify_update(
                commits, {after: [merged_pr(6, after, "release-plz-main")]}, REPOSITORY, PREFIX,
            ), 6)

            with self.assertRaisesRegex(ValueError, "first-parent"):
                release_metadata.first_parent_commits(root, git("rev-parse", "nested"), after)


class ReleaseNotesTests(unittest.TestCase):
    def test_reviewed_notes_are_selected_by_exact_version(self):
        changelog = """# Changelog

## [Unreleased]

Not released yet.

## [0.2.0](https://example.invalid/compare/v0.1.0...v0.2.0) - 2026-11-16

### Changed
- Maintainer-edited wording, including `literal` markup.

## [0.1.0] - 2026-11-01

Older notes.
"""
        self.assertEqual(release_metadata.release_notes(changelog, "0.2.0"),
                         "\n### Changed\n- Maintainer-edited wording, including `literal` markup.\n\n")

    def test_fenced_heading_examples_do_not_split_notes(self):
        for fence in ("```", "~~~~"):
            with self.subTest(fence=fence):
                body = f"\nExample:\n{fence}markdown\n## [0.1.0] - example\n{fence}\n\nAfter example.\n\n"
                changelog = "## [0.2.0] - 2026-11-16\n" + body + "## [0.1.0]\nOlder.\n"
                self.assertEqual(release_metadata.release_notes(changelog, "0.2.0"), body)

    def test_missing_duplicate_or_empty_release_notes_fail(self):
        for changelog in (
            "## [0.1.0]\nOld.\n",
            "## [0.2.0]\nOne.\n## [0.2.0]\nTwo.\n",
            "## [0.2.0]\n\n## [0.1.0]\nOld.\n",
        ):
            with self.subTest(changelog=changelog):
                with self.assertRaisesRegex(ValueError, r"0\.2\.0"):
                    release_metadata.release_notes(changelog, "0.2.0")


if __name__ == "__main__":
    unittest.main()
