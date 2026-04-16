# M9.5 — Enum methods via `impl` blocks on algebraic `type` definitions

## TL;DR

**The feature is already implemented.** Both the interpreter and the VM
already parse `impl Type { fn method(it, ...) { ... } }` blocks where
`Type` is an algebraic `type`, register methods into `method_tables`,
and dispatch `instance.method(args)` correctly. Pattern matching via
`match it { Variant(x) => ... }` also works on both backends.

Hands-on audit (below) confirms: basic dispatch, extra args, returning
new ADT values, chained method calls, and JIT all work end-to-end on
both `--vm` (default) and `--interp`.

So M9.5 is **not** a new-feature milestone — it is a **coverage and
confidence** milestone. The checklist item exists because no one has
formally verified the surface yet, and there are zero dedicated tests.

## Scope of this PR

1. **Interpreter test suite** (~22 tests) covering the full enum-method
   surface in `src/interpreter/tests.rs`.
2. **VM test suite** (~22 tests) mirroring the interpreter suite under a
   new `src/vm/enum_methods_tests.rs` module.
3. **Parity fixtures** (~8 `.fg` files under `tests/parity/supported/`)
   so the cross-backend harness catches any future divergence.
4. **Example file** `examples/enum_methods.fg` demonstrating idiomatic
   enum methods on a `Shape` ADT — shipped as docs.
5. **CHANGELOG** entry under `[Unreleased] > Added`.
6. **CLAUDE.md** Core Builtins section — one line noting impl blocks on
   ADTs.
7. **ROADMAP.md** checkbox flip.

Out of scope (tracked as followups, not blockers):

- Fixing the built-in `None` vs user-defined `None` pattern conflict
  (separate bug; not M9.5's problem).
- Adding `when it { is Constructor(x) -> ... }` syntax (M9 design
  question, not implementation).
- Trait-like `impl Ability for Type { }` on ADTs (that is M9.6).
- `Self` return / parameter types on enum methods — not supported today;
  test #35 pins current behavior so the future implementation is a
  visible diff rather than a surprise.
- **Default method implementations on ADTs** — e.g.
  `impl Default for Shape { fn default() -> Self { Circle(1.0) } }`.
  The roadmap line "Enum methods via impl blocks" does not require this,
  but a reader might reasonably expect it; listing explicitly so the
  next engineer doesn't think M9.5 already covered it.

## Audit findings

Tested against commit `4f70971` (main, post-M9.4 merge).

### What works

| Scenario                                            | VM  | Interp | JIT |
| --------------------------------------------------- | --- | ------ | --- |
| `impl Shape { fn area(it) { ... } }` basic dispatch | ✅  | ✅     | ✅  |
| Method with extra args: `it.scale(2.0)`             | ✅  | ✅     | ✅  |
| Method returning new ADT: `Circle(r * factor)`      | ✅  | ✅     | ✅  |
| Chained: `c.scale(2.0).area()`                      | ✅  | ✅     | ✅  |
| `match it { Variant(x) => expr }` in method body    | ✅  | ✅     | ✅  |
| Zero-field variant match: `Square(_) =>`            | ✅  | ✅     | ✅  |
| Multiple methods on same type                       | ✅  | ✅     | ✅  |
| Method calls across variants (sum type dispatch)    | ✅  | ✅     | ✅  |

### Known quirks (not M9.5 scope)

- **`None` as pattern on user-defined ADT:** The interpreter's
  `match_pattern` at `src/interpreter/mod.rs:4314-4316` special-cases
  `Binding("None")` to match `Value::None` (the built-in). User-defined
  `type Foo = A | None` conflicts. File as separate bug. Workaround in
  tests: use variant names that don't collide with built-in Option/Result.
- **`when` with constructor patterns:** `when it { is Some(r) -> ... }`
  doesn't parse — `when` guards don't support destructuring. This is a
  design decision for M9.x, not part of enum methods.

### Test matrix (interpreter + VM, 35 each)

**Core dispatch:**

1. Single-variant type, method returns primitive
2. Two-variant type, method returns primitive per variant (dispatch via match)
3. Method with one extra arg
4. Method with multiple extra args
5. Method returns new ADT instance (same type)
6. Method returns new ADT instance (different type)
7. **Variant-converting method** — `Circle(r).to_square() → Square(r*2)` (per B1; exercises `_0` projection + different-variant constructor in the same method body)
8. Chained method calls: `x.a().b()`
9. Multi-variant type with field destructuring in match
10. Method that calls another method on `it`
11. Method that calls a free function passing `it`
12. Method body with `let` bindings and early return
13. Zero-field variant + data-carrying variant in same type
14. Method that returns a boolean (`is_circle`, `is_square`)
15. Method that reads a single field via `_0` access
16. Method with conditional `match`: falls through to wildcard
17. Three-variant type
18. Method taking a closure as arg
19. **Method body closes over `it`** (per R1) — `fn scale_all(it) { return [1,2,3].map(fn(x) { return x * it._0 }) }`. Exercises upvalue capture of the ADT value.
20. **Nested ADT recursion** (per M1) — `type Tree = Leaf(int) | Node(Tree, Tree); impl Tree { fn sum(it) { ... } }` with constructor-recursive fields, not just self-recursion on a flat variant.
21. Method that throws via `must err(...)`
22. Method dispatched from inside another method (nested dispatch)
23. Type annotation on method argument: `fn area(it, multiplier: float)`

**Collection-builtin integration (per M2):**

24. `shapes.map(fn(s) { return s.area() })`
25. `shapes.filter(fn(s) { return s.is_circle() })`
26. `shapes.reduce(0.0, fn(acc, s) { return acc + s.area() })`
27. `shapes.sort(fn(a, b) { return a.area() - b.area() })`

**Static methods (per B3):**

28. `Shape.unit_circle()` — zero-arg static constructor returning an ADT instance
29. `Shape.from_radius(r)` — static with one arg returning an ADT instance
30. Calling an instance method as a static → expect clean error message (pinned)
31. Calling a static method on an instance → expect clean error message (pinned)

**Error path coverage (per M3):**

32. `shape.nonexistent_method()` → error message is deterministic and mentions the method name
33. `shape.area(99)` (wrong arity — area takes `it` only) → error message mentions expected vs got
34. `shape.area` (field access without call) → error or method value; pin the current behavior

**Pinning assertions for out-of-scope quirks (per B2, M5):**

35. **`Self` return type is not yet supported** — `fn new() -> Self` either parses-as-string-ident and erases, or errors. Test asserts current behavior so a future `Self` implementation shows up as a diff, not a surprise.

Plus **one interpreter-only test** pinning that `when it { is Circle(r) -> r }` does not parse (per M5). VM twin unnecessary because parser is shared.

### Parity fixtures (12 .fg files, per M4)

1. `enum_methods_basic.fg` — Shape.area() on Circle
2. `enum_methods_dispatch.fg` — match across variants
3. `enum_methods_return_adt.fg` — Shape.scale() returning new Shape
4. `enum_methods_chained.fg` — `c.scale(2.0).area()`
5. `enum_methods_predicate.fg` — is_circle / is_square
6. `enum_methods_with_arg.fg` — method with extra arg
7. `enum_methods_nested_dispatch.fg` — method calling another method
8. `enum_methods_recursive.fg` — tree traversal via recursion
9. `enum_methods_static.fg` — static constructor returning ADT
10. `enum_methods_nested_adt.fg` — `Tree = Leaf | Node(Tree, Tree)` sum
11. `enum_methods_error_no_method.fg` — error path, missing method
12. `enum_methods_error_arity.fg` — error path, wrong arity

## Files touched

- `src/interpreter/tests.rs` — +36 test functions (35 in matrix + 1 `when`-quirk pin)
- `src/vm/mod.rs` — `#[cfg(test)] mod enum_methods_tests;`
- `src/vm/enum_methods_tests.rs` — NEW, +35 test functions mirroring interpreter
  (first skim an existing VM test module like `src/vm/stream_tests.rs` to keep the harness convention — `parse_program`/`vm_output`/`vm_run` helpers)
- `src/interpreter/mod.rs` — remove stale `// Phase 4 will fully implement method tables + dispatch` comment at line 1193 (Phase 4 happened)
- `tests/parity/supported/enum_methods_*.fg` — NEW, 12 files
- `examples/enum_methods.fg` — NEW
- `CHANGELOG.md` — entry under `[Unreleased] > Added` (wording: "verified and test-covered enum methods via `impl` blocks on algebraic `type` definitions — feature was latent, now locked in by 70+ tests")
- `CLAUDE.md` — Core Builtins note
- `ROADMAP.md` — checkbox flip (in a separate commit on main after merge)

## Risks

- **"Tests-only PR" risk:** If the existing impl-block code silently
  regresses, there is currently no alarm. These tests are the alarm.
  That is the point.
- **Built-in Option/Result name collisions:** Any test that uses
  `Some`/`None`/`Ok`/`Err` as user-defined variants will hit the
  interpreter's special-cased pattern matching. Mitigation: the test
  suite uses non-colliding variant names (`Shape`, `Tree`, `Expr`, etc.)
  throughout. The tests also explicitly document that collision is a
  known gap.
- **VM-interpreter parity:** Every interpreter test has a VM twin, and
  every parity fixture runs the bytecode round-trip. Divergences will
  surface at CI time, not in prod.
- **JIT coverage is not free from the parity harness** (per R3).
  Before writing the test body, verify whether `src/testing/parity.rs`
  actually runs `--jit` alongside interpreter/VM/round-trip. If not,
  add at least a manual smoke assertion in the test plan:
  `./target/debug/forge --jit run examples/enum_methods.fg` — JIT
  silently falling back to VM is a historical Forge footgun (see the
  fib(30) learning in CLAUDE.md).
- **Method-table inheritance across child VMs** (spawn / schedule /
  watch). `src/vm/machine.rs:192, 865-874, 2146` store `method_tables`
  and must copy them into `fork_for_spawn`. If any test spawns a child
  VM that calls an ADT method, and the child sees an empty method
  table, dispatch will fail with "no such method." Not in the current
  matrix because spawn-stream tests in M9.4 already cover closure
  transfer — but flag it as a known watch-point. Add a quick sanity
  test: `spawn { let s = Circle(1.0); say s.area() }` and assert it
  prints `3.14`.
- **Harness convention drift.** Before writing 35 VM tests, skim
  `src/vm/stream_tests.rs` (M9.4) to confirm the `parse_program` /
  `vm_output` / `vm_run` helper shape and reuse it verbatim. Otherwise
  it's a rewrite.

## Test plan

- `cargo test --lib` → all existing passing tests stay green, ~72 new
  tests pass (36 interpreter + 35 VM + 1 spawn-child-VM sanity).
- `cargo test` → pre-existing `lsp::tests::references_respects_word_boundaries`
  failure unrelated; everything else green.
- `./target/debug/forge run examples/enum_methods.fg` → exits 0, prints
  expected output.
- `./target/debug/forge --interp run examples/enum_methods.fg` → same.
- `./target/debug/forge --jit run examples/enum_methods.fg` → same
  (JIT smoke, per R3).
- Parity harness run: `cargo test parity_` → all 12 new fixtures pass
  across interpreter / VM / bytecode round-trip / JIT (if JIT is
  harnessed; otherwise manually verified above).
- `./target/debug/forge --interp run examples/enum_methods.fg` → same.
- `./target/debug/forge --jit run examples/enum_methods.fg` → same.

## Rollback plan

Tests-only PR; revert the branch if a regression is found. No user-facing
behavior changes.

## Phases

1. Audit the existing implementation (done — this document).
2. Write interpreter tests.
3. Write VM tests.
4. Write parity fixtures.
5. Write example file.
6. Run full test suite; fix any discovered gaps.
7. Update CHANGELOG + CLAUDE.md.
8. rust-expert adversarial review; revise.
9. PR → rust-expert final review → merge → flip roadmap.
