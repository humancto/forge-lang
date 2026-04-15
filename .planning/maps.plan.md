# M9.3 — Map type (any-key dictionary)

## Goal

Add a first-class `Map` value type distinct from `Object` (string-keyed).
A `Map` accepts **any value** as a key — ints, floats, tuples, strings,
even sets. Backed by an ordered `Vec<(Value, Value)>` to preserve
insertion order and avoid the Hash-trait cost on `Value`.

## Why it's different from Object

| Feature   | `Object`               | `Map` (new)                   |
| --------- | ---------------------- | ----------------------------- |
| Key type  | `String` only          | any `Value`                   |
| Literal   | `{ a: 1, b: 2 }`       | no literal; `map()` ctor      |
| Access    | `o.a`, `o["a"]`        | `m.get(key)`                  |
| Iteration | `for k, v in o`        | `for k, v in m`               |
| Equality  | recursive string-keyed | recursive value-keyed         |
| JSON      | native object          | **errors on non-string keys** |
| typeof    | `"Object"`             | `"Map"`                       |

## Surface

```forge
let m = map()                        // empty
let m = map([("a", 1), ("b", 2)])    // from tuples
let m = map([[1, "x"], [2, "y"]])    // from arrays of pairs
m.set(1, "one")                      // new map (returns)
m.get(1)                             // "one"
m.has(1)                             // true
m.remove(1)                          // new map without key 1
m.keys()                             // [1, "a", "b"]
m.values()                           // ["one", 1, 2]
m.len()                              // 3
m.to_array()                         // [(1, "one"), ("a", 1), ("b", 2)]
for k, v in m { say k + "=" + v }    // iteration
```

`.set`/`.remove` return a **new** map (non-mutating, like Set). The
interpreter also has an in-place rewrite path for `let mut m = ...;
m.set(k, v)` — mirrors Set exactly, must follow the clone-then-drop GC
pattern documented in CLAUDE.md.

### Display form

`Map(1 => "one", "a" => 1, "b" => 2)` — `Map(...)` prefix distinguishes
from Object, `=>` separator distinguishes from Object's `:` and from
Set's `{...}`. Lock this in before writing snapshot tests.

### Insertion-order on overwrite

On `.set(k, v)` where `k` already exists, **preserve the original
position** (scan with index, update in place). Matches JS `Map`, Python
`dict`, Java `LinkedHashMap`. Only truly new keys append.

### typeof

`typeof map()` returns `"Map"` (capitalized — matches `"Array"`,
`"Object"`, `"Set"` convention in `src/interpreter/mod.rs:169` and
`src/vm/value.rs:430`).

## Files to touch

### Interpreter (`src/interpreter/`)

- `mod.rs`:
  - Add `Value::Map(Vec<(Value, Value)>)` variant.
  - Update manual `PartialEq` (order-independent on entries).
  - Update `Display` — `Map(k => v, ...)`.
  - Update `container_eq` Map arm.
  - Update `typeof` → `"Map"`.
  - Update `is_truthy` → non-empty.
  - Add `Stmt::For` Map arm yielding `(key, value)` pairs. (Interpreter
    already handles `var2`; just add the Map case.)
  - Add method dispatch in `Expr::Call → FieldAccess` for
    `get`/`set`/`has`/`remove`/`keys`/`values`/`len`/`to_array`.
  - Frozen-peel for read methods; reject `.set`/`.remove` on frozen
    with `"cannot mutate a frozen map"` (matches Set semantics — even
    though they return a new map, frozen rejects for mental-model
    consistency with the in-place rewrite path).
  - In-place mutation path for `let mut m = ...; m.set(k, v)` and
    `m.remove(k)` mirrors Set exactly.
- `builtins.rs`:
  - `map()` constructor: 0 args (empty), array-of-pairs, or another Map.
  - `contains()` Map arm using `container_eq`.
  - `len()` Map arm.
  - Free-function `keys(m)` / `values(m)` — Map arms alongside Object.

### VM (`src/vm/`)

- `value.rs`:
  - Add `ObjKind::Map(Vec<(Value, Value)>)`.
  - `display` — `Map(k => v, ...)`.
  - `trace` — visit every key and value (GC).
  - `type_name` → `"Map"`.
  - `GcObject::obj_equals` Map arm (order-independent — every A entry
    must have a B entry with `set_eq` key and `equals` value).
  - `SharedValue::Map` variant + `value_to_shared` / `shared_to_value`
    Map arms. **Don't skip this** — channel-ferried Maps panic
    otherwise.
- `builtins.rs`:
  - `map()` constructor.
  - `len()` Map arm (line ~414).
  - `contains()` Map arm.
  - Free functions `keys(m)` / `values(m)` — Map arms.
  - Map method dispatch block in `call_forge_method` after Set
    dispatch: `has`/`get`/`set`/`remove`/`keys`/`values`/`len`/
    `to_array`. Frozen-peel for reads, reject for `.set`/`.remove`.
  - **No collision with Object.get**: `get_object_fields`
    (`src/vm/builtins.rs:3286`) filters strictly to `ObjKind::Object`,
    so Map receivers fall straight through to the new dispatch block.
    Verified in plan review. Put Map block immediately after Set.
- `machine.rs`:
  - In-place mutation path for mutable map variables via the
    clone-then-drop pattern (same shape as Set's path). Watch GC
    borrow — collect before alloc.
- `compiler.rs` — **for-loop `var2` destructure (unlocks Map + Object
  VM parity)**:
  - Today `Stmt::For` compile (`src/vm/compiler.rs:781-836`) reads
    `var` and silently ignores `var2`. This means `for k, v in obj` is
    interpreter-only in VM mode.
  - After this change: when `var2.is_some()`, the for-loop body
    emits an `IterGet` into a scratch register (expected to be a
    2-tuple) followed by two `GetIndex`-equivalent ops that extract
    `[0]` → `var_reg` and `[1]` → `var2_reg`.
  - `IterGet` handler in `machine.rs` gains a Map arm: yields a
    freshly-allocated 2-tuple `(key, value)` for iteration index `i`.
  - Object's existing `for k, v in obj` now also works in the VM — this
    is a **free parity win** worth calling out in CHANGELOG.
- `set_tests.rs` is the template layout. Add `map_tests.rs` with ≥22
  tests.

### Stdlib (`src/stdlib/`)

- `json_module.rs`: `forge_to_json_compact` / `forge_to_json_pretty`
  Map arm. **Error** on non-string keys (matches Python's behavior).
  Don't coerce via display — coercion produces `{"[1, 2]": "a"}` which
  is a lossy, non-round-trippable footgun. Error message:
  `"JSON serialization requires string keys; map contains a <type> key"`.

### Parity fixtures (`tests/parity/supported/`)

- `map_basic.fg`, `map_get.fg`, `map_set.fg`, `map_remove.fg`,
  `map_keys_values.fg`, `map_int_keys.fg`, `map_equality.fg`,
  `map_iteration.fg`, `map_len.fg`, `map_nested.fg`.

### Docs

- `CHANGELOG.md` under `[Unreleased]`:
  - **Added:** First-class `Map` type with any-value keys
    (`map([(k, v), ...])`). Distinct from `Object` (which remains
    string-keyed). Supports `.get`/`.set`/`.remove`/`.has`/`.keys`/
    `.values`/`.len`/`.to_array`, value-equality lookup
    (`set_eq` — so `1` and `1.0` collide), insertion-order
    preservation on overwrite, frozen-map rejection on mutating
    methods, error on JSON serialization with non-string keys.
  - **Added:** VM support for `for k, v in obj` iteration over
    Objects (previously interpreter-only). Unlocked as a side effect
    of Map iteration work.
- No `CLAUDE.md` learnings expected up-front; add any new pitfalls
  discovered during implementation at the end.

## Iteration strategy (final)

**Problem:** The VM compiler at `src/vm/compiler.rs:781-836` reads `var`
and ignores `var2`. There's no destructuring path today — `for k, v in
m` would silently give `k = (key, val)` and leave `v` undefined.

**Solution:** Implement `var2` destructuring in the VM compiler
properly. This unblocks Map iteration _and_ fixes the latent
Object-iteration parity gap in one stroke.

1. `IterGet` handler in `machine.rs` grows a Map arm: for iteration
   index `i`, allocate a fresh 2-tuple `(k, v)` and write to the
   destination register.
2. `Stmt::For` compile in `compiler.rs`: when `var2.is_some()`, after
   emitting `IterGet` into a scratch register, emit two index extracts
   (`GetIndex` with constant `0` and `1`) into `var_reg` and
   `var2_reg`.
3. Object iteration gets the same treatment: `IterGet` on Object
   yields a 2-tuple of `(key_string, value)` at the i-th field slot.
4. Parity: `for k, v in obj` in VM mode starts working as a side
   effect. Add a VM regression test.

**Option rejected:** Adding a dedicated `MapIterGet` opcode. Too
invasive for v1 — new opcode byte, JIT type analysis touch, bytecode
serialization tag. The `var2`-destructure approach piggybacks on
existing machinery.

**Option rejected:** Runtime-normalize-to-Array-of-tuples. Doesn't
work — still bottoms out on the missing `var2` path.

## Equality semantics

Map equality is order-independent on entries but order-preserving on
iteration. Two maps are equal iff same `len()` and every `(k, v)` in A
has a matching `(k', v')` in B where `k.set_eq(k')` and `v.equals(v')`.

**Keys use `set_eq`** (NaN self-match, Int/Float promotion) so
`map([(1.0, "x")]).get(1)` works. **Values use strict `equals`**
(IEEE-754 NaN non-self-match).

**Key-collision rule:** `m.set(1, "a"); m.set(1.0, "b")` results in
`{1 => "b"}` — `len() == 1`. The second `set` overwrites the first
because `set_eq(1, 1.0)` is true. Document in constructor doc-comment
and CHANGELOG — users will hit it.

## Test plan

**VM tests (≥22)** in `src/vm/map_tests.rs`:

- `vm_map_empty`, `vm_map_from_pairs`, `vm_map_from_array`,
  `vm_map_get`, `vm_map_get_missing` (Null), `vm_map_set`,
  `vm_map_set_overwrite_preserves_position`, `vm_map_has_true`,
  `vm_map_has_false`, `vm_map_remove`, `vm_map_remove_missing`,
  `vm_map_keys`, `vm_map_values`, `vm_map_len`, `vm_map_int_keys`,
  `vm_map_tuple_keys`, `vm_map_float_key_collides_with_int`,
  `vm_map_equality_order_independent`, `vm_map_inequality`,
  `vm_map_typeof`, `vm_map_display`, `vm_map_contains_builtin`,
  `vm_map_is_truthy`, `vm_map_iteration_k_v`,
  `vm_map_nested_equality`, `vm_map_in_set_dedups`.

**Interpreter tests (≥24)** mirror VM + in-place mutation
(`let mut m; m.set(k, v)` path, `m.remove(k)` path) and frozen
rejection.

**Parity fixtures** cover cross-backend consistency including nested
maps and map iteration.

**Extra targeted tests:**

- `set([map([(1,"a")]), map([(1,"a")])]).len() == 1` — Map equality
  exercised through Set dedup. Validates equality wiring end-to-end.
- `map([("outer", map([("inner", 1)]))])` — nested Maps: display,
  equality, JSON error on non-string keys (nested case).
- `for k, v in {a: 1, b: 2}` VM regression test — confirms the bonus
  Object parity win didn't regress.

## Risks

1. **O(n) linear scan on get/has/set-overwrite.** For 10-element maps
   it's invisible; for 10K-element maps it's noticeable. Document in
   CHANGELOG that `Map` is for small-to-medium key spaces and `Object`
   remains the right choice for string-keyed lookups that need fast
   access. Do not add hashing now — ship simple, optimize later if
   benchmarks demand.
2. **Typechecker interaction.** Will infer as `Any`. Do not add a
   `Map<K, V>` generic type in this PR — runtime type only, matches
   Set's scope.
3. **`ObjKind` enum size.** `Vec<(Value, Value)>` is 16-byte pairs;
   largest existing variant (`Function`/`Object`) is already similar.
   Run `std::mem::size_of::<ObjKind>()` before/after; if it grows, box
   the vec. Probably fine.
4. **Method dispatch collision with Object.** `get_object_fields`
   (`src/vm/builtins.rs:3286`) strictly matches `ObjKind::Object`, so
   Map receivers fall through cleanly to the new dispatch block. No
   collision. Verified in plan review.

**Not risks (dropped from earlier draft):**

- ~~Bytecode serialization minor-version bump.~~ Not needed — Sets
  aren't serialized (constructed via `set(...)` function call, not a
  literal in the constant pool). Map is the same. `VERSION_MINOR`
  stays at 1.
- ~~JIT touch.~~ Not needed — `type_analysis.rs` only reasons about
  numeric specialization. Map never enters JIT-compiled code paths.

## Rollback

If the VM `var2` destructure work fights us (shouldn't — it's
~15 lines — but just in case), fall back to:

1. Ship Map in the interpreter only.
2. VM Map iteration requires `for pair in m.to_array() { let k =
pair[0]; let v = pair[1]; ... }` as a documented workaround.
3. Log a CLAUDE.md learning about the VM `var2` gap.
4. File a follow-up item on the roadmap for the compiler fix.

The M9.3 roadmap item does not specify VM parity as a hard requirement,
so this fallback is acceptable — but the primary plan is full parity.
