# One-time release setup

This guide configures release automation for qso-io. For recurring operations,
see [Releasing qso-io](releasing.md).

## GitHub App access

Create or reuse a GitHub App with Contents and Pull requests read/write access.
Install it for `Technosorcery/qso-io`. No webhook service is required for the
token-based preparation workflow. Keep collaborator-only PR creation enabled.

In qso-io's Actions settings, configure:

| Kind | Name | Value |
| --- | --- | --- |
| Repository variable | `RELEASE_APP_CLIENT_ID` | The App's client ID |
| Repository secret | `RELEASE_APP_PRIVATE_KEY` | The App's PEM private key |

The preparation job obtains a repository-scoped installation token using those
credentials. Publication uses its job's `GITHUB_TOKEN` for GitHub operations and
Trusted Publishing for Cargo, rather than the preparation App token.

Observe an ordinary main-push preparation run and its App-authored release PR,
including the generated changelog. Confirm the App can create the PR under
collaborator-only access and that all five checks run without a separate
workflow-approval click.

## Required repository checks

Use an active ruleset targeting `main`, with no bypasses, requiring PRs, the
native merge queue, and these five GitHub Actions checks:

- `Tests (ubuntu-latest, Rust stable)`
- `Tests (windows-latest, Rust stable)`
- `Tests (macos-latest, Rust stable)`
- `Tests (ubuntu-latest, Rust 1.88.0)`
- `Documentation and package`

Use the actual observed check identities and GitHub Actions as their expected
source. Leave required reviewers disabled. Select the queue's **Merge** method,
allow merge commits in repository settings, and leave linear history disabled.
Queue checks run through the workflow's `merge_group` trigger. Publication uses
successful test and quality jobs from its existing post-merge `main` push run;
it does not add another execution of those checks.

Queue checks test integration with current `main` without requiring authors to
update their branches. Grouping/build limits do not define release eligibility;
a release update containing another PR fails publication as described in
[Authorize publication](releasing.md#authorize-publication).

## Manual first publication

Keep `RELEASE_PUBLISH_ENABLED` unset or `false` during setup. The read-only
Classify release update job still runs, while registry and GitHub release writes
remain disabled.

Let release-plz generate the initial changelog in its release PR. Review the
proposed 0.1.0 version and notes, adjusting them as needed. Merge only that PR
through the queue and require all five existing checks on its merged revision.
Preparation on that release run waits while publication remains disabled.

The first publication needs separate release authorization. Once authorized,
check out the exact verified merged SHA, inspect the package contents, and
publish using the maintainer's local Cargo credentials:

```sh
cargo package --list --locked
cargo publish --locked
```

Confirm the version on crates.io, then create its `v<version>` tag at that SHA
and GitHub release with the reviewed notes. A later ordinary push can resume
preparation against the published baseline.

## Trusted Publishing and activation

Once the crate exists, register its GitHub Trusted Publisher on crates.io:

| Field | Value |
| --- | --- |
| Repository owner | `Technosorcery` |
| Repository | `qso-io` |
| Workflow filename | `ci.yml` |
| Environment | Leave unset |

The publication job has `id-token: write` for the standard crates.io
authentication action. That action supplies a short-lived token to Cargo and
revokes it when the job ends. Store no long-lived registry token in Actions.

Verify registration matches the deployed workflow. After the manual first
publication and setup are verified, explicitly authorize activation and set
`RELEASE_PUBLISH_ENABLED` to the exact string `true`. An ordinary activation push
does not authorize publication; the next isolated release PR merge does.

For the first automated publication, follow
[Monitor the release](releasing.md#monitor-the-release).

## References

- [Release-plz App integration](https://release-plz.dev/docs/github/token#use-a-github-app)
- [GitHub App installation tokens](https://github.com/actions/create-github-app-token)
- [Native merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
- [Crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing)
- [Crates.io authentication action](https://github.com/rust-lang/crates-io-auth-action)
