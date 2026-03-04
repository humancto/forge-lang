# Forge Testing Guide

## Running Tests

```bash
# Run ALL tests (Rust unit tests + Forge .fg integration tests)
cargo test && ./target/debug/forge test tests/

# Run just Rust unit tests
cargo test

# Run just Forge integration tests
./target/debug/forge test tests/

# Run a single .fg test file
./target/debug/forge test tests/language_spec_test.fg   # NOTE: forge test needs a FILE here but reports error; easier to use the directory
./target/debug/forge run tests/language_spec_test.fg     # runs and reports test results
```

## Test File Locations

| File | What it covers |
|---|---|
| `tests/language_spec_test.fg` | Complete language spec (variables, operators, control flow, functions, types, strings, arrays, objects, modules) |
| `tests/interpreter_behavior_test.fg` | Exact interpreter behavior contracts — regression tests for copy semantics, APIs, rounding, etc. |
| `tests/control_flow_test.fg` | if/else/when/match/loops — edge cases |
| `tests/edge_cases_test.fg` | Boundary conditions, zero/negative values, overflow, type edge cases |
| `tests/collections_test.fg` | Array and object built-ins |
| `tests/stdlib_test.fg` | Standard library module smoke tests |

## Adding a New `.fg` Test

### Anatomy of a Forge test

```forge
@test
define test_my_feature() {
    // Arrange
    let x = 42

    // Act
    let result = some_function(x)

    // Assert
    assert_eq(result, 84)
    assert(result > 0)
}
```

### Rules (learned the hard way — violating these causes silent failures)

| Rule | Why |
|---|---|
| **No `;` as statement separator** | Forge uses newlines. `{ a = 1; b = 2 }` fails |
| **No `else if` on separate line** | Parser sees `}` as statement end. Keep `else if` on same line |
| **`push(arr, v)` does NOT mutate `arr`** | Copy semantics. Use `map/filter/reduce` to build arrays |
| **`pop(arr)` returns the remaining array**, not the removed element | `let shorter = pop(arr)` |
| **`trim/upper/lower` are string METHODS** | Use `"hello".upper()` not `upper("hello")` — BUT `upper()` as a global also works via the method table |
| **`channel.send()` does NOT work** | Use `send(ch, value)` and `receive(ch)` as free functions |
| **`enumerate` returns `{index, value}` objects** | `pair.index`, `pair.value` — NOT `pair[0]`, `pair[1]` |
| **Triple-quoted `"""..."""` strings are RAW** | No interpolation. `{expr}` preserved literally |
| **`int(true)` is NOT supported** | Use `if b { 1 } else { 0 }` |
| **`<`/`>` on strings not in interpreter** | Use `sort()` to test ordering |
| **`push(arr,v)` inside `for` loop** | Outer `arr` won't grow. Use `map/filter/reduce` |
| **Closures capture by value (copy semantics)** | Counter pattern won't work — use local mutation |
| **`%=` not in parser** | Use `x = x % n` |

### Regex patterns

```forge
// Use double-escaped patterns (Forge doesn't have raw string literals for regex)
assert(regex.test("hello", "[a-z]+"))    // regex.test(), NOT regex.match()
let m = regex.find("foo bar", "[a-z]+") // returns first match
let all = regex.find_all("a b c", "[a-z]+")
let replaced = regex.replace("foo bar", "bar", "baz")
let parts = regex.split("a::b", ":+")
```

### Crypto functions

```forge
crypto.sha256("hello")          // 64-char hex string
crypto.md5("hello")             // 32-char hex string
crypto.sha512("hello")          // 128-char hex string
crypto.hmac_sha256("key", "msg") // 64-char hex string
crypto.base64_encode("hello") / crypto.base64_decode(encoded)
crypto.hex_encode("hello") / crypto.hex_decode(encoded)
crypto.random_bytes(16)         // 16 bytes as 32-char hex
// NOTE: bcrypt_hash/bcrypt_verify/random(min,max) are NOT in the interpreter
```

### Math functions

```forge
math.abs(-5)    math.floor(1.9)   math.ceil(1.1)   math.round(1.5)
math.max(a, b)  math.min(a, b)    math.pow(2, 8)   math.sqrt(16)
math.sin(x)     math.cos(x)       math.tan(x)      math.log(x)
math.pi         math.e
// NOTE: math.round uses "round half away from zero" (not banker's rounding)
//   round(-0.5) == -1, round(2.5) == 3, round(-2.5) == -3
```

### Error handling assertions

```forge
// Check an operation throws
let mut caught = false
try { let _ = 1 / 0 } catch e { caught = true }
assert(caught)

// Inspect error fields
try {
    let _ = 1 / 0
} catch e {
    assert(has_key(e, "message"))
    assert(has_key(e, "type"))
}

// Extract Err message (unwrap_err not available; use match)
match Err("oops") {
    Err(msg) => assert_eq(msg, "oops")
    Ok(_)    => assert(false)
}
```

## What to Test Next (Remaining Gaps)

### 1. Missing from Rust unit tests
- `interpreter/eval.rs` — `exec_stmt` arms for each statement type (could use `#[test]` in eval.rs)
- `interpreter/builtins.rs` — each builtin arm directly (not just via .fg tests)
- Parser error recovery — malformed input should give useful error messages

### 2. Missing from .fg tests
- `fs` module: `fs.read`, `fs.write`, `fs.exists`, `fs.mkdir`, `fs.ls`
- `http` module (if available in tests): GET/POST with mock server
- `jwt` module: sign and verify tokens
- `term` module: color output (smoke tests only)
- `csv` module: parse and stringify round-trip
- PostgreSQL: parameterized queries, TLS connection (requires test DB)

### 3. Known language gaps (future bugs to fix)
- `%=` compound assignment missing from parser
- String `<`/`>` comparison not working in interpreter (works in VM)
- `push` copy semantics inconsistency between interpreter and VM
- `pop` returning remaining array instead of removed element seems like a bug

## CI Pipeline

Tests run automatically on every push/PR via `.github/workflows/ci.yml`:

```
cargo test          → 577 Rust unit tests
forge test tests/   → 571 Forge integration tests
cargo clippy        → Lint check
cargo fmt --check   → Format check
cargo audit         → Security audit
```

All must pass before merging to `main`.
