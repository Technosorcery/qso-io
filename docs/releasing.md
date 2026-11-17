# Releasing qso-io

Release-plz prepares version and changelog proposals after ordinary pushes to
`main`. Review the release PR and merge it through the queue using merge commits.
Publication uses the existing post-merge CI results and publishes, tags, and
creates a GitHub release from that run's exact revision.

For access, repository rules, manual first publication, and activation, see
[One-time release setup](release-setup.md).

## Review the release PR

Review the proposed version against the public changes, API compatibility
report, and supported Rust version. Confirm that the version and release notes
account for intentional breaking changes. Edit the generated changelog as needed;
its entry for the package version supplies the GitHub release notes.

The release PR and queue candidate must pass the five required checks. The
four test jobs cover stable Rust on Linux, Windows, and macOS, plus Rust 1.88 on
Linux, with all-target tests and doctests. Documentation and package checks
cover release metadata handling, formatting, Clippy, strict rustdoc, publication
dry-run, package construction, and tests from its extracted archive. Editing
version, notes, or other files requires checks for the updated revision.

Release-plz 0.4.3 updates bot-only preparation branches in place. If a maintainer
adds commits, it closes the old PR and opens a new proposal to preserve that
history. Inspect the closed PR and reapply intended version/note edits before
reviewing the new proposal. Use `release-plz set-version <version>` on a release
branch to adjust both its manifest and changelog.

## Authorize publication

Merge the release PR through the native queue using merge commits. Release-plz
preparation commits may have a title only.

Let the release PR land by itself. If the same update to `main` includes another
PR, the Classify release update job fails, regardless of the PRs' order. No
registry upload, tag, or GitHub release is created. Investigate the update and
prepare a newly reviewed, isolated release authorization; rerunning the mixed
update does not make it eligible.

An ordinary update without a release PR skips publication. An isolated release
PR merge authorizes publication, subject to all five existing post-merge checks
passing and automatic publication being enabled. A failed, cancelled, or skipped
check prevents publication. A later push cannot change the authorized source.

## Monitor the release

Watch the CI run for the release merge, including Classify release update and
Publish release. Publication reads the version and reviewed changelog entry
from that checked revision. It checks existing registry/tag/release state, obtains
a short-lived Trusted Publishing credential, and runs `cargo publish --locked`.
Only after a successful upload does it create `v<version>` at the checked SHA
and create the GitHub release with the reviewed notes.

Confirm the version on crates.io, the tag's target, and the GitHub release notes.
The next preparation job waits for successful publication and GitHub finalization
on a release-authorizing push. If publication fails or is disabled, preparation
on that run is skipped so it does not replace the pending release proposal.

## Recover from a failed release

Preserve the authorized revision and version. Inspect the failed operation,
registry version, remote tag, and GitHub release before retrying. Registry
publication and GitHub finalization are separate operations; an upload timeout
can occur after the registry accepted the crate.

- If verification failed, correct the problem through a reviewed change. A new
  source revision requires its own checks and release authorization.
- If upload did not occur, repair credentials or transport and rerun the failed
  publication job on the original authorized revision, retaining its successful
  CI evidence. Preflight requires the registry version, tag, and GitHub release
  to be absent; existing artifacts require inspection and recovery instead.
- If the crate exists, do not repeat the upload or bump the version merely to
  rerun. Verify its source using the downloaded crate's `.cargo_vcs_info.json`
  and packaged source against the authorized revision. Complete only missing
  GitHub operations after establishing that identity.
- If the tag exists, verify its commit; never move or recreate it. If the GitHub
  release exists, inspect its tag and notes before deciding what remains.

Useful inspection commands, substituting the reviewed version:

```sh
git ls-remote --tags origin 'refs/tags/v0.1.0' 'refs/tags/v0.1.0^{}'
gh release view v0.1.0 --repo Technosorcery/qso-io
```

For a missing tag, create it explicitly at the verified source SHA using ordinary
Git/GitHub tooling. For a missing GitHub release, use the verified existing tag
and the reviewed changelog entry saved to a file:

```sh
gh release create v0.1.0 --repo Technosorcery/qso-io --verify-tag \
  --notes-file /path/to/reviewed-release-notes.md
```

The workflow deliberately stops on existing release artifacts rather than
attempting automatic repair. Do not publish from newer `main` or bypass a
mixed-batch failure during recovery.

## References

- [Release-PR refresh](https://release-plz.dev/docs/usage/release-pr#pr-update)
- [Version adjustment](https://release-plz.dev/docs/usage/set-version)
- [Cargo publication](https://doc.rust-lang.org/cargo/commands/cargo-publish.html)
- [GitHub release creation](https://cli.github.com/manual/gh_release_create)
