# Upstream ADI fixtures

These 12 fixtures are unchanged upstream files from the pinned revisions in
[provenance.json](provenance.json). They keep their bytes, line endings,
spelling, framing mistakes, and encoding discrepancies. The `.adif`
extension on difa's sample still contains ADI text.

This directory supplies inputs for independently written tests. It contains no
parser implementation or executable test harness. The classifications below
describe inspected properties. They do not establish that every file passes
all ADIF semantic validation.

## Licenses and provenance

The third-party files keep their upstream licenses. The project's dual license
does not replace them. For redistribution of these fixtures, keep the
applicable notices. These files have no established public-domain or CC0
status.

| Directory | Upstream at pinned revision | License copy |
| --- | --- | --- |
| `matir-adifparser` | [Matir/adifparser](https://github.com/Matir/adifparser/tree/2c465737f437b70867819bb986aa79722057bd1c) | [BSD-2-Clause](matir-adifparser/COPYING) |
| `difa` | [cammeresi/adif](https://github.com/cammeresi/adif/tree/63783b458d52026d944af49594a7d2a5a397cdb8) | [BSD-3-Clause](difa/LICENSE) |
| `adif-parser-ts` | [k0swe/adif-parser-ts](https://github.com/k0swe/adif-parser-ts/tree/a77ad183a79148749f9d9d21bd5f010fcd2dcf3f) | [Apache-2.0](adif-parser-ts/LICENSE) |
| `transadif` | [ham2k/transadif](https://github.com/ham2k/transadif/tree/90586ecf600623a51b560a2cfa3a443b3d9c8d56) | [MIT](transadif/LICENSE) |

The provenance manifest records every fixture and license file's repository,
commit, source path, raw download URL, upstream Git blob ID, and byte count.
Files live under a source-specific directory with their upstream paths intact.
The selected upstream trees contain no separate NOTICE file. This directory
includes full license texts, not just abbreviated license identifiers.

## Fixture classification

Paths below are relative to their source-specific directory. ASCII inputs have
the same boundaries under byte and Unicode-scalar length settings.

| Source and file | Classification | Useful properties and caveats |
| --- | --- | --- |
| Matir `testdata/header_none.adi` | Complete framing, ASCII | Headerless single record. `MYCALL` is not a standard-field recognition requirement. |
| Matir `testdata/header_comment.adi` | Complete framing, ASCII | Preamble-only header followed by one record. |
| Matir `testdata/header_version.adi` | **Malformed structural input** | `<mycall>` has no length. The file starts directly with `<adif_ver:4>`, then supplies EOH. The standard header form starts with a character other than `<`. Do not use the filename as proof of correct framing. |
| Matir `testdata/readrecord.adi` | **Malformed: unterminated final record** | Two complete records precede `<mycall:4>W1AW` without EOR. The file supplies a reliable prefix and an unfinished final record for tests. |
| difa `examples/sample.adif` | Complete framing, incorrect standard MODE | A header and three framed records. The third supplies Q65 as MODE, but ADIF defines it as a submode. Typed parsing returns the first two records, then rejects Q65. The input keeps four- and six-digit time spellings. |
| TypeScript `test/data/wsjtx.adi` | Complete framing, ASCII | WSJT-X export, exact decimal frequency text, six-digit times and signal reports. |
| TypeScript `test/data/js8call.adi` | Complete framing, ASCII | JS8Call export with mode/submode and explicit zero-length GRIDSQUARE/RST_RCVD fields. |
| TransADIF `test-cases/01-plain-examples/1-plain-ascii-in.adi` | Complete framing, ASCII | Header, CRLF line endings, decimal frequency and minute-precision time. |
| TransADIF `test-cases/01-plain-examples/2-plain-iso-in.adi` | **Latin-1 compatibility input** | The name bytes include `E1` and `F1`. They are not valid UTF-8. NAME has 14 input bytes. Use explicit Latin-1, not the decoded preview from a file API. Non-ASCII text is outside strict ASCII ADI. |
| TransADIF `test-cases/01-plain-examples/3-plain-utf-in.adi` | **UTF-8 compatibility input with scalar-counted NAME** | NAME declares 14. Its intended line value has 14 scalars and 16 UTF-8 bytes. With byte lengths, the framed value ends at `Juán Muñoz U`. The text `TF` remains outside the field. Do not silently change the length interpretation. A difference from the intended text does not establish a structural error. |
| TransADIF `test-cases/02-field-length/1-undercount-in.adi` | **Mixed compatibility and invalid field content** | UTF-8 NAME declares 18, the length of its 18 bytes/16 scalars in byte mode. NOTES and COMMENT each declare 35. Their line text has 34 bytes, so each consumes a lone CR. A lone CR violates multiline and single-line field rules. Byte framing remains reliable. See details below. |
| TransADIF `test-cases/02-field-length/2-overcount-in.adi` | **Misleading encoding label, length-mode-dependent input** | The preamble calls this ISO-8859-1, but the actual name uses UTF-8. NAME declares 24, the length of its 24 bytes/21 scalars in byte mode. Scalar mode consumes the following CRLF and the opening `<` of the next descriptor. It is not a valid scalar-counted example of the intended fields. |

## Malformed and compatibility cases

The Matir missing-length and unfinished-record cases are structural negatives.
Use explicit encoding and length units for the TransADIF files. Their upstream
names and explanatory preambles supply evidence. They do not define the
expected test results.

In `1-undercount-in.adi`, byte mode frames NAME as intended, but the trailing
lone CR in NOTES/COMMENT is a field-content problem. Under UTF-8 scalar
lengths, NAME consumes its following CRLF as well. Keep these distinctions
separate from malformed descriptor, truncated-value and unfinished-record
errors.

In `2-overcount-in.adi`, the NAME bytes contain `C3 A1`,
`C3 B1`, and `C3 AD`. These sequences differ from Latin-1's
corresponding single bytes. Byte mode keeps the name. Scalar mode includes the
next descriptor's opening delimiter. The grammar determines whether the parser
can read the text between subsequent fields. Do not infer a mandatory
structural rejection or recovery rule from the filename.

With standard field typing enabled, byte-framed undercount input fails on the
lone CR in NOTES. Scalar-framed undercount/overcount input fails when NAME
includes CRLF. These are located typed-value failures, not length truncation.
Framing tests inspect the counted input bytes independently.

TransADIF repairs encodings and field counts. This directory excludes its
expected-output files because those files encode behavior outside the parser
contract here. It also excludes mojibake/entity-repair cases.

The source inspection did not establish redistribution terms for the official
ADIF generated corpus. This directory therefore excludes that corpus. A
generator's software license does not establish a license for its separately
distributed output.

## Byte preservation and verification

Download from the pinned raw URLs in the manifest. Do not use a rendered page
or a text API that can transcode bytes. Do not normalize newlines. Do not
repair fixture contents in place. If changes are necessary, add separately
named derived cases. Record their provenance. Explain the changes.

The nested `.gitattributes` disables Git text normalization for fixture and
upstream license files. Before a source update, compare each raw file's byte
count and Git blob ID with its pinned upstream tree entry. For example, from
the repository root:

```sh
git hash-object --no-filters tests/fixtures/transadif/test-cases/01-plain-examples/2-plain-iso-in.adi
```

The result for that file is `791ccc7781ad0206f14a74a3f4eec01c7fd39422`. This
checks identity independently of any parser implementation.
