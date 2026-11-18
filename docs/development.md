# Maintainer development

## Development checks

Use a current stable Rust toolchain with rustfmt and Clippy installed:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo test --doc --locked
RUSTDOCFLAGS="-D warnings -D missing_docs" cargo doc --no-deps --locked
```

The current dependency tree requires Rust 1.88. CI tests that toolchain.
Run the same checks locally:

```sh
rustup toolchain install 1.88.0 --profile minimal
cargo +1.88.0 test --all-targets --locked
cargo +1.88.0 test --doc --locked
```

CI runs parser tests on Linux, Windows, and macOS. It also tests Rust 1.88 on
Linux. The Linux checks cover formatting, Clippy, documentation, and package
verification. Package verification includes tests from an extracted archive.

## Updating fixtures

Find source URLs, licenses, and input classifications in
[Upstream ADI fixtures](../tests/fixtures/README.md). Follow
[Byte preservation and verification](../tests/fixtures/README.md#byte-preservation-and-verification)
for fixture updates.

## Releases

Follow [Releasing qso-io](releasing.md) for release review, publication, and
recovery. Follow [One-time release setup](release-setup.md) to configure
release access and first publication.
