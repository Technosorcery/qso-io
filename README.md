# qso-io

qso-io is a Rust library for the Amateur Data Interchange Format (ADIF).
It reads ADI text and ADX XML with strict and permissive parsers, and writes
ADI text from structured headers and records. The field definitions follow
ADIF 3.1.8.

## Install

The current dependencies require Rust 1.88 or later.
Add qso-io to your project:

```sh
cargo add qso-io
```

## Documentation

See the [API documentation](https://docs.rs/qso-io) for the guide and examples.
To open the documentation for this checkout, run:

```sh
cargo doc --no-deps --open
```

For standard fields and enumerations, see
[ADIF 3.1.8 reference types](docs/adif-3.1.8-catalog.md).

## Reports and feedback

Use GitHub issues for bug reports, feature requests, and feedback. Include a
small example that reproduces the problem. Describe the result that you
expected.

Jacob Helwig maintains qso-io. The project does not accept outside code or
documentation patches. Open an issue instead of a pull request.
This policy keeps implementation and review work within the maintainer’s
available time.

## Development

Maintainers can find
[Development checks](docs/development.md#development-checks) and
[Updating fixtures](docs/development.md#updating-fixtures). See
[Releasing qso-io](docs/releasing.md) for release review and recovery, and
[One-time release setup](docs/release-setup.md) for credentials, first
publication, and activation.

## License

You can use qso-io under either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE).

The [Upstream ADI fixtures](tests/fixtures/README.md) keep their upstream
licenses and notices. Their
[provenance manifest](tests/fixtures/provenance.json) records the sources and
revisions.
