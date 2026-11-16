"""Read source and GitHub metadata for the single qso-io release workflow."""

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path


def command(*args, cwd=None):
    return subprocess.run(
        args, cwd=cwd, check=True, capture_output=True, text=True,
    ).stdout.strip()


def first_parent_commits(root, before, after):
    """Return only commits introduced along the updated branch's first parent."""
    for sha in (before, after):
        if not re.fullmatch(r"[0-9a-f]{40}", sha) or sha == "0" * 40:
            raise ValueError(f"Invalid existing-branch push revision: {sha!r}")
        command("git", "rev-parse", "--verify", f"{sha}^{{commit}}", cwd=root)

    commits = command(
        "git", "rev-list", "--first-parent", "--reverse", f"{before}..{after}", cwd=root,
    ).splitlines()
    if not commits or command("git", "rev-parse", f"{commits[0]}^1", cwd=root) != before:
        raise ValueError(f"Push {before}..{after} is not a contiguous first-parent update")
    return commits


def classify_update(commits, pulls_by_commit, repository, prefix):
    """Return the authorizing PR number, None for ordinary updates, or fail."""
    attributed = []
    releases = []
    for sha in commits:
        matches = [pr for pr in pulls_by_commit[sha] if (
            pr["merged_at"] is not None
            and pr["merge_commit_sha"] == sha
            and pr["base"]["ref"] == "main"
            and pr["base"]["repo"]["full_name"] == repository
        )]
        if len(matches) > 1:
            numbers = ", ".join(f"#{pr['number']}" for pr in matches)
            raise ValueError(f"ambiguous merge attribution for {sha}: {numbers}")

        pr = matches[0] if matches else None
        attributed.append(f"#{pr['number']} ({sha})" if pr else f"unattributed {sha}")
        if (pr and pr["head"]["repo"] is not None
                and pr["head"]["repo"]["full_name"] == repository
                and pr["head"]["ref"].startswith(prefix)):
            releases.append(pr["number"])

    if not releases:
        return None
    if len(commits) != 1 or len(releases) != 1:
        raise ValueError("mixed release update: " + ", ".join(attributed))
    return releases[0]


def update_metadata(event, root):
    repository = event["repository"]["full_name"]
    if repository != "Technosorcery/qso-io" or event["ref"] != "refs/heads/main":
        raise ValueError("Release metadata requires a Technosorcery/qso-io main push")
    if command("git", "rev-parse", "HEAD", cwd=root) != event["after"]:
        raise ValueError("Checkout does not match the push's after revision")
    with (root / "release-plz.toml").open("rb") as config_file:
        prefix = tomllib.load(config_file)["workspace"]["pr_branch_prefix"]
    if not prefix:
        raise ValueError("Release PR branch prefix must not be empty")

    commits = first_parent_commits(root, event["before"], event["after"])
    pulls = {}
    for sha in commits:
        pages = json.loads(command(
            "gh", "api", "--paginate", "--slurp",
            f"repos/{repository}/commits/{sha}/pulls", cwd=root,
        ))
        pulls[sha] = [pr for page in pages for pr in page]
    return classify_update(commits, pulls, repository, prefix)


def release_notes(changelog, version):
    """Select the reviewed release body, excluding neighboring version sections."""
    sections = []
    fence = None
    offset = 0
    for line in changelog.splitlines(keepends=True):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line.rstrip("\r\n"))
        if fence:
            if (marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence)
                    and not marker[2].strip()):
                fence = None
        elif marker:
            fence = marker[1]
        else:
            heading = re.match(
                r"^ {0,3}##[ \t]+\[([^]\r\n]+)\](?:\([^\r\n]*\))?(?:[ \t]+- .*)?[ \t]*$",
                line.rstrip("\r\n"),
            )
            if heading:
                sections.append((heading[1], offset, offset + len(line)))
        offset += len(line)

    matches = [index for index, section in enumerate(sections) if section[0] == version]
    if len(matches) != 1:
        raise ValueError(f"Expected one changelog section for {version}; found {len(matches)}")
    index = matches[0]
    end = sections[index + 1][1] if index + 1 < len(sections) else len(changelog)
    notes = changelog[sections[index][2]:end]
    if not notes.strip():
        raise ValueError(f"Changelog section for {version} is empty")
    return notes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="operation", required=True)
    update = subcommands.add_parser("update", help="Classify a main push")
    update.add_argument("event", type=Path, help="GitHub push event JSON")
    notes = subcommands.add_parser("notes", help="Extract reviewed notes for a version")
    notes.add_argument("version")
    notes.add_argument("output", type=Path)
    args = parser.parse_args()

    try:
        if args.operation == "notes":
            changelog = Path("CHANGELOG.md").read_bytes().decode("utf-8")
            args.output.write_text(release_notes(changelog, args.version), encoding="utf-8")
        else:
            release_pr = update_metadata(json.loads(args.event.read_text()), Path.cwd())
            with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
                output.write(f"eligible={'true' if release_pr is not None else 'false'}\n")
                output.write(f"release_pr={release_pr or ''}\n")
            print(f"Release authorized by PR #{release_pr}" if release_pr else "Ordinary update; no publication authorized")
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Release metadata failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
