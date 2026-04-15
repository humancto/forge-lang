# Set Type Implementation Plan (Revised)

## Goal

Add first-class `Set` type to Forge with `set()` constructor, element operations, and set algebra.

## Syntax Design

```forge
let s = set([1, 2, 3])          // from array
let s = set((1, 2, 3))          // from tuple
let empty = set()               // empty set

// Mutating methods (on mutable vars, follows push/pop pattern)
let mut s = set([1, 2])
s.add(4)                        // in-place add (interpreter), returns new set (VM)
s.remove(2)                     // in-place remove / returns new set

// Non-mutating
s.has(3)                        // membership test → bool

// Set algebra via methods (returns new set)
let u = s1.union(s2)
let i = s1.intersect(s2)
let d = s1.diff(s2)

// Builtins
len(s)                          // cardinality
contains(s, x)                  // same as s.has(x)

// Iteration
for x in s { ... }              // iterate elements (insertion order)

// Conversion
s.to_array()                    // convert back to array

// Equality
set([1,2]) == set([2,1])        // true (order-independent)

// Display
println(s)                      // set(1, 2, 3)

// typeof
typeof(s)                       // "Set"
```

## Design Decisions (from expert review)

1. **No `|`/`&` operators for sets.** Forge has no bitwise ops. Adding `|`/`&` as set operators requires new BinOp variants + parser changes — overkill for v1. Use `.union()`, `.intersect()`, `.diff()` methods instead. Can add operator sugar later.

2. **NaN handling:** Use container equality (NaN == NaN inside sets), matching Python semantics. Implement via `Value::identical()` / `Value::equals()` which already handle this in the VM.

3. **GC safety:** For `.add()` and `.remove()` in the VM, clone the existing Vec, drop the GC borrow, then mutate and allocate the new Set. Never hold a GC borrow while allocating. Same pattern as `push()`.

4. **`.remove()` on missing element:** Silent no-op (returns set unchanged). Matches Python's `discard()` semantics.

5. **`set("hello")` from string:** Not supported in v1. `set()` accepts arrays and tuples only. Strings are iterable but auto-splitting into chars for a set is surprising.

6. **Destructuring:** Not supported. Sets have no defined order, so `let (a, b) = set([1,2])` is meaningless. Will error.

7. **JSON serialization:** Serialize as JSON array (no set type in JSON).

8. **Frozen sets:** `.add()` / `.remove()` on a frozen set → error. Uses existing `ObjKind::Frozen` wrapper — no new frozen flag needed.

## Implementation Steps

### 1. AST — No changes needed

`set()` is a builtin function call, not new syntax.

### 2. VM Value Layer (`src/vm/value.rs`)

- Add `Set(Vec<Value>)` to `ObjKind`
- Internal `Vec<Value>` with dedup on construction (O(n²) dedup is fine for typical set sizes)
- Display: `set(1, 2, 3)`
- Equality: check `len(a) == len(b)` first, then verify every element in A exists in B (O(n²))
- GC tracing: combined arm `ObjKind::Array(items) | ObjKind::Tuple(items) | ObjKind::Set(items)`
- SharedValue conversion: `Set(Vec<SharedValue>)` — error on non-shareable elements (closures)
- type_name: `"Set"`
- to_json_string: serialize as JSON array

### 3. VM Bytecode — No new opcode

`set()` is a builtin function call. No syntax-level set literal.

### 4. VM Builtins (`src/vm/builtins.rs`)

- Register `set()` constructor: 0 args → empty set, 1 arg (array/tuple) → set from elements
- `contains(set, x)` support
- `len(set)` support

### 5. VM Machine (`src/vm/machine.rs`)

- GetField dispatch for `.has()`, `.to_array()`, `.len` on Set values
- For `.add()`, `.remove()`, `.union()`, `.intersect()`, `.diff()`: these go through the builtin call path (method → builtin function with self as first arg)
- Iterator support: `GetIter`/`IterNext` for `for x in set { ... }`
- Equality: `==`/`!=` in the BinOp handler for Set-Set comparison

### 6. Interpreter (`src/interpreter/mod.rs`)

- Add `Set(Vec<Value>)` to `Value` enum
- PartialEq: order-independent, length check first
- Display: `set(1, 2, 3)`
- is_truthy: non-empty = true
- to_json_string: serialize as JSON array
- MethodCall dispatch: `.add()`, `.remove()` (in-place on mutable vars, like push/pop), `.has()`, `.union()`, `.intersect()`, `.diff()`, `.to_array()`
- FieldAccess: `.len` returns cardinality
- For-loop iteration
- Binary ops `==`/`!=` for Set-Set
- `typeof(set)` → "Set"

### 7. Interpreter Builtins (`src/interpreter/builtins.rs`)

- `set()` constructor
- `len()` combined arm with Array/Tuple/Set
- `contains()` combined arm

### 8. main.rs

- Add Set handling in `collect_vm_incompatible_expr` if needed (but `set()` is a Call, so may not need changes)

### 9. Tests

**VM tests** (`src/vm/set_tests.rs`): 20 tests

- `set_from_array`, `set_from_tuple`, `set_empty`
- `set_dedup` (duplicates eliminated)
- `set_add`, `set_remove`, `set_remove_missing` (silent no-op)
- `set_has_true`, `set_has_false`
- `set_len`, `set_contains`
- `set_union`, `set_intersect`, `set_diff`
- `set_equality_order_independent`, `set_inequality`
- `set_iteration`
- `set_display`
- `set_to_array`
- `set_typeof`
- `set_frozen_immutable`

**Interpreter tests** (`src/interpreter/tests.rs`): 20 tests

- Same coverage as VM

**Parity tests** (`tests/parity/supported/`): 7 files

- `set_basic.fg` — `len(set([1, 2, 3]))`
- `set_dedup.fg` — `len(set([1, 1, 2, 2, 3]))`
- `set_has.fg` — `set([1, 2, 3]).has(2)`
- `set_add.fg` — `let mut s = set([1]); s.add(2); len(s)`
- `set_equality.fg` — `set([1, 2]) == set([2, 1])`
- `set_iteration.fg` — sum elements via for loop
- `set_algebra.fg` — `len(set([1,2,3]).union(set([3,4,5])))`

### 10. JIT

No changes needed. The JIT type analysis already marks unsupported ops (calls to builtins like `set()` will mark `has_unsupported_ops = true`), causing fallback to VM.

## Edge Cases

- `set([1, 1, 2, 2])` → `set(1, 2)` (dedup on creation)
- `set()` with no args → empty set
- `set(42)` → runtime error ("set() requires an array or tuple")
- `set("hello")` → runtime error (not supported in v1)
- `.add()` on frozen set → runtime error
- `.remove(missing)` → silent no-op, returns set unchanged
- Empty set is falsy, non-empty is truthy
- NaN in sets: container equality (NaN == NaN)
- Set equality checks length first, then element membership (O(n²))
- Nested sets: technically allowed (sets can contain any value), but equality between inner sets uses same O(n²) logic

## Rollback Plan

Revert the feature branch. No existing functionality is modified.
