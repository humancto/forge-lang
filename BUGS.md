# Forge Language Bugs Found During Test Coverage Work

These bugs were discovered during the comprehensive test coverage sprint (2026-03-04).
All are pre-existing — the interpreter/VM split did NOT introduce them.

## BUG-001: `%=` Compound Assignment Missing from Parser

**Symptom:** `x %= 5` causes parse error: `unexpected token: Eq`

**Steps to reproduce:**
```forge
let mut x = 17
x %= 5  // ERROR: unexpected token: Eq
```

**Expected:** `x` becomes `2` (17 mod 5)
**Actual:** Parse error

**Location:** `src/parser/mod.rs` — `%=` not added to compound assignment token list
**Fix:** Add `%=` token (PercentEq) to lexer and parser, similar to `+=`, `-=`, `*=`, `/=`

---

## BUG-002: String `<`/`>` Comparison Not Working in Tree-Walk Interpreter

**Symptom:** `"apple" < "banana"` errors with `invalid operator for String`

**Steps to reproduce:**
```forge
assert("apple" < "banana")  // ERROR: invalid operator for String
```

**Expected:** `true` (lexicographic comparison)
**Actual:** Runtime error

**Location:** `src/interpreter/eval.rs` — `eval_binop` for String values, `Lt`/`Gt`/`Lte`/`Gte` operators
**Note:** Works fine in the VM/bytecode path. Inconsistency between interpreter and VM.
**Fix:** Add string comparison arms in `eval_binop` for String values

---

## BUG-003: `push(arr, v)` Has Copy Semantics in Interpreter (Inconsistent with VM)

**Symptom:** `push(arr, v)` inside a loop or function does NOT modify the outer `arr`

**Steps to reproduce:**
```forge
let mut arr = []
let mut i = 0
while i < 5 {
    push(arr, i)
    i += 1
}
say len(arr)  // prints 0, not 5
```

**Expected:** `arr` grows to length 5
**Actual:** `arr` stays empty (copy semantics — push operates on a copy)

**Location:** `src/interpreter/builtins.rs` — `push` builtin does not mutate the binding
**Fix:** The `push` builtin should mutate the array in the calling scope (like `sort_by` which already works). Need to pass a mutable reference to the environment binding, not just the value.

---

## BUG-004: `pop(arr)` Returns Remaining Array, Not Removed Element

**Symptom:** `let p = pop([1, 2, 3])` gives `[1, 2]` instead of `3`

**Steps to reproduce:**
```forge
let arr = [10, 20, 30]
let p = pop(arr)
say p  // prints [10, 20] — EXPECTED: 30
```

**Expected:** Returns the last element (`30`)
**Actual:** Returns the array with the last element removed (`[10, 20]`)

**Location:** `src/interpreter/builtins.rs` — `pop` builtin implementation
**Fix:** `pop` should return the last element AND mutate the array (or return a tuple/struct with both). Current behavior is surprising and inconsistent with arrays in every other language.

---

## BUG-005: Mutable Closures Don't Share State (Counter Pattern Broken)

**Symptom:** Closures that mutate a captured variable don't share the mutation between calls

**Steps to reproduce:**
```forge
fn make_counter() {
    let mut count = 0
    return fn() {
        count = count + 1
        return count
    }
}
let c = make_counter()
say c()  // prints 1
say c()  // prints 1 again — EXPECTED: 2
say c()  // prints 1 again — EXPECTED: 3
```

**Expected:** Counter increments: `1, 2, 3`
**Actual:** Always returns `1` (count resets each call)

**Location:** `src/interpreter/eval.rs` — closure capture mechanism
**Fix:** Closures need to capture mutable variables by **reference** (shared Arc<Mutex<Value>>) rather than by value copy. This is the classic closure capture bug.

---

## Summary Table

| Bug | Severity | Effort to Fix |
|---|---|---|
| BUG-001: `%=` missing | Medium | Low (lexer + parser, ~20 lines) |
| BUG-002: String `<`/`>` in interpreter | Medium | Low (add 4 arms to eval_binop) |
| BUG-003: `push` copy semantics | High | Medium (need env mutation path) |
| BUG-004: `pop` returns wrong value | High | Low (swap return value) |
| BUG-005: Mutable closure capture | High | High (Arc<Mutex> refactor) |

All 5 bugs have regression tests locked in via:
- `tests/interpreter_behavior_test.fg` — documents current behavior as contract
- Once fixed, update those tests to assert the CORRECT behavior
