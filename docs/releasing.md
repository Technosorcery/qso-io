# Releasing qso-io

Release-plz prepares version and changelog proposals after ordinary pushes to
`main`. Examine the release PR. Merge it through the queue with a
merge commit. Publication uses the existing post-merge CI results. It
publishes the crate, creates its tag, and creates a GitHub release from that
run's exact revision.

For access, repository rules, manual first publication, and activation, see
[One-time release setup](release-setup.md).

## Review the release PR

Compare the proposed version with the public changes, API compatibility
report, and supported Rust version. Make sure that the version and release
notes account for intentional breaking changes. Change the generated changelog
as necessary. Its entry for the package version supplies the GitHub release
notes.

The release PR and queue candidate must pass the five required checks. The
four test jobs run all-target tests and doctests on these platforms:

- Stable Rust on Linux
- Stable Rust on Windows
- Stable Rust on macOS
- Rust 1.88 on Linux.

The Documentation and package job checks release metadata, formatting, Clippy,
and strict rustdoc. It also checks a publication dry-run, package
construction, and tests from the extracted archive. After edits to the
version, notes, or other files, run the checks for the updated revision.

Release-plz 0.4.3 updates preparation branches that contain only bot commits.
If a maintainer adds commits, release-plz closes the old PR. It opens a new
proposal to keep that history.

Before review of the new proposal, examine the closed PR. Apply the intended
version and note edits again. Use `release-plz set-version <version>` on a
release branch to adjust both its manifest and changelog.

## Authorize publication

Merge the release PR through the native queue using merge commits. Release-plz
preparation commits can have a title only.

Merge the release PR without other PRs in the same update to `main`.
If the same update to `main` includes another PR, the Classify
release update job fails, regardless of the PRs' order. The workflow creates
no registry upload, tag, or GitHub release.

Investigate the update. Prepare an isolated release authorization for a new
review. Another run of the mixed update cannot authorize publication.

An ordinary update without a release PR skips publication. An isolated release
PR merge authorizes publication when automatic publication is enabled and all
five existing post-merge checks pass. A failed, canceled, or skipped check
prevents publication. A later push cannot change the authorized source.

## Monitor the release

Watch the CI run for the release merge, with Classify release update and
Publish release. Publication reads the version and reviewed changelog entry
from that checked revision. It checks the existing registry version, tag, and
GitHub release. It then gets a short-lived Trusted Publishing credential and
runs `cargo publish --locked`. Only after a successful upload does it create
`v<version>` at the checked SHA and create the GitHub release with the
reviewed notes.

Examine the version on crates.io, the tag's target, and the GitHub release
notes. On a release-authorizing push, the next preparation job waits for
publication and GitHub finalization to succeed. If publication fails or is
disabled, that run skips preparation. This keeps the pending release proposal.

## Recover from a failed release

Keep the authorized revision and version. Examine the failed operation,
registry version, remote tag, and GitHub release before another attempt.
Registry publication and GitHub finalization are separate operations. An
upload timeout can occur after the registry accepted the crate.

- If verification failed, correct the problem through a reviewed change. A new
  source revision must have its own checks and release authorization.
- If upload did not occur, repair credentials or transport. Rerun the failed
  publication job on the authorized revision. Keep its successful CI evidence.
  The preflight checks permit a new upload only if the registry version, tag,
  and GitHub release are absent. If these artifacts exist, examine them before
  recovery.
- If the crate exists, do not repeat the upload or increase the version just
  to rerun publication. Compare the downloaded crate's `.cargo_vcs_info.json`
  and packaged source with the authorized revision. After you make sure that
  the sources agree, complete only the missing GitHub operations.
- If the tag exists, compare its commit with the authorized revision. Never
  move or create that tag again. If the GitHub release exists, examine its tag
  and notes. Use that information to identify the remaining operations.

Useful inspection commands, substituting the reviewed version:

```sh
git ls-remote --tags origin 'refs/tags/v0.1.0' 'refs/tags/v0.1.0^{}'
gh release view v0.1.0 --repo Technosorcery/qso-io
```

For a missing tag, create it explicitly at the verified source SHA using
ordinary Git/GitHub tooling. For a missing GitHub release, use the verified
existing tag and the reviewed changelog entry saved to a file:

```sh
gh release create v0.1.0 --repo Technosorcery/qso-io --verify-tag \
  --notes-file /path/to/reviewed-release-notes.md
```

The workflow stops when release artifacts exist. It does not repair them
automatically. Do not publish from newer `main` or bypass a
mixed-batch failure during recovery.

## References

- [Release-PR refresh](https://release-plz.dev/docs/usage/release-pr#pr-update)
- [Version adjustment](https://release-plz.dev/docs/usage/set-version)
- [Cargo publication](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)
- [GitHub release creation](https://cli.github.com/manual/gh_release_create)
