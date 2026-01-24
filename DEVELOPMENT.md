# Development

## Quick Start

```bash
./script/test      # Run unit tests and corpus tests
./script/install   # Build and install
```

## Test Structure

cctr uses cctr to test itself. The test suite lives in `test/` and has a specific structure that can be confusing at first.

### The Two-Level Pattern

Most test suites follow a two-level pattern:

```
test/
  feature_name/
    feature_name.txt      # Top-level: calls cctr on fixtures
    fixture/
      tests/              # Inner level: actual cctr test cases
        test_file.txt
```

**Top-level tests** (`test/feature_name/feature_name.txt`) are corpus tests that:
- Call `cctr` as a command
- Run cctr against the fixtures
- Verify cctr's output (pass/fail counts, error messages, etc.)

**Fixture tests** (`test/feature_name/fixture/tests/*.txt`) are:
- Actual cctr corpus test files
- What cctr runs when the top-level test executes
- May have their own fixtures (nested `fixture/` directories)

### Example: json_types

```
test/json_types/
  json_array.txt                    # Top-level: tests cctr behavior with json arrays
  json_bool.txt
  json_object.txt
  json_string.txt
  fixture/
    json_array/
      passing.txt                   # Inner: actual tests using json array variables
      json_array_wrong_length.txt   # Inner: test that should fail (for error testing)
    json_bool/
      passing.txt
      json_bool_wrong_value.txt
    ...
```

The top-level `json_array.txt` contains tests like:

```
===
json_array passing tests
===
cctr $CCTR_FIXTURE_DIR/json_array/passing.txt --no-color 2>&1 | tail -1
---
All 25 tests passed in {{ t }}s
---
where
* t: number
```

This runs cctr on the fixture and verifies that all 25 inner tests pass.

### Example: Nested Fixtures

Some fixtures have their own fixtures:

```
test/cctr/
  run.txt                           # Top-level tests
  fixture/
    with_fixture/
      read_fixture.txt              # Inner test that reads from its fixture
      fixture/
        data.txt                    # Data file for the inner test
```

The inner test `read_fixture.txt` can use `$CCTR_FIXTURE_DIR` to access `data.txt`.

### Important: All Top-Level Tests Must Call cctr

Every top-level test file (directly under `test/*/`) must call `cctr` as a command. This ensures we're always testing cctr's behavior, not just shell commands.

**Correct pattern:**
```
test/feature/
  feature.txt           # Calls: cctr $CCTR_FIXTURE_DIR/tests ...
  fixture/
    tests/
      actual_tests.txt  # Contains actual test cases
```

**Wrong pattern:**
```
test/feature/
  feature.txt           # Contains: echo hello  ← WRONG! Should call cctr
```

If you need to test shell behavior (echo, pipes, loops), put those in a fixture and have the top-level test verify they pass via cctr.

### Test Categories

| Directory | Purpose |
|-----------|---------|
| `basic/` | Basic cctr functionality, output format |
| `cctr/` | cctr CLI behavior (run, list, failures, multiline commands) |
| `env_vars/` | Environment variable expansion |
| `exit_only/` | Exit-code-only tests (no expected output) |
| `expressions/` | Constraint expression evaluation |
| `fixtures/` | Fixture directory copying and access |
| `json_types/` | JSON variable types and constraints |
| `no_fixture/` | Tests that don't need fixtures |
| `setup_teardown/` | `_setup.txt` and `_teardown.txt` behavior |
| `stdin/` | Reading tests from stdin |
| `template_expansion/` | `{{ VAR }}` template substitution |
| `update_mode/` | `-u` flag for updating expected output |
| `variables/` | Variable capture and constraints |
| `verbose/` | `-v` verbose output mode |
| `with_fixture/` | Basic fixture functionality |

### Writing New Tests

1. **Testing cctr behavior**: Use the two-level pattern
   - Create `test/your_feature/your_feature.txt` that calls cctr
   - Create `test/your_feature/fixture/tests/*.txt` with actual test cases
   - Top-level verifies cctr's output format, pass/fail counts, error messages

2. **Testing shell/command behavior**: Use simple tests
   - Create `test/your_feature/simple.txt` with direct shell commands
   - No fixture needed if you're just testing echo, pipes, etc.

3. **Testing error cases**: Create intentionally failing fixtures
   - Name them descriptively: `wrong_value.txt`, `missing_key.txt`
   - Top-level test verifies the error message format

### Running Tests

```bash
# Run all tests
cctr test/

# Run a specific suite
cctr test/json_types/

# Run with verbose output
cctr test/ -v

# Run tests matching a pattern
cctr test/ -p json
```

## Code Structure

```
crates/
  cctr/           # Main CLI application
    src/
      cli.rs      # Command-line argument parsing
      discover.rs # Test file discovery
      main.rs     # Entry point
      matcher.rs  # Pattern matching with variables
      output.rs   # Terminal output formatting
      parse.rs    # Corpus file parser
      runner.rs   # Test execution
      template.rs # {{ VAR }} expansion
      update.rs   # -u mode file updates
  cctr-corpus/    # Corpus file parsing library
  cctr-expr/      # Constraint expression parser/evaluator
```

## Release Process

1. Update version in `Cargo.toml` (workspace version)
2. Update `CHANGELOG.md` in each crate
3. Run `cargo test` and `cctr test/`
4. Commit and tag: `git tag v0.X.Y`
5. Push with tags: `git push --tags`
6. CI builds and publishes to crates.io and creates GitHub release
