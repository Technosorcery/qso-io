# ADIF 3.1.8 reference types

The maintained Rust definitions follow the released
[ADIF 3.1.8 specification](https://www.adif.org.uk/318/ADIF_318.htm), dated
2026-10-12. The Rust definitions and their documentation define the crate's
reference data. Official JSON exports supply independent validation inputs.

## Field and member identity

`adif::StandardField` describes ordinary header and QSO fields from
III.C.1.a/b. It gives canonical field names, sections, data types, and
applicable enumerations. The parser uses those definitions directly. USERDEFn
uses a numeric name grammar. Application-defined and user-defined fields keep
their document rules.

Reference types under `adif` describe the enumerations in III.B.
Named enums represent antenna paths, confirmation states, modes, awards, and
geographic codes. Primary subdivisions have separate country-specific enums
under `adif::primary_subdivisions`. Each enum follows a DXCC group in
III.B.12. Each variant has a documented meaning and canonical ADIF spelling.

Strum derives code parsing and iteration from those variants. The
`AntPath`, `ArrlSection`, and `Award` types participate in
parser membership checks. There is no parallel membership string catalog.

Reference parsing and parser membership use ASCII case-insensitive identity
without trimming. ADI values keep their spelling independently of reference
types. They use `Value::Text` and `Value::List` representations. ARRL
sections include historical dates and multiple DXCC associations.
`dxcc_entity_codes()` returns all associated entity codes.

## Coverage policy

Secondary subdivisions dispatch comma-separated county codes by their explicit
state prefix. The `AK` branch parses the county name with
`AlaskaCounty`. `HI` keeps a nonempty county name. Other
prefixes use the United States primary-subdivision enum. A nested failure is
terminal within the selected branch. The leaf enum's codes contain only county
names, without the state prefix.

For codes without commas, the parser constructs `SecondarySubdivision`
candidates whose formats match groups in III.B.22. Formats can overlap. The
parser accepts any successful candidate without a country assignment. Japanese
city, ku, gun, and combined AJA forms share the Japan variant. The parser does
not check external membership. Inputs must match a published code format.

Alternate subdivisions dispatch the name before `:`.
`NZ_Regions` parses the region/district pair through `NzRegionDistrict`. A
nested failure cannot select another enumeration. The parser keeps other
nonempty names and codes as `SecondarySubdivisionAlt::Other`. List framing and
duplicate-name checks remain in the list parser. The leaf enum contains
region/district codes without the `NZ_Regions:` prefix. The parser does not
check DOK membership.

`SecondarySubdivision::candidates` and `SecondarySubdivisionAlt::parse_member`
perform these checks without a selection of the field's stored representation.
ADI values remain text or top-level text lists with their supplied spelling.
Standard field definitions dispatch membership checks directly to reference
types. Parsing failures include member-set names. Compound parsers check their
component types directly. Externally assigned codes can pass format checks
without membership in the reference tables. These checks use only the formats
that ADIF supplies.

Primary-subdivision membership tries the country-specific enum parsers. It
accepts a code if any group recognizes it. It operates without a DXCC field.
It does not identify or infer a country. The record's country/subdivision
relationship remains a separate validation concern. Historical entries remain
members independently of QSO-date applicability.

For sponsored awards, the parser checks ADIF-supplied sponsor identifiers and
compound grammar. `AwardSponsor` represents the sponsor component, such as
`ARRL`. The compound parser consumes the `_` separators
in `SPONSOR_PROGRAM_AWARD`. External sources assign programs and awards.

String-field enumeration recommendations do not replace the String data type.
ADIF_VER is metadata and does not select another catalog.

## Independent validation inputs

The official resource archive supplies `fields.json` and
`enumerations.json`. These files permit checks of maintained names, type
associations, and membership against the specification. They do not create
Rust types or their documentation. The specification's prose and source limits
also apply to these checks.

The released exports have these hashes of their unchanged bytes:

- fields.json:
  `09ed6c7afa89420ccadcd60bd319d1c01cf86c2728264e2e4f0acf14a2bf6033`
- enumerations.json:
  `7a161acdec46e19e058cb626f44b2f8733608fb82546549b9cb7b04b7dca8054`

The exports are available from the
[official resources](https://www.adif.org.uk/318/ADIF_318_resources.htm).
