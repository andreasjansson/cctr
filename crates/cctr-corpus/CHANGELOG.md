# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
