<img src="./assets/logo.png" alt="cctr logo" width="100" align="right" />

# `cctr` - CLI Corpus Test Runner

cctr is a test runner for command-line tools. Tests are defined as plain text corpus files that specify commands and their expected output.

```
===
Test cryptic hello
===
echo "khoor zruog" | tr "a-z" "x-za-w"
---
hello world
```

cctr is heavily inspired by [Tree-sitter's corpus tests](https://tree-sitter.github.io/tree-sitter/creating-parsers/5-writing-tests.html), which act both as high-level end-to-end tests and documentation.

cctr is especially suited for agentic development of command line tools. cctr test cases can be easily written and read by humans, while the agent satisfies the test cases with code. In agentic development, code is a leaky abstraction. cctr is a sealant.

## Table of contents

- [Installation](#installation)
- [Usage](#usage)
- [Directory structure](#directory-structure)
  - [Suites](#suites)
  - [Fixtures](#fixtures)
  - [Setup and teardown](#setup-and-teardown)
- [Test file format](#test-file-format)
  - [Basic structure](#basic-structure)
  - [Multiple tests per file](#multiple-tests-per-file)
  - [Exit-only tests](#exit-only-tests)
  - [Multiline output](#multiline-output)
- [Variables](#variables)
- [Constraints](#constraints)
  - [Comparison operators](#comparison-operators)
  - [Arithmetic operators](#arithmetic-operators)
  - [Logical operators](#logical-operators)
  - [String operators](#string-operators)
  - [Regular expressions](#regular-expressions)
  - [Array membership](#array-membership)
  - [Functions](#functions)
  - [Operator precedence](#operator-precedence)
- [Built-in variables](#built-in-variables)
- [Environment variables](#environment-variables)
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

## Corpus test directory structure

cctr discovers tests by recursively scanning for `.txt` files. The directory structure determines how tests are organized into suites.

### Suites

Each directory containing `.txt` files becomes a test suite. The suite name is the directory path relative to the test root:

```
tests/
  auth/
    login.txt           → suite "auth", file "login"
    logout.txt          → suite "auth", file "logout"
  api/
    v1/
      users.txt         → suite "api/v1", file "users"
      products.txt      → suite "api/v1", file "products"
  utils.txt             → suite "tests", file "utils"
```

Files starting with `_` are reserved for setup/teardown and are not treated as test files.

### Fixtures

A `fixture/` subdirectory contains test data that gets copied to a temporary directory before the suite runs. This ensures tests start with a clean, known state.

```
tests/
  my_suite/
    feature.txt
    integration.txt
    fixture/
      config.json
      data/
        users.csv
        products.csv
      scripts/
        helper.sh
```

When a suite has a fixture:

- The entire `fixture/` directory is copied to a temp directory
- Tests run with that temp directory as the working directory
- The `{{ FIXTURE_DIR }}` variable points to this location
- Changes made during tests don't affect the original fixture
- The temp directory is cleaned up after the suite completes

Files inside `fixture/` are never treated as test files.

### Setup and teardown

Create `_setup.txt` and/or `_teardown.txt` in a suite directory:

```
tests/
  my_suite/
    _setup.txt          → runs before all tests
    _teardown.txt       → runs after all tests
    feature.txt
    integration.txt
    fixture/
      ...
```

`_setup.txt` runs once before any tests in the suite. If setup fails, all tests in the suite are skipped:

```
===
initialize database
===
./scripts/init-db.sh
---
Database initialized

===
seed test data
===
./scripts/seed-data.sh
---
```

`_teardown.txt` runs after all tests complete, regardless of whether they passed or failed:

```
===
cleanup temp files
===
rm -rf /tmp/test-*
---

===
stop services
===
./scripts/stop-services.sh
---
```

Setup and teardown files use the same format as regular test files. Each test case in them must pass for the file to succeed.

### Complete example

A full-featured test directory:

```
tests/
  auth/
    _setup.txt
    _teardown.txt
    login.txt
    logout.txt
    permissions.txt
    fixture/
      users.json
      roles.json
  api/
    v1/
      users.txt
      products.txt
      fixture/
        sample_request.json
        expected_response.json
    v2/
      users.txt
  utils/
    strings.txt
    numbers.txt
```

This creates three suites:
- `auth` (with fixture, setup, and teardown)
- `api/v1` (with fixture)
- `api/v2` (no fixture)
- `utils` (no fixture)

## Test file format

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

The description appears in test listings and failure messages. The command is executed in a shell (`sh -c`). The expected output is compared against stdout.

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

Two variable types are supported:

| Type | Matches |
|------|---------|
| `number` | Integers and decimals, including negative: `42`, `3.14`, `-17`, `0.001` |
| `string` | Any text up to the next literal part of the pattern (or end of line) |

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

## Built-in variables

These variables are automatically available in both commands and expected output:

| Variable | Description |
|----------|-------------|
| `{{ WORK_DIR }}` | Temporary directory where tests run |
| `{{ FIXTURE_DIR }}` | Location of copied fixture files (same as `WORK_DIR` when fixture exists) |

Use `FIXTURE_DIR` to reference test data:

```
===
read config
===
cat {{ FIXTURE_DIR }}/config.json
---
{"debug": true}
```

Use `WORK_DIR` to write temporary files:

```
===
create and read file
===
echo "hello" > {{ WORK_DIR }}/temp.txt && cat {{ WORK_DIR }}/temp.txt
---
hello
```

When a fixture exists, `FIXTURE_DIR` and `WORK_DIR` point to the same location (the fixture is copied into the work directory).

## Environment variables

Environment variables can be used in both commands and expected output using the same `{{ VAR_NAME }}` syntax:

```
===
use home directory
===
echo "home={{ HOME }}"
---
home={{ HOME }}

===
current user
===
whoami
---
{{ USER }}
```

This is useful for:
- Testing commands that output paths or user-specific information
- Configuring tests based on the environment
- Avoiding hardcoded values that differ between machines

Environment variables are expanded after built-in variables (`WORK_DIR`, `FIXTURE_DIR`), so built-in variables take precedence if there's a name conflict. Unknown variables (not set in the environment) are left unchanged.

## Parallel execution

By default, cctr runs test suites in parallel using all available CPU cores. Tests within a suite run sequentially (to allow setup/teardown and shared fixture state).

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
