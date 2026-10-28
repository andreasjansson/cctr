# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.32.0](https://github.com/andreasjansson/cctr/releases/tag/v0.32.0) - 2026-10-28

### Added

- `#` comments in the file header.
- A bare separator with no `where` clause explicitly ends an expected block.
- `TestCase::expected_start_line` and `TestCase::expected_line_count`.

### Fixed

- Content that is not a test case is a parse error naming the line, instead of
  being silently dropped along with the rest of the file.
- Nested parentheses in `%skip(...)` reasons, with precise errors for
  unterminated or malformed directives.
- Line numbers are derived from the parse position and no longer drift.

## Before 0.32.0

Released at some point between 0.1.0 and 0.31.0; this changelog was not kept
up to date for those releases.

### Added

- Adjustable delimiter length: use more `=` or `-` characters (e.g., `====`/`----`) when your content contains the standard 3-character delimiters
- Helpful error messages for delimiter length mismatches

## [0.1.0](https://github.com/andreasjansson/cctr/releases/tag/v0.1.0) - 2026-01-07

### Other

- Remove unused position tracking variables
- Fix lifetime annotation in expected_line
- Fix lifetime annotations in cctr-corpus parsers
- Fix lifetime and unused import issues in cctr-corpus
- Add cctr-corpus crate with winnow-based test file parser
- Add cctr-corpus Cargo.toml
