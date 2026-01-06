# cctr - CLI Corpus Test Runner

cctr is a test runner for command-line tools. Tests are defined as plain text corpus files that specify commands and their expected output.

```
===
simple echo
===
echo hello world
---
hello world
```

## Installation

```bash
./script/install              # Install to ~/.local/bin
./script/install --system     # Install to /usr/local/bin (requires sudo)
./script/install -d ~/bin     # Install to custom directory
```

Or build manually:

```bash
cargo build --release
cp target/release/cctr ~/.local/bin/
```

## Usage

```bash
cctr tests/              # Run all tests in directory
cctr tests/ -p auth      # Run tests matching "auth"
cctr tests/ -l           # List all tests
cctr tests/ -v           # Verbose output with timing
cctr tests/ -u           # Update expected output from actual results
```

## Test format

Each test case has three parts: description, command, and expected output:

```
===
description of the test
===
command to run
---
expected output
```

Multiple tests can be in one file:

```
===
test one
===
echo one
---
one

===
test two
===
echo two
---
two
```

### Variables and constraints

Capture dynamic values and validate them with expressions:

```
===
timing output
===
./slow-command
---
Completed in {{ duration }}s
---
with
* duration: number
having
* duration > 0
* duration < 60
```

Variable types: `number`, `string`

Constraint expressions support:
- Arithmetic: `+`, `-`, `*`, `/`, `**`, `%`
- Comparison: `<`, `<=`, `>`, `>=`, `==`, `!=`
- Logical: `and`, `or`, `not`
- String ops: `contains`, `startswith`, `endswith`, `matches`
- Membership: `in`
- Functions: `len()`

### Fixtures

Put test data in a `fixture/` subdirectory. It's copied to a temp directory before each suite runs. Access it via `{{ FIXTURE_DIR }}`:

```
tests/
  my_suite/
    test_file.txt
    fixture/
      data.json
```

```
===
read fixture
===
cat {{ FIXTURE_DIR }}/data.json
---
{"key": "value"}
```

### Setup and teardown

Create `_setup.txt` and `_teardown.txt` in a suite directory. Setup runs before all tests, teardown runs after.

### Exit-only tests

Omit the expected output to only check that the command succeeds:

```
===
just check exit code
===
true
---
```

## Project structure

```
tests/
  suite_one/
    feature_a.txt      # corpus file with tests
    feature_b.txt
    fixture/           # optional test data
      sample.json
    _setup.txt         # optional setup
    _teardown.txt      # optional teardown
  suite_two/
    ...
```

## Why "cctr"?

Named after the Corpus Christi Terminal Railroad.

## License

MIT
