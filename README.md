# qso-io

Amateur radio data interchange with strict and permissive ADI parsing.

qso-io reads ADI, the length-delimited text representation of the Amateur Data
Interchange Format (ADIF). It provides borrowed sections, input bytes, and
source spans. Callers can select text decoding and length units. Typed values
keep exact numeric values. The standard field and reference definitions follow
ADIF 3.1.8.

## Using the library

Use Rust 1.88 or newer. Add qso-io to your dependencies:

```toml
[dependencies]
qso-io = "0.1.0"
```

Use `StrictParser` for typed values and the supported ADIF checks. Use
`PermissiveParser` for framed contents from loggers whose values can violate
ADIF definitions.

```rust
use qso_io::adif::adi::parser::options::ParseOpts;
use qso_io::adif::adi::parser::permissive::{PermissiveParser, PermissiveSection};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut parser = PermissiveParser::new(b"<K_INDEX:4>1.33<EOR>", ParseOpts::default());
    let record = match parser.next() {
        Some(Ok(PermissiveSection::Record(record))) => record,
        other => panic!("expected the supplied complete record, got {other:?}"),
    };
    let field = record.field("K_INDEX")?
        .expect("record should contain K_INDEX because the input supplies that descriptor");
    assert_eq!(field.text().map_err(Clone::clone)?, "1.33");
    assert_eq!(field.raw_data(), b"1.33");
    Ok(())
}
```

See
[ParseOpts](https://docs.rs/qso-io/0.1.0/qso_io/adif/adi/parser/options/struct.ParseOpts.html)
for parsing options and the
[ADI parser API](https://docs.rs/qso-io/0.1.0/qso_io/adif/adi/parser/index.html)
for field values and error handling. Build the documentation locally with:

```sh
cargo doc --no-deps --open
```

For standard-field and enumeration coverage, see
[ADIF 3.1.8 reference types](docs/adif-3.1.8-catalog.md).

## Reports and feedback

Use GitHub issues for bug reports, feature requests, and feedback. Include a
small example that reproduces the problem. Describe the result that you
expected.

Jacob Helwig maintains qso-io. The project does not accept outside code or
documentation patches. Open an issue as an alternative to a pull request. This
policy keeps implementation and review work within the maintainer’s available
time.

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
