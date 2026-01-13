# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1](https://github.com/andreasjansson/cctr/compare/v0.2.0...v0.2.1) - 2026-01-13

### Other

- update Cargo.lock dependencies

## [0.1.0](https://github.com/andreasjansson/cctr/releases/tag/v0.1.0) - 2026-01-07

### Other

- Add libc dependency for SIGPIPE handling
- Handle SIGPIPE gracefully to avoid crash when piped to head/tail
- Fix clippy: collapse if and use is_none_or
- Box TestResult when creating ProgressEvent::TestComplete
- Fix clippy: Box large enum variant
- Fix clippy: use is_none_or and is_some_and instead of map_or
- Fix clippy: use is_some_and instead of map_or
- Show relative paths in test failure output
- Fix suite naming: use directory name when suite is at root
- Clean up stale files, add missing files, track Cargo.lock
- Update run_suite to pass pattern to run_corpus_file
- Filter by test name pattern instead of file name
- Update list_tests signature and remove unused parse_filter
- Pass pattern to run_suite instead of file_filter
- Update main.rs for new CLI: test_root and pattern
- Change CLI: positional TEST_ROOT, -p/--pattern for filtering
- Add test_count() method to Suite
- Count individual tests for skipped suites in summary
- Remove debug regex test
- Remove debug output from matcher
- Add debug output to matcher
- Add test for empty string variable parsing
- Add debug test for regex matching
- Add unit test for empty string matching
- allow empty strings in pattern matching
- Fix fixture detection to only check relative path from root
- Fix test helper to use VarType enum
- Fix VarType pattern matching in extract_values
- Fix VarType pattern matching in build_regex
- Fix matcher to use VarType enum pattern matching
- Add matcher module back to cctr lib.rs
- Add matcher module to cctr using cctr-expr for constraints
- Update cctr dependencies to use cctr-expr and cctr-corpus
- Add cctr-parsers dependency to cctr crate
- Update cctr lib.rs to use cctr-match and cctr-parsers
- Add cctr crate Cargo.toml
