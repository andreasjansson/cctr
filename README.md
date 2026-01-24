<img src="./assets/logo.png" alt="cctr logo" width="100" align="right" />

# `cctr` - CLI Corpus Test Runner

cctr is a test runner for command-line tools. Tests are defined as plain text corpus files that specify commands and their expected output.

```console
$ cat test/cryptic.txt
===
Test cryptic hello
===
echo "khoor zruog" | tr "a-z" "x-za-w"
---
hello world

$ cctr test/
.

✓ test: 1/1 tests passed in 0.02s

All 1 tests passed in 0.02s
```

cctr is heavily inspired by [Tree-sitter's corpus tests](https://tree-sitter.github.io/tree-sitter/creating-parsers/5-writing-tests.html), which act both as high-level end-to-end tests and documentation.

cctr is especially suited for agentic development of command line tools. cctr test cases can be easily written and read by humans, while the agent satisfies the test cases with code. In agentic development, code is a leaky abstraction. cctr is a sealant.

See the [test/](https://github.com/andreasjansson/cctr/tree/main/test) directory for a comprehensive suite of cctr tests for cctr itself.

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
  - [Membership with contains](#membership-with-contains)
  - [Functions](#functions)
  - [Operator precedence](#operator-precedence)
- [Environment variables](#environment-variables)
- [Parallel execution](#parallel-execution)
- [Updating expected output](#updating-expected-output)
- [Development](#development)
- [License](#license)

## Installation

### Via Homebrew (macOS/Linux)

```bash
brew install andreasjansson/tap/cctr
```

### Via Cargo

```bash
cargo install cctr
```

### Pre-built binaries

Download from the [releases page](https://github.com/andreasjansson/cctr/releases). Binaries are available for:
- Linux (x86_64, ARM64)
- macOS (Intel, Apple Silicon)
- Windows (x86_64, ARM64)

### From source

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

Variables capture dynamic parts of the output using `{{ name }}` or `{{ name: type }}` syntax. Types can be specified inline or omitted for automatic duck-typing.

```
===
process stats
===
./stats-command
---
Processed {{ count: number }} items in {{ time: number }} seconds
---
where
* count > 0
* time < 60
```

### Duck typing

When no type is specified, cctr automatically infers the type from the captured value:

```
===
auto-typed variable
===
echo "count: 42"
---
count: {{ n }}
---
where
* n == 42
* type(n) == number
```

Duck typing uses the following priority:
1. JSON object (starts with `{`)
2. JSON array (starts with `[`)
3. JSON string (starts with `"`)
4. Boolean (`true` or `false`)
5. Null (`null`)
6. Number (valid numeric format)
7. String (fallback)

### Explicit types

Seven variable types can be specified explicitly:

| Type | Matches |
|------|---------|
| `number` | Integers and decimals, including negative: `42`, `3.14`, `-17`, `0.001` |
| `string` | Any text up to the next literal part of the pattern (or end of line) |
| `json string` | JSON string literal: `"hello"`, `"with \"escapes\""` (value is the string content) |
| `json bool` | JSON boolean: `true`, `false` |
| `json array` | JSON array: `[1, 2, 3]`, `["a", "b"]` |
| `json object` | JSON object: `{"name": "alice", "age": 30}` |

Type annotations can have flexible whitespace: `{{ x:number }}`, `{{ x: number }}`, `{{ x : number }}` are all valid.

### JSON types

JSON types are useful when your command outputs JSON data. The captured value is parsed as JSON and can be accessed using array indexing, object property access, and functions.

```
===
test json output
===
echo '{"users": [{"name": "alice"}, {"name": "bob"}]}'
---
{{ data: json object }}
---
where
* len(data.users) == 2
* data.users[0].name == "alice"
* type(data.users) == array
```

Access patterns:
- Array indexing: `arr[0]`, `arr[1]`
- String indexing: `str[0]` (first char), `str[1]` (second char)
- Negative indexing: `arr[-1]` (last element), `str[-1]` (last char)
- Object property: `obj.name`, `obj.nested.value`
- Bracket notation: `obj["key-with-dashes"]`

JSON values may contain `null`, which can be tested with `== null` or `type(x) == null`.

## Constraints

Add a `where` section to validate captured variables with expressions:

```
===
timing must be reasonable
===
./timed-command
---
Took {{ ms }}ms
---
where
* ms > 0
* ms < 5000
```

All constraints must pass for the test to pass.

### Comparison operators

| Operator | Description |
|----------|-------------|
| `==` | Equal |
| `!=` | Not equal |
| `<` | Less than (numbers or strings) |
| `<=` | Less than or equal (numbers or strings) |
| `>` | Greater than (numbers or strings) |
| `>=` | Greater than or equal (numbers or strings) |

```
having
* n == 42
* n != 0
* n >= 10
* n < 100
* "apple" < "banana"
```

### Arithmetic operators

| Operator | Description |
|----------|-------------|
| `+` | Addition (numbers), concatenation (strings, arrays) |
| `-` | Subtraction |
| `*` | Multiplication |
| `/` | Division |
| `%` | Modulo |
| `^` | Exponentiation |

```
having
* n == 10 + 5
* n ^ 3 == 8
* total == count * price
* n % 2 == 0
* "hello" + " " + "world" == "hello world"
* [1, 2] + [3, 4] == [1, 2, 3, 4]
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
| `startswith` | Prefix match |
| `endswith` | Suffix match |
| `not startswith` | Negated prefix match |
| `not endswith` | Negated suffix match |

```
having
* path startswith "/usr"
* filename endswith ".txt"
* path not startswith "/home"
* filename not endswith ".bak"
```

### Regular expressions

Use `matches` with a regex literal (surrounded by `/`):

```
having
* id matches /^[a-z]+[0-9]+$/
* email matches /^[^@]+@[^@]+\.[^@]+$/
* version matches /^\d+\.\d+\.\d+$/
* id not matches /^[0-9]+$/
```

Escape special regex characters with backslash:

```
having
* expr matches /^\(a\+b\)\*c$/
```

### Membership with contains

The `contains` operator works uniformly for strings, arrays, and objects:

```
having
* message contains "error"              # substring in string
* ["ok", "success"] contains status     # element in array
* config contains "debug"               # key in object
* message not contains "fatal"          # negated (shorthand)
* ["error", "fail"] not contains status # negated array membership
```

### Functions

| Function | Description |
|----------|-------------|
| `len(x)` | Length of string, array, or object |
| `type(x)` | Type of value: `number`, `string`, `bool`, `null`, `array`, `object` |
| `keys(obj)` | Array of keys from an object (sorted alphabetically) |
| `values(obj)` | Array of values from an object (sorted by key) |
| `sum(arr)` | Sum of numbers in an array |
| `min(arr)` | Minimum value in a numeric array |
| `max(arr)` | Maximum value in a numeric array |
| `abs(n)` | Absolute value of a number |
| `unique(arr)` | Array with duplicate elements removed (preserves order) |
| `lower(s)` | Convert string to lowercase |
| `upper(s)` | Convert string to uppercase |

```
having
* len(name) > 0
* len(arr) == 3
* type(value) == number
* type(items) == array
* keys(obj) == ["a", "b", "c"]
* values(obj) == [1, 2, 3]
* sum(numbers) == 100
* min(scores) >= 0
* max(scores) <= 100
* abs(delta) < 0.001
* unique([1, 2, 2, 3]) == [1, 2, 3]
* lower("HELLO") == "hello"
* upper("hello") == "HELLO"
```

### Quantifiers

Use `forall` to check that a condition holds for all elements in an array or object:

```
where
* x > 0 forall x in numbers
* len(item.name) > 0 forall item in users
* type(v) == number forall v in obj
```

When iterating over an object, `forall` iterates over the values (not the keys).

### Operator precedence

From highest to lowest:

1. Parentheses `()`
2. Function calls `len()`
3. Unary `-`, `not`
4. Exponentiation `^`
5. Multiplicative `*`, `/`, `%`
6. Additive `+`, `-`
7. Comparison `<`, `<=`, `>`, `>=`, `==`, `!=`
8. String/membership `contains`, `startswith`, `endswith`, `matches`
9. Logical `and`
10. Logical `or`

## Environment variables

cctr injects special environment variables that your commands can use:

| Variable | Description |
|----------|-------------|
| `$CCTR_WORK_DIR` | Temporary directory where tests run |
| `$CCTR_FIXTURE_DIR` | Location of copied fixture files (same as `CCTR_WORK_DIR` when fixture exists) |

Use `$CCTR_FIXTURE_DIR` to reference test data:

```
===
read config
===
cat "$CCTR_FIXTURE_DIR/config.json"
---
{"debug": true}
```

Use `$CCTR_WORK_DIR` to write temporary files:

```
===
create and read file
===
echo "hello" > "$CCTR_WORK_DIR/temp.txt" && cat "$CCTR_WORK_DIR/temp.txt"
---
hello
```

When a fixture exists, `CCTR_FIXTURE_DIR` and `CCTR_WORK_DIR` point to the same location (the fixture is copied into the work directory).

Standard shell environment variables (`$HOME`, `$USER`, `$PATH`, etc.) are also available as usual.

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

See [DEVELOPMENT.md](DEVELOPMENT.md) for development setup, test structure, and contribution guidelines.

## License

MIT
