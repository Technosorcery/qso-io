# One-time release setup

This guide configures release automation for qso-io. For recurring operations,
see [Releasing qso-io](releasing.md).

## GitHub App access

Create or reuse a GitHub App. Give it Contents and Pull requests read/write
access. Install it for `Technosorcery/qso-io`. The token-based preparation
workflow operates without a webhook service. Keep collaborator-only PR
creation enabled.

In qso-io's Actions settings, configure:

| Kind | Name | Value |
| --- | --- | --- |
| Repository variable | `RELEASE_APP_CLIENT_ID` | The App's client ID |
| Repository secret | `RELEASE_APP_PRIVATE_KEY` | The App's PEM private key |

The preparation job gets a repository-scoped installation token with those
credentials. Publication uses its job's `GITHUB_TOKEN` for GitHub operations
and Trusted Publishing for Cargo, rather than the preparation App token.

Observe a preparation run after an ordinary push to main. Examine its
App-authored release PR and generated changelog. Make sure that the App can
create the PR under collaborator-only access. Make sure that all five checks
run without a separate workflow-approval click.

## Required repository checks

Use an active ruleset for `main` with no bypasses. Make PRs, the
native merge queue, and these five GitHub Actions checks mandatory:

- `Tests (ubuntu-latest, Rust stable)`
- `Tests (windows-latest, Rust stable)`
- `Tests (macos-latest, Rust stable)`
- `Tests (ubuntu-latest, Rust 1.88.0)`
- `Documentation and package`

Use the actual observed check identities and GitHub Actions as their expected
source. Leave required reviewers disabled. Select the queue's **Merge**
method. Permit merge commits in repository settings. Leave linear history
disabled.

Queue checks run through the workflow's `merge_group` trigger. Publication
uses successful test and quality jobs from its existing post-merge
`main` push run. It does not run those checks again.

Queue checks test integration with current `main` without requiring
authors to update their branches. Grouping and build limits do not define
release eligibility. A release update with another PR fails publication as
described in [Authorize publication](releasing.md#authorize-publication).

## Manual first publication

Keep `RELEASE_PUBLISH_ENABLED` unset or `false` during setup. The
read-only Classify release update job still runs, while registry and GitHub
release writes remain disabled.

Let release-plz generate the initial changelog in its release PR. Examine the
proposed 0.1.0 version and notes. Adjust them as necessary. Merge only that PR
through the queue. Make sure that all five existing checks pass on its merged
revision. Preparation on that release run waits while publication remains
disabled.

The first publication must have separate release authorization. After
authorization, do these steps:

1. Check out the exact verified merged SHA.
2. Examine the package contents.
3. Publish with the maintainer's local Cargo credentials.

Use these commands from the verified checkout:

```sh
cargo package --list --locked
cargo publish --locked
```

Make sure that the version exists on crates.io. Create its `v<version>`
tag at that SHA. Create the GitHub release with the reviewed notes. A later
ordinary push can resume preparation against the published baseline.

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

Compare the registration with the deployed workflow. Make sure that the manual
first publication and setup are correct. Then explicitly authorize activation.
Set `RELEASE_PUBLISH_ENABLED` to the exact string `true`.

An ordinary activation push does not authorize publication. The next isolated
release PR merge authorizes publication.

For the first automated publication, follow
[Monitor the release](releasing.md#monitor-the-release).

## References

- [Release-plz App integration](https://release-plz.dev/docs/github/token#use-a-github-app)
- [GitHub App installation tokens](https://github.com/actions/create-github-app-token)
- [Native merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
- [Crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing)
- [Crates.io authentication action](https://github.com/rust-lang/crates-io-auth-action)
