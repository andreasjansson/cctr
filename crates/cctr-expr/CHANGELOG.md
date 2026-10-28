# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.32.0](https://github.com/andreasjansson/cctr/releases/tag/v0.32.0) - 2026-10-28

### Added

- `Display` for `Expr`, rendering it back to constraint syntax.
- `free_variables()`, `Expr::is_literal()` and `BinaryOp::is_comparison()`.

### Changed

- Access errors name the expression that failed. `KeyNotFound` and
  `IndexOutOfBounds` are now struct variants carrying the path, and
  `KeyNotFound` lists the keys that were available. Property and index access
  on the wrong type returns the new `InvalidAccess` instead of `TypeError`.

## [0.1.0](https://github.com/andreasjansson/cctr/releases/tag/v0.1.0) - 2026-01-07

### Other

- Add backslash string parsing test
- Add cctr-expr crate with expression parser and evaluator
- Add cctr-expr Cargo.toml
