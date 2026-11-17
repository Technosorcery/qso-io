# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/Technosorcery/qso-io/releases/tag/v0.1.0) - 2026-11-17

### Added

- Strict and permissive parsers for ADI headers and QSO records.
- ASCII, ISO-8859-1 (Latin-1), and UTF-8 decoding, with field lengths interpreted as bytes or Unicode scalar values.
- Typed field values, including arbitrary-precision integers and exact decimal numbers.
- Strict parsing using ADIF 3.1.8 field definitions, USERDEF types and constraints, and application-field type consistency.
- Original input bytes, source spans, and case-insensitive field lookup with ambiguity diagnostics.
- Permissive parsing that preserves duplicate fields and decoding errors when field boundaries are known.

Requires Rust 1.88 or newer. Licensed under MIT OR Apache-2.0.
