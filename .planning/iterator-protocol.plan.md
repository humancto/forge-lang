# M9.4 — Iterator protocol

**Roadmap item:** `Iterator protocol with .stream(), lazy .filter(), .map(), .take(), .skip(), .collect()` (ROADMAP.md:585)

**Status:** Revised after rust-expert adversarial review (verdict: REVISE → changes applied → ready to implement).

## Goal

Add a first-class lazy `Stream` type to Forge so users can compose pipelines without intermediate allocation:

```forge
let result = [1, 2, 3, 4, 5]
  .stream()
  .filter((x) => x > 1)
  .map((x) => x * 10)
  .take(3)
  .collect()
// => [20, 30, 40]
```

Must work across interpreter, VM, bytecode round-trip, and JIT (JIT falls back to VM for Stream ops — `type_analysis::analyze` marks them unsupported).

## What's in scope

**Sources:**

- Array
- Tuple
- Set
- Map (yields `(k, v)` 2-tuples, mirrors `for k, v in m`)
- String (yields single-character strings to match `for c in "hi"` semantics — **not** `char`, confirmed against existing interpreter/VM iteration)
- Range: `range(n)` / `range(a, b)` already return arrays; out of scope for this PR.

**Lazy combinators:**

- `.filter(pred)` — keeps items where `pred(x)` is truthy
- `.map(fn)` — transforms items through `fn`
- `.take(n)` — first n items
- `.skip(n)` — skip first n items
- `.chain(other_stream)` — concatenate two streams
- `.zip(other_stream)` — pair items (stops at shorter)
- `.enumerate()` — yields `(index, item)` tuples

**Terminal (eager) operations:**

- `.collect()` — drain into an Array
- `.to_array()` — alias for `.collect()` (discoverability)
- `.count()` — drain and return item count
- `.reduce(fn, init)` — fold
- `.for_each(fn)` — side effects, returns Null
- `.first()` — first item or Null
- `.sum()` — drain and sum numeric items (Int stays Int; any Float promotes result to Float; non-numeric → `RuntimeError`)
- `.find(pred)` — first item matching predicate
- `.any(pred)` / `.all(pred)` — boolean short-circuit

**Trigger:** `.stream()` method on any of the source types.

**Out of scope:** user-defined iterators, flat_map, infinite streams, channel/file streams, parallel iteration.

## Representation

### Interpreter

```rust
enum StreamKind {
    ArrayIter   { items: Vec<Value>, idx: usize },
    TupleIter   { items: Vec<Value>, idx: usize },
    SetIter     { items: Vec<Value>, idx: usize },
    MapIter     { pairs: Vec<(Value, Value)>, idx: usize },
    StringIter  { chars: Vec<char>, idx: usize },
    Filter      { upstream: Rc<RefCell<StreamCell>>, pred: Value },
    Map         { upstream: Rc<RefCell<StreamCell>>, fn_val: Value },
    Take        { upstream: Rc<RefCell<StreamCell>>, remaining: usize },
    Skip        { upstream: Rc<RefCell<StreamCell>>, skip_n: usize, started: bool },
    Chain       { first: Rc<RefCell<StreamCell>>, second: Rc<RefCell<StreamCell>>, on_second: bool },
    Zip         { left: Rc<RefCell<StreamCell>>, right: Rc<RefCell<StreamCell>> },
    Enumerate   { upstream: Rc<RefCell<StreamCell>>, idx: usize },
}

struct StreamCell {
    kind: StreamKind,
    poisoned: Option<RuntimeError>, // set if a user closure errored; future next() returns same error
}

// Value::Stream wraps Rc<RefCell<StreamCell>>.
```

**Borrow rules (showstopper #1 fix):**

- A terminal operation calls `next()` in a **loop at the top level**, holding no persistent borrow across calls. Each `next()` takes `&RefCell<StreamCell>` and calls `try_borrow_mut()`.
- If `try_borrow_mut` fails (e.g., re-entrancy because the same `Rc` appears twice and is being walked), return `RuntimeError("stream already in use")`. No silent panic.
- `zip(a, a)` with two `Rc` clones of the _same_ cell is explicitly detected: each `next()` on the zip borrows left then drops then borrows right. If they alias, the right borrow succeeds (not nested), yielding `(x0, x1), (x2, x3), ...`. Document this in a fixture.

**Iterative, not recursive (showstopper #2 fix):**

Every combinator's `next` is written as an explicit `loop`, not a recursive self-call. `Filter::next` pulls from upstream in a `loop { match upstream.next() { Some(v) if pred(v) => return Some(v), Some(_) => continue, None => return None } }`. No Rust-level recursion — a 1M-element `.filter(always_false)` must not blow the stack. Same pattern for `Skip`, `Chain`, and any combinator that "keeps pulling."

**Error handling / poisoning (bug #7 fix):**

If a user closure (pred, fn_val, reduce fn) errors mid-pipeline, the error is stored in `StreamCell::poisoned` and propagated. All subsequent `next()` calls return the same error. Terminals surface the error via `?`. Rationale: partial drains leave cursors in indeterminate state; re-trying would be surprising.

### VM

`ObjKind::Stream(StreamBox)` where `StreamBox` carries:

```rust
struct StreamBox {
    kind: StreamKind,        // VM version with Value fields (Value is Copy + NaN-boxed)
    poisoned: Option<VMError>,
}

enum StreamKind {
    ArrayIter  { items: Vec<Value>, idx: usize },
    TupleIter  { items: Vec<Value>, idx: usize },
    SetIter    { items: Vec<Value>, idx: usize },
    MapIter    { pairs: Vec<(Value, Value)>, idx: usize },
    StringIter { chars: Vec<char>, idx: usize },
    Filter     { upstream: Value, pred: Value },
    Map        { upstream: Value, fn_val: Value },
    Take       { upstream: Value, remaining: usize },
    Skip       { upstream: Value, skip_n: usize, started: bool },
    Chain      { first: Value, second: Value, on_second: bool },
    Zip        { left: Value, right: Value },
    Enumerate  { upstream: Value, idx: usize },
}
```

`upstream: Value` is `Value::obj(gc_ref)` pointing at another `ObjKind::Stream`. Advancement uses `gc.get_mut` on the stream ref — but **never across a user closure call**.

**Borrow protocol (showstopper #3 fix — GC + iterative):**

The VM adds a single helper: `fn stream_next(&mut self, s: GcRef) -> Result<Option<Value>, VMError>`. Inside:

1. Check `poisoned` (peel via `gc.get`, clone the error, drop borrow).
2. Peel the cursor state (e.g., `items.get(idx).copied()` + bump idx via `gc.get_mut`, then drop borrow).
3. For combinators: peel the upstream ref + closure Value, drop borrow, call `self.stream_next(upstream_ref)` iteratively in a `loop`, then call `self.call_value(pred)` with no borrow held, then (on success) nothing to write back — the state is already advanced upstream.
4. If a closure errors, peel the stream ref again, write `poisoned = Some(err.clone())`, return `Err(err)`.

Iterative loops for `Filter` / `Skip` / `Chain` mirror the interpreter. No recursion into `stream_next` on the same stream — only on the _upstream_ stream, which has its own independent `GcRef`. Rust stack depth = pipeline depth, not input length.

**GC trace (showstopper #3 fix — explicit):**

`ObjKind::Stream(sb)` trace visitor:

1. Walks `sb.kind` and calls `Value::as_obj()` on every `Value` field in every variant. For `Filter`: `upstream.as_obj()`, `pred.as_obj()`. For `Chain`: `first`, `second`. For `Zip`: `left`, `right`. For the source variants: every element of `items` / `pairs` (both `k` and `v`).
2. Each returned `Option<GcRef>` is pushed to the worklist.
3. Closure Values reached this way will recursively trace their upvalues via the existing `ObjKind::Closure` trace path.

**Test (missing #14, #15):** a GC-pressure test that allocates thousands of transient objects while a stream is half-drained with a closure capturing a heap upvalue. Assert the upvalue is still alive after GC.

### SharedValue / VM↔interp boundary (bug #6 fix)

`SharedValue::Stream` is **not** added. Conversions fail loudly:

- `value_to_shared(Stream)` → returns a sentinel that, when used, errors with `"Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize"`.
- Same for `shared_to_value` and `convert_to_interp_val` / `convert_interp_value`.
- Concretely: instead of a silent `=> Null` arm, emit a `VMError` / `RuntimeError` with the above message.

Rationale: Forge auto-falls back to the interpreter for some code paths. A silent Null coercion would produce baffling `Null has no method .collect` errors downstream.

### Display

`Stream(<kind>)` where `<kind>` is e.g. `ArrayIter`, `Filter`, `Map` — more useful for debugging than `Stream(...)`. Does not drain.

### typeof → `"Stream"`

### Frozen

`freeze(stream)` errors: `"cannot freeze a Stream (streams are single-use; freeze the result of .collect() instead)"`.

### Truthiness

Streams are always truthy (like functions).

### Equality (bug #8 fix)

`Stream == Stream` compares **by pointer identity** (Rc identity for interpreter, GcRef identity for VM). `let s = xs.stream(); s == s` is `true` (reflexive). Two distinct streams over equal sources are `false`. This keeps `==` reflexive for `assert_eq`.

### Bytecode serializer (risk #13 fix)

Streams are runtime-only and must never reach the bytecode serializer. Add an explicit `panic!("BUG: Stream value reached bytecode serializer")` / `unreachable!` arm in `Chunk::add_constant` / constant-pool encoding so a regression is loud.

### JIT (risk #9 fix — actively audit, don't assume)

Before Phase 5: read `src/vm/jit/type_analysis.rs`. Confirm that method-call opcodes dispatching on unknown `ObjKind` mark the function as JIT-ineligible (`has_unsupported_ops = true`). If not, **add an explicit Stream arm**. Do not assume. Add a regression test: a function containing `.stream()` must not be JIT-compiled.

## Phases

### Phase 1 — Foundation (both backends)

- Add `Value::Stream(Rc<RefCell<StreamCell>>)` to interpreter, `ObjKind::Stream(StreamBox)` to VM.
- Add `trace` visiting every `Value` field in every `StreamKind` variant.
- `type_name → "Stream"`, `display → Stream(<kind>)`, `is_truthy = true`, `to_json_string → error`.
- `obj_equals` — pointer identity.
- `value_to_shared` / boundary conversions: explicit error, not Null.
- Bytecode serializer: `unreachable!` arm.
- No `.stream()` method yet, no combinators.

### Phase 2 — Interpreter: sources + eager `.collect()` / `.to_array()` / `.count()` / `.for_each` / `.first()`

- Method dispatch for `.stream()` on Array / Tuple / Set / Map / String.
- `.collect()` drains upstream into Array (also `.to_array()` alias).
- `.count()`, `.for_each(fn)`, `.first()`.
- String iteration yields single-character strings — confirm match with existing `for c in "hi"` behavior first.
- Tests.

### Phase 3 — Interpreter: lazy combinators

- `.filter`, `.map`, `.take`, `.skip` (with `started: bool` flag — bug #4), `.enumerate`, `.chain`, `.zip`.
- All combinators implemented as `loop`, no recursive `next()`.
- `.skip` uses `{skip_n, started}` so `Skip { skip_n: 0, started: false }` is distinguishable from `Skip { skip_n: 0, started: true }`.
- Poisoning on user closure errors.
- Tests: each combinator in isolation + interleaved.

### Phase 4 — Interpreter: remaining terminals

- `.reduce`, `.sum` (Int-stays-Int, any-Float-promotes, non-numeric errors — bug #5), `.find`, `.any`, `.all` (short-circuit).

### Phase 5 — VM Phase 1: type + dispatch scaffolding

- `ObjKind::Stream(StreamBox)`, trace, type_name, display, obj_equals, is_truthy, frozen rejection.
- Boundary conversions → error.
- Bytecode serializer → `unreachable!`.
- `src/vm/jit/type_analysis.rs` audit + explicit Stream arm if needed.
- No method dispatch yet.

### Phase 6 — VM Phase 2: `.stream()` sources + eager terminals

- `.stream()` dispatch on Array/Tuple/Set/Map/String in VM's `call_forge_method`.
- Stream method-dispatch block (after Map block).
- `.collect()` / `.to_array()` / `.count()` / `.for_each` / `.first()`.
- **Critical:** Every closure call follows the peel-cursor → drop-borrow → call pattern (see sort_by). Verify with a GC-under-load test.

### Phase 7 — VM Phase 3: lazy combinators

- All combinators. Each allocates a new `ObjKind::Stream(StreamBox)` with its upstream stored as `Value::obj(upstream_ref)`.
- `stream_next` helper is iterative and never holds a borrow across user closure calls.
- Tests mirror Phase 3.

### Phase 8 — VM Phase 4: remaining terminals

- `.reduce`, `.sum`, `.find`, `.any`, `.all`.

### Phase 9 — JIT parity fixture

- Parity fixture exercising a stream pipeline under all four backends.
- Regression test that a function with `.stream()` is marked unsupported.

### Phase 10 — Tests + parity fixtures

- `src/vm/stream_tests.rs` — ≥35 VM tests covering sources, each combinator, each terminal, pipelines, and these specific cases:
  - Empty source on every terminal.
  - `.take(0)` yields nothing.
  - `.skip(n)` where `n > len` yields nothing.
  - `.take(n)` where `n > len` yields `len`.
  - `.zip` unequal lengths stops at shorter.
  - `.chain` with empty first or second.
  - `.filter` filtering everything out.
  - `.map` to Null.
  - **Re-entrancy:** `zip(a, a)` — assert documented behavior (pairs `(x0,x1),(x2,x3)`).
  - **Terminal on already-drained stream** (each terminal, missing #16).
  - **Short-circuit** for `.any` / `.all` using a side-effectful counter (missing #18).
  - **Error propagation + poisoning:** closure throws, terminal errors, second terminal call returns the same error.
  - **GC pressure mid-pipeline** with closure upvalue capture (missing #14, #15).
  - **Boundary rejection:** attempting to pass a Stream across VM/interp errors.
  - **Int/Float sum promotion** (bug #5).
- `src/interpreter/tests.rs` — mirror ≥35 tests under a new "Stream Tests" section (fix typo from earlier draft — nit #26).
- `tests/parity/supported/stream_*.fg` — **≥18 fixtures** (bumped from 12 per missing #19) covering:
  1. every terminal at least once
  2. every combinator at least once
  3. two long pipelines
  4. empty-source on every terminal
  5. error propagation through a throwing closure (if parity supports error-expect)
  6. single-use re-drain yields empty
  7. `zip(a, a)` documented behavior
  8. `chain(a, b)` where `a` is separately drained first (risk #12)

### Phase 11 — Documentation + CHANGELOG + PR

- `CHANGELOG.md` `[Unreleased]`: new Stream type, sources, combinators, terminals, single-use semantics, boundary-error behavior (bug #6).
- `CLAUDE.md` Core Builtins: mention `.stream()` under Functional.
- Open PR, rust-expert review on the merged diff, address Showstoppers/Bugs, merge on APPROVE.
- Flip ROADMAP.md line 585.

## Rollback plan

`Value::Stream` / `ObjKind::Stream` are additive. Revert the branch cleanly. No bytecode format change.

## Risk list

1. **VM GC borrow hazards** — mitigated by the strict peel-then-call pattern and `stream_next` helper; the biggest risk vector. Tests must exercise GC under load.
2. **Re-entrancy / `try_borrow_mut` errors** (interpreter) — `zip(a, a)` and closures that re-enter. Fix: `try_borrow_mut` returns an error instead of panicking. Documented fixture.
3. **Stack depth** — mitigated by iterative `next` implementations; recursion is only via upstream `next()`, which is bounded by pipeline depth, not input length.
4. **Closure lifetime under GC** — mitigated by explicit trace visitor walking every `Value` field in every `StreamKind` variant.
5. **Poisoning correctness** — closure errors must set `poisoned` before returning; test that the second terminal on the same stream yields the same error.
6. **Boundary crossing** — fails loudly, never silently coerces.
7. **Infinite loops** — no infinite sources in this PR.
8. **`.first()` name collision** with the existing `first` free builtin — method dispatch takes precedence on `Stream` values; test both orderings (nit #24).
9. **`chain(a, b)` where user separately drains `a`** — documented behavior: `chain` will see `a` as empty. Fixture.

## Integration points

- `src/interpreter/mod.rs` — Value enum, method dispatch in `Expr::Call → FieldAccess`, PartialEq, container_eq, type_name, Display, to_json_string, is_truthy, obj_equals.
- `src/vm/value.rs` — `ObjKind::Stream`, trace, type_name, display, obj_equals, to_json_string (error), value_to_shared (error), shared_to_value (error).
- `src/vm/nanbox.rs` — is_truthy Stream arm → true.
- `src/vm/builtins.rs` — Stream method dispatch block in `call_forge_method`; `.stream()` dispatch on Array/Tuple/Set/Map/String; `stream_next` helper.
- `src/vm/machine.rs` — `convert_to_interp_val` / `convert_interp_value` Stream arms (error, not Null).
- `src/vm/jit/type_analysis.rs` — audit + explicit Stream arm if missing.
- `src/vm/bytecode.rs` (or wherever constant pool serializes) — `unreachable!` arm.
- `src/vm/stream_tests.rs` — new file.
- `src/interpreter/tests.rs` — new "Stream Tests" section.
- `tests/parity/supported/stream_*.fg` — ≥18 fixtures.

## Success criteria

- `[1,2,3,4,5].stream().filter((x)=>x>1).map((x)=>x*10).take(3).collect()` → `[20, 30, 40]` on interpreter, VM, and bytecode round-trip (JIT falls back cleanly).
- All nine terminals produce identical results across backends.
- All seven combinators produce identical results across backends.
- `zip(a, a)`, chain-empty, filter-all-out, error-poisoning, GC-under-pressure, and boundary-crossing tests all pass.
- Full `cargo test` suite green except preexisting LSP flake.
- Parity corpus (≥18 new fixtures) passes on all backends.

## Review status

- 2026-04-15: rust-expert adversarial review — verdict REVISE. Applied all 3 showstoppers, 5 bugs with concrete decisions, 5 risks, and all missing-test items. Nits 23 (StreamState → StreamKind), 25 (.to_array alias), 26 (typo) also applied. Nit 24 (.first dispatch priority) captured in risk list.
