# qso-io

Amateur radio data interchange with strict and permissive ADI parsing.

qso-io reads ADI, the length-delimited text representation of the Amateur Data
Interchange Format (ADIF). It provides borrowed sections, original input bytes
and source spans, configurable text decoding and length units, and exact typed
values. The standard field and reference definitions follow ADIF 3.1.8.

## Using the library

Requires Rust 1.88 or newer. Add qso-io to your dependencies:

```toml
[dependencies]
qso-io = "0.1.0"
```

Choose `StrictParser` for typed values and the supported ADIF checks, or
`PermissiveParser` for framed contents from loggers whose values may violate
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

See [ParseOpts](https://docs.rs/qso-io/0.1.0/qso_io/adif/adi/parser/options/struct.ParseOpts.html)
for parsing options and the
[ADI parser API](https://docs.rs/qso-io/0.1.0/qso_io/adif/adi/parser/index.html)
for field values and error handling. Build the documentation locally with:

```sh
cargo doc --no-deps --open
```

For standard-field and enumeration coverage, see
[ADIF 3.1.8 reference types](docs/adif-3.1.8-catalog.md).

## Reports and feedback

Bug reports, feature requests, and feedback are welcome. A small reproducible
example and a description of what you expected are especially helpful.

qso-io is maintained by Jacob Helwig and is not currently accepting outside code
or documentation patches. Please open an issue rather than preparing a pull
request. This keeps implementation and review work within the maintainer’s
available time.

## Development

Maintainers can find [Development checks](docs/development.md#development-checks)
and [Updating fixtures](docs/development.md#updating-fixtures).
See [Releasing qso-io](docs/releasing.md) for release review and recovery, and
[One-time release setup](docs/release-setup.md) for credentials, first publication,
and activation.

## License

qso-io is licensed under either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

The [Upstream ADI fixtures](tests/fixtures/README.md) retain their upstream
licenses and notices. Their [provenance manifest](tests/fixtures/provenance.json)
records the original sources and revisions.
