# Changelog

This file records notable changes to the project.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0](https://github.com/Technosorcery/qso-io/releases/tag/v0.1.0) - 2026-11-17

### Added

- Strict and permissive parsers for ADI headers and QSO records.
- ASCII, ISO-8859-1 (Latin-1), and UTF-8 decoding, with field lengths
  interpreted as bytes or Unicode scalar values.
- Typed field values with arbitrary-precision integers and exact decimal
  numbers.
- Strict parsing using ADIF 3.1.8 field definitions, USERDEF types and
  constraints, and application-field type consistency.
- Input bytes, source spans, and case-insensitive field lookup with ambiguity
  diagnostics.
- Permissive parsing that keeps duplicate fields and decoding errors when
  field boundaries are known.

Minimum Rust version: 1.88. The license is MIT OR Apache-2.0.
