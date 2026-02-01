---
name: cctr
description: >
  CLI Corpus Test Runner for testing command-line tools. Tests are plain text
  corpus files that specify commands and expected output. Use cctr for end-to-end
  testing of CLI tools where tests also serve as documentation. Especially suited
  for agentic development where humans write test cases and agents implement code.
---

# cctr - CLI Corpus Test Runner

cctr runs end-to-end tests for command-line tools. Tests are defined as plain text corpus files that specify commands and their expected output.

## ⚠️ CRITICAL: Tests Are Documentation

cctr tests serve **dual purposes**: they test behavior AND document how the tool works. When writing tests:

- **Don't filter or transform output** unless absolutely necessary
- **Show real output** - future readers use tests to understand the tool
- **Only use `| jq .`** for JSON readability, nothing else
- **Avoid `| head`, `| tail`, `| grep`** - they hide behavior
- **If output is too verbose**, that's a signal the tool's output should be improved

```
# ❌ BAD: Hides what the tool actually outputs
===
test search
===
mytool search "query" | head -1
---
first result

# ✅ GOOD: Shows actual behavior, serves as documentation  
===
test search returns results with metadata
===
mytool search "query" -n 1
---
* Result Title [Author] 2024-01-15
  Result excerpt showing matched content...
  https://example.com/page/123
```

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

Download from [releases](https://github.com/andreasjansson/cctr/releases) for Linux, macOS, Windows (x86_64 and ARM64).

## Claude Code Skill

Copy the skill to your Claude Code skills directory:

```bash
# Personal (available across all your projects):
cp -r skills/cctr ~/.claude/skills/

# Project-specific (commit to version control):
cp -r skills/cctr .claude/skills/
```

## Quick Start

```bash
# Create a test file
cat > test/cli.txt << 'EOF'
===
help shows usage
===
mytool --help
---
Usage: mytool <COMMAND>

Commands:
  run     Run the tool
  help    Print help

Options:
  -h, --help     Print help
  -V, --version  Print version
EOF

# Run tests
cctr test/
```

## CLI Reference

```
cctr [OPTIONS] [TEST_ROOT]

Arguments:
  [TEST_ROOT]  Root directory or file for test discovery [default: .]

Options:
  -p, --pattern <PATTERN>  Filter tests by name pattern
  -u, --update             Update expected outputs from actual results
  -l, --list               List all available tests
  -v, --verbose            Show each test as it completes with timing
  -vv                      Stream test output in real-time (for debugging)
  -s, --sequential         Run suites sequentially instead of in parallel
      --no-color           Disable colored output
  -h, --help               Print help
  -V, --version            Print version
```

### Common Commands

```bash
cctr test/                    # Run all tests in directory
cctr test/cli.txt             # Run single test file
cctr test/ -p auth            # Filter by pattern
cctr test/ -v                 # Verbose: show each test result
cctr test/ -vv                # Extra verbose: stream output in real-time
cctr test/ -u                 # Update expected output from actual
cctr test/ -l                 # List tests without running
cctr -                        # Read test from stdin
```

## Test File Format

### Basic Structure

```
===
test description (shown in output)
===
command to run
---
expected output
```

### Multiple Tests Per File

```
===
test one
===
echo hello
---
hello

===
test two
===
echo world
---
world
```

### Exit-Only Tests (No Expected Output)

Omit expected output to only verify exit code 0:

```
===
file exists check
===
test -f config.json
---
```

### Multiline Commands

```
===
complex pipeline
===
echo "line1"
echo "line2"
echo "line3"
---
line1
line2
line3
```

## Variables and Constraints

### Capturing Dynamic Values

Use `{{ name }}` to capture variable parts of output:

```
===
server starts on dynamic port
===
./start-server
---
Server listening on port {{ port }}
---
where
* port > 1024
* port < 65536
```

### Variable Types

| Type | Matches | Example |
|------|---------|---------|
| (auto) | Duck-typed from value | `{{ x }}` |
| `number` | Integers, decimals | `{{ n: number }}` |
| `string` | Any text | `{{ s: string }}` |
| `json object` | `{"key": "value"}` | `{{ obj: json object }}` |
| `json array` | `[1, 2, 3]` | `{{ arr: json array }}` |
| `json string` | `"quoted"` | `{{ s: json string }}` |
| `json bool` | `true`, `false` | `{{ b: json bool }}` |

### Constraint Expressions

```
===
test with constraints
===
./stats
---
Processed {{ count }} items in {{ time }}s
---
where
* count > 0
* time < 60
* count % 10 == 0
```

#### Operators

| Category | Operators |
|----------|-----------|
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `^` |
| Logical | `and`, `or`, `not` |
| String | `startswith`, `endswith`, `contains`, `matches /regex/` |
| Membership | `["a","b"] contains x`, `obj contains "key"` |

#### Functions

| Function | Description |
|----------|-------------|
| `len(x)` | Length of string, array, or object |
| `type(x)` | Returns: `number`, `string`, `bool`, `null`, `array`, `object` |
| `keys(obj)` | Array of object keys (sorted) |
| `values(obj)` | Array of object values (sorted by key) |
| `sum(arr)` | Sum of numbers in array |
| `min(arr)` | Minimum value |
| `max(arr)` | Maximum value |
| `abs(n)` | Absolute value |
| `unique(arr)` | Remove duplicates |
| `lower(s)` | Lowercase string |
| `upper(s)` | Uppercase string |
| `env("VAR")` | Get environment variable |

#### JSON Access

```
===
test json response
===
curl -s http://api/users/1
---
{{ user: json object }}
---
where
* user.name == "alice"
* len(user.roles) > 0
* user.roles[0] == "admin"
```

#### Quantifiers

```
===
all items valid
===
./list-items
---
{{ items: json array }}
---
where
* item.price > 0 forall item in items
* len(name) > 0 forall name in names
```

## Directives

### %skip - Skip Tests

```
===
not implemented yet
%skip
===
future-feature
---
expected

===
requires special setup
%skip(needs database) if: ! command -v psql
===
db-test
---
result
```

File-level skip (top of file):
```
%skip(all tests disabled)

===
test one
===
...
```

### %platform - Platform Restriction

```
%platform unix

===
unix only test
===
ls -la
---
...
```

| Platform | Matches |
|----------|---------|
| `unix` | Linux, macOS, BSD |
| `linux` | Linux only |
| `macos` | macOS only |
| `windows` | Windows only |

### %shell - Shell Selection

```
%shell bash

===
bash-specific syntax
===
echo $'hello\nworld'
---
hello
world
```

| Shell | Platforms |
|-------|-----------|
| `bash` | Unix (default), Windows (if Git Bash installed) |
| `sh` | Unix |
| `zsh` | Unix |
| `powershell` | Windows (default), Unix (if installed) |
| `cmd` | Windows only (single-line commands only) |

## Directory Structure

```
test/
  auth/
    login.txt           # Tests in "auth" suite
    logout.txt
    fixture/            # Test data, copied to temp dir
      users.json
    _setup.txt          # Runs before all tests in suite
    _teardown.txt       # Runs after all tests in suite
  api/
    v1/
      users.txt         # Tests in "api/v1" suite
```

### Fixtures

The `fixture/` directory is copied to a temp directory before tests run:

```
===
read config file
===
cat config.json
---
{"debug": true}
```

### Setup and Teardown

`_setup.txt` runs before all tests (if it fails, suite is skipped).
`_teardown.txt` runs after all tests (always, even if tests fail).

## Environment Variables

| Variable | Description |
|----------|-------------|
| `$CCTR_WORK_DIR` | Temp directory where tests run |
| `$CCTR_FIXTURE_DIR` | Location of copied fixture files |
| `$CCTR_TEST_PATH` | Original test directory path |

```
===
use fixture data
===
cat "$CCTR_FIXTURE_DIR/data.txt"
---
test data content
```

## Output Handling

### ANSI Escape Codes

cctr automatically strips ANSI escape codes from command output. Test against plain text:

```
===
colored output is stripped
===
mytool --color=always status
---
Status: OK
```

### Line Endings

Windows `\r\n` line endings are normalized to `\n`.

## Longer Delimiters

If output contains `---`, use longer delimiters:

```
====
markdown with horizontal rules
====
cat doc.md
----
# Title
---
Content
----
More content
```

## Best Practices

### Write Tests as Documentation

```
# ❌ Filters hide behavior
===
test
===
mytool list | grep -c item
---
5

# ✅ Shows what users will see
===
list shows all items with details
===
mytool list
---
Items:
  1. First item (active)
  2. Second item (pending)
  3. Third item (active)
  4. Fourth item (done)
  5. Fifth item (active)
```

### Use Variables for Dynamic Parts Only

```
# ❌ Over-capturing hides expected format
===
test
===
mytool status
---
{{ output }}

# ✅ Capture only truly dynamic values
===
status shows uptime
===
mytool status
---
Status: running
Uptime: {{ hours }}h {{ minutes }}m
Memory: {{ mb }}MB
---
where
* hours >= 0
* minutes >= 0
* minutes < 60
```

### Keep Tests Focused

```
# ❌ Too many things in one test
===
everything works
===
mytool init
mytool add item1
mytool add item2
mytool list
mytool delete item1
mytool list
---
...

# ✅ One behavior per test
===
init creates empty database
===
mytool init
---
Initialized empty database

===
add creates item
===
mytool add "test item"
---
Added item 1: test item

===
list shows items
===
mytool list
---
1: test item
```

## Updating Tests

When command output changes intentionally:

```bash
cctr test/ -u  # Updates expected output in failing tests
git diff       # Review changes before committing
```

Note: Tests with variables are not auto-updated.

## Debugging

```bash
cctr test/ -vv              # Stream output in real-time
cctr test/specific.txt -vv  # Debug single file
cctr test/ -p failing       # Run only matching tests
```
