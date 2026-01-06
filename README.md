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

## Table of contents

- [Installation](#installation)
- [Usage](#usage)
- [Test format](#test-format)
  - [Basic structure](#basic-structure)
  - [Multiple tests per file](#multiple-tests-per-file)
  - [Exit-only tests](#exit-only-tests)
  - [Multiline output](#multiline-output)
- [Variables](#variables)
  - [Number variables](#number-variables)
  - [String variables](#string-variables)
  - [Multiple variables](#multiple-variables)
- [Constraints](#constraints)
  - [Comparison operators](#comparison-operators)
  - [Arithmetic operators](#arithmetic-operators)
  - [Logical operators](#logical-operators)
  - [String operators](#string-operators)
  - [Regular expressions](#regular-expressions)
  - [Array membership](#array-membership)
  - [Functions](#functions)
  - [Operator precedence](#operator-precedence)
- [Fixtures](#fixtures)
  - [Using FIXTURE_DIR](#using-fixture_dir)
  - [Using WORK_DIR](#using-work_dir)
- [Setup and teardown](#setup-and-teardown)
- [Test discovery](#test-discovery)
- [Parallel execution](#parallel-execution)
- [Updating expected output](#updating-expected-output)
- [Development](#development)
- [Why "cctr"?](#why-cctr)
- [License](#license)

## Installation

```bash
./script/install              # Install to ~/.local/bin
./script/install --system     # Install to /usr/local/bin (requires sudo)
./script/install -d ~/bin     # Install to custom directory
```

## Usage

```
cctr [OPTIONS] [TEST_ROOT]

Arguments:
  [TEST_ROOT]  Root directory for test discovery [default: .]

Options:
  -p, --pattern <PATTERN>  Filter tests by name pattern
  -u, --update             Update expected outputs from actual results
  -l, --list               List all available tests
  -v, --verbose            Show each test as it completes with timing
  -s, --sequential         Run suites sequentially instead of in parallel
      --no-color           Disable colored output
  -h, --help               Print help
  -V, --version            Print version
```

### Examples

Run all tests in a directory:

```bash
cctr tests/
```

Run tests matching a pattern:

```bash
cctr tests/ -p auth
cctr tests/ -p "user.*create"
```

List all tests without running them:

```bash
cctr tests/ -l
```

Output:

```
my_suite [fixture]
  auth: 3 test(s)
    - login with valid credentials
    - login with invalid password
    - logout clears session
  users: 2 test(s)
    - create user
    - delete user
```

Verbose output showing each test with timing:

```bash
cctr tests/ -v
```

Output:

```
✓ my_suite/auth: login with valid credentials 0.03s
✓ my_suite/auth: login with invalid password 0.02s
✓ my_suite/auth: logout clears session 0.04s
✓ my_suite/users: create user 0.05s
✓ my_suite/users: delete user 0.03s

✓ my_suite: 5/5 tests passed in 0.17s

All 5 tests passed in 0.17s
```

Default (non-verbose) output shows dots for passing tests and `F` for failures:

```bash
cctr tests/
```

Output:

```
.....

✓ my_suite: 5/5 tests passed in 0.15s

All 5 tests passed in 0.15s
```

## Test format

### Basic structure

Each test case has three parts separated by `===` and `---`:

```
===
description of the test
===
command to run
---
expected output
```

The description appears in test listings and failure messages. The command is executed in a shell. The expected output is compared against stdout.

### Multiple tests per file

Put multiple tests in a single corpus file:

```
===
test addition
===
echo $((2 + 2))
---
4

===
test subtraction
===
echo $((10 - 3))
---
7

===
test multiplication
===
echo $((6 * 7))
---
42
```

### Exit-only tests

Omit the expected output to only verify that the command exits successfully (exit code 0):

```
===
check file exists
===
test -f /etc/passwd
---

===
directory is writable
===
test -w /tmp
---
```

### Multiline output

Expected output can span multiple lines:

```
===
list files
===
printf "one\ntwo\nthree\n"
---
one
two
three
```

## Variables

Variables capture dynamic parts of the output. Declare them in a `with` section and reference them in the expected output using `{{ name }}` syntax.

### Number variables

Capture numeric values:

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
```

Number variables match integers and decimals, including negative numbers: `42`, `3.14`, `-17`, `0.001`.

### String variables

Capture text values:

```
===
error message
===
./failing-command
---
Error: {{ message }}
---
with
* message: string
```

String variables match any text up to the next literal part of the pattern (or end of line).

### Multiple variables

Capture several values in one test:

```
===
process stats
===
./stats-command
---
Processed {{ count }} items in {{ time }} seconds
---
with
* count: number
* time: number
```

## Constraints

Add a `having` section to validate captured variables with expressions:

```
===
timing must be reasonable
===
./timed-command
---
Took {{ ms }}ms
---
with
* ms: number
having
* ms > 0
* ms < 5000
```

All constraints must pass for the test to pass.

### Comparison operators

| Operator | Description |
|----------|-------------|
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than |
| `<=` | Less than or equal |
| `>` | Greater than |
| `>=` | Greater than or equal |

```
having
* n == 42
* n != 0
* n >= 10
* n < 100
```

### Arithmetic operators

| Operator | Description |
|----------|-------------|
| `+` | Addition |
| `-` | Subtraction |
| `*` | Multiplication |
| `/` | Division |
| `%` | Modulo |
| `^` or `**` | Exponentiation |

```
having
* n == 10 + 5
* n == 2 ^ 3
* total == count * price
* n % 2 == 0
```

### Logical operators

| Operator | Description |
|----------|-------------|
| `and` | Logical AND |
| `or` | Logical OR |
| `not` | Logical NOT |

```
having
* n > 0 and n < 100
* status == "ok" or status == "success"
* not (n < 0)
```

Use parentheses to control evaluation order:

```
having
* (a > 0 and b > 0) or c == 0
```

### String operators

| Operator | Description |
|----------|-------------|
| `contains` | Substring match |
| `startswith` | Prefix match |
| `endswith` | Suffix match |

```
having
* message contains "error"
* path startswith "/usr"
* filename endswith ".txt"
```

Combine with `not`:

```
having
* not (message contains "fatal")
```

### Regular expressions

Use `matches` with a regex literal (surrounded by `/`):

```
having
* id matches /^[a-z]+[0-9]+$/
* email matches /^[^@]+@[^@]+\.[^@]+$/
* version matches /^\d+\.\d+\.\d+$/
```

Escape special regex characters with backslash:

```
having
* expr matches /^\(a\+b\)\*c$/
```

### Array membership

Check if a value is in a set using `in`:

```
having
* status in ["ok", "success", "completed"]
* code in [200, 201, 204, 301, 302]
* not (code in [400, 401, 403, 404, 500])
```

### Functions

| Function | Description |
|----------|-------------|
| `len(s)` | Length of string |

```
having
* len(name) > 0
* len(name) <= 50
* len(short) < len(long)
```

### Operator precedence

From highest to lowest:

1. Parentheses `()`
2. Function calls `len()`
3. Unary `-`, `not`
4. Exponentiation `^`, `**`
5. Multiplicative `*`, `/`, `%`
6. Additive `+`, `-`
7. Comparison `<`, `<=`, `>`, `>=`, `==`, `!=`
8. String/membership `contains`, `startswith`, `endswith`, `matches`, `in`
9. Logical `and`
10. Logical `or`

## Fixtures

Put test data files in a `fixture/` subdirectory. cctr copies this directory to a temporary location before running the suite.

```
tests/
  my_suite/
    test_file.txt
    fixture/
      config.json
      data/
        sample.csv
```

### Using FIXTURE_DIR

Reference fixture files in commands using the `{{ FIXTURE_DIR }}` variable:

```
===
read config
===
cat {{ FIXTURE_DIR }}/config.json
---
{"debug": true}

===
process data
===
wc -l {{ FIXTURE_DIR }}/data/sample.csv
---
100 {{ path }}
---
with
* path: string
```

### Using WORK_DIR

The `{{ WORK_DIR }}` variable points to the temporary directory where tests run. Use it to write output files:

```
===
create output
===
echo "result" > {{ WORK_DIR }}/output.txt && cat {{ WORK_DIR }}/output.txt
---
result
```

When a fixture exists, `FIXTURE_DIR` and `WORK_DIR` point to the same location (the fixture is copied into the work directory).

## Setup and teardown

Create `_setup.txt` and `_teardown.txt` files in a suite directory for code that runs before and after the suite's tests.

```
tests/
  my_suite/
    _setup.txt
    _teardown.txt
    test_feature.txt
```

`_setup.txt`:

```
===
initialize database
===
./init-db.sh
---
```

`_teardown.txt`:

```
===
cleanup
===
./cleanup.sh
---
```

Setup runs once before all tests in the suite. If setup fails, the suite's tests are skipped. Teardown runs after all tests complete, regardless of pass/fail status.

## Test discovery

cctr recursively finds all `.txt` files in the test root, excluding:

- Files starting with `_` (setup/teardown files)
- Files inside `fixture/` directories

Each directory containing corpus files becomes a test suite. The suite name is derived from the directory path relative to the test root.

```
tests/
  auth/
    login.txt          → suite "auth", file "login"
    logout.txt         → suite "auth", file "logout"
    fixture/
      users.json       → (not a test file)
  api/
    v1/
      users.txt        → suite "api/v1", file "users"
```

## Parallel execution

By default, cctr runs test suites in parallel using all available CPU cores. Tests within a suite run sequentially.

Use `-s` or `--sequential` to run suites one at a time:

```bash
cctr tests/ -s
```

This is useful when suites share external resources or for debugging.

## Updating expected output

When command output changes intentionally, use `-u` to update the corpus files:

```bash
cctr tests/ -u
```

This replaces the expected output in failing tests with the actual output. Review the changes with `git diff` before committing.

Only tests without variables are updated. Tests with variables must be updated manually.

## Development

```bash
./script/test      # Run unit tests and corpus tests
./script/install   # Build and install
```

## Why "cctr"?

Named after the [Corpus Christi Terminal Railroad](https://en.wikipedia.org/wiki/Corpus_Christi_Terminal_Railroad).

## License

MIT
