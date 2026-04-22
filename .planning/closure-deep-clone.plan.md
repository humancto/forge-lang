# Closure Deep-Clone -- Close the Per-Request Fork to Sound Isolation

## TL;DR

PR #108 made `fork_for_serving` deep-clone `env` so top-level scope storage
is independent per request. **But** the closure fields on `Value::Function`
and `Value::Lambda` are still shallow over `Vec<Arc<Mutex<HashMap>>>`,
so two concurrent requests calling the same captured-closure helper share
scope `Arc`s through the closure. The contract in CLAUDE.md says "handlers
must be pure functions of `(args) -> response`" -- but nothing in the
runtime detects or warns when a user violates it, and the worst case
(`Value::Lambda` with its writeback at `interpreter/mod.rs:4317`) is a
silent lost-update race across concurrent requests.

This PR walks `Value` during a **new, opt-in** isolated-deep-clone path
used only by `fork_for_serving`. Existing `spawn_task` and
`fork_for_background_runtime` call sites keep their current shallow-on-
closures behavior because their semantics are different (see `Why two
flavors`).

> Reviewed by `rust-expert`: **REVISE -> addressed below in `Review-driven
> changes`**. Showstoppers: silent `spawn_task` semantic break (resolved by
> introducing two flavors), dead `MutMap` parameter (removed), Stream
> sharing hazard (now an explicit forbid + assert), too-loose cost gate
> (tightened from 5ms to 1ms).

## Scope (in)

1. **New `Environment::deep_clone_isolated`** that recursively rewrites
   the closure fields on Function and Lambda values. Used **only**
   by `fork_for_serving`.
2. **`Environment::deep_clone` stays as-is** (scopes deep, values shallow).
   Continues to back `spawn_task` and `fork_for_background_runtime` --
   their shared-mutable-closure semantics are preserved by design (see
   below).
3. Cycle handling via Arc-identity memoization: a
   `HashMap<*const Mutex<...>, Arc<Mutex<...>>>` maps old scope Arcs to
   their new clones. The recursive-function self-reference case
   (FnDef double-define at `mod.rs:1130-1153` puts an Arc-cycle through
   the top-level scope) reuses the same outer scope Arc instead of
   re-allocating mid-walk.
4. **Stream-in-template-env safety check.** Add a debug-assert (release
   build: skip) in `fork_for_serving` that walks the template env once
   at first fork and panics if it finds a `Value::Stream`. Streams are
   single-use and silently break under sharing across forks; better to
   detect at boot than at first request.
5. New unit tests:
   - **Lambda mutation isolation across forks.** Two forks each call a
     captured-counter pattern repeatedly; assert each fork's counter is
     independent and Arc-pointer-distinct.
   - **Captured-closure helper isolation.** A non-global Function with
     captured outer state, called concurrently from two forks, no
     race.
   - **Recursive function survives the cycle.** A self-referential
     fn defined in template, called from a fork, computes correctly and
     does not infinite-loop on the cycle.
   - **`spawn_task` semantics preserved.** A captured Lambda mutated
     via spawn -- assert mutations accumulate (the existing shared-state
     contract).
   - **Cost gate.** `fork_for_serving` < 1ms median.
6. New `examples/bench_server_closure.fg` and matching test case in
   `tests/server_concurrency.rs` proving the closure-handler isolation
   under load.

## Scope (out)

- Changing the representation of Function/Lambda values. Closures stay
  as `Environment` and `Arc<Mutex<Environment>>`.
- A `shared { ... }` block syntax for explicit cross-request state.
- Walking through `Value::Channel` and `Value::TaskHandle`. These are
  intentionally shared across forks (Channel: cross-request coordination
  is legitimate; TaskHandle: holding a handle from before the request
  started is rare and ambiguous, document only).
- Walking through `Value::Stream`. Forbidden in template env via the
  debug-assert; if it ever needs to work, that's a separate design.
- `fork_for_background_runtime` and `spawn_task` switching to isolated
  semantics. **Explicitly preserve their shared-closure behavior.**

### Considered and rejected: making the Lambda closure non-shareable

The third option -- change Lambda's closure type from
`Arc<Mutex<Environment>>` to `Box<Environment>` and remove the
writeback at line 4317 -- would kill BUG-005's mutable-closure-capture
feature (a captured-counter pattern would no longer accumulate across
calls). That's a real language-semantics regression for Forge users
who rely on mutable closure capture as an idiom. Rejected.

### Considered and rejected: the smaller fix

Skip closure deep-clone when `closure.scopes.len() == 1` (global fns).
**True** that global fns hit the `is_global_fn` fast path and don't
read the closure, so sharing is harmless for them. **False** that this
is the whole story -- non-global functions defined inside expressions
(if-blocks, match arms) and Lambdas always need the deep-clone. The
cycle case still arises through the recursive-function double-define
at the top level. So the smaller fix is "skip for global fns, walk
otherwise" -- which is `if scopes.len() > 1 { walk }` plus
`always walk Lambda`. Marginal code savings, branchier logic. Rejected
in favor of the universal walk for simplicity.

## Why two flavors (the load-bearing decision)

| Caller | Today | After this PR | Rationale |
|---|---|---|---|
| `spawn_task` (squad blocks) | `env.deep_clone()` (scopes deep, closures shallow) | unchanged | User opted into concurrency. Shared closure state across spawns is the *expected* model -- a captured-counter pattern across two spawns wants accumulation. |
| `fork_for_background_runtime` (schedule/watch) | `env.deep_clone()` (scopes deep, closures shallow) | unchanged | Background blocks at startup; mutations from a periodic schedule legitimately want to be visible to the next iteration. |
| `fork_for_serving` (HTTP per-request) | `env.deep_clone()` (scopes deep, closures shallow) | **`env.deep_clone_isolated()` (scopes AND closures deep)** | Implicit fork -- user did not opt into anything. Must be sound by default. Cross-request mutation is a race condition. |

This split is the correct semantic line. Spawn = "I want a thread,
sharing is the point." Schedule/watch = "I want a periodic task with
its own state continuity." HTTP request = "I want isolation." Three
different forks, two different deep-clone flavors. Old `deep_clone`
keeps the share-closures behavior; the new isolated variant does the
full walk.

## Design

### `Environment::deep_clone_isolated`

```rust
// New, opt-in. Only fork_for_serving calls this.
// Uses scope-Arc-identity memoization to handle the FnDef
// recursive-function cycle (see below).

type ScopeMap = HashMap<*const Mutex<HashMap<String, Value>>,
                        Arc<Mutex<HashMap<String, Value>>>>;

impl Environment {
    pub fn deep_clone_isolated(&self) -> Self {
        let mut scope_map = ScopeMap::new();
        Self::deep_clone_env(self, &mut scope_map)
    }

    fn deep_clone_env(env: &Environment, scope_map: &mut ScopeMap) -> Self {
        let scopes = env.scopes.iter()
            .map(|s| Self::dup_scope(s, scope_map))
            .collect();
        // mutability table is just String -> bool, no Values. Plain
        // shallow per-scope clone is fine -- matches existing deep_clone.
        let mutability = env.mutability.iter()
            .map(|m| {
                Arc::new(Mutex::new(
                    m.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                ))
            })
            .collect();
        Self { scopes, mutability }
    }

    fn dup_scope(
        s: &Arc<Mutex<HashMap<String, Value>>>,
        scope_map: &mut ScopeMap,
    ) -> Arc<Mutex<HashMap<String, Value>>> {
        let key = Arc::as_ptr(s);
        if let Some(existing) = scope_map.get(&key) {
            return existing.clone();   // cycle: this scope is already being cloned
        }
        // Tie-the-knot: insert an empty placeholder Arc BEFORE recursing
        // so a self-reference that comes back through the recursion
        // resolves to it. Nobody reads the placeholder's contents during
        // the walk -- dup_scope reads from the original (locked once
        // here), writes to a temp `new_map`, and installs it at the end.
        let new_arc = Arc::new(Mutex::new(HashMap::new()));
        scope_map.insert(key, new_arc.clone());

        let original = s.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut new_map = HashMap::with_capacity(original.len());
        for (k, v) in original {
            new_map.insert(k, Self::dup_value(v, scope_map));
        }
        *new_arc.lock().unwrap_or_else(|p| p.into_inner()) = new_map;
        new_arc
    }

    fn dup_value(v: Value, scope_map: &mut ScopeMap) -> Value {
        match v {
            Value::Function { name, params, body, closure, decorators } => {
                Value::Function {
                    name, params, body, decorators,
                    closure: Self::deep_clone_env(&closure, scope_map),
                }
            }
            Value::Lambda { params, body, closure } => {
                let captured = closure.lock().unwrap_or_else(|p| p.into_inner()).clone();
                let new_env = Self::deep_clone_env(&captured, scope_map);
                Value::Lambda {
                    params, body,
                    closure: Arc::new(Mutex::new(new_env)),
                }
            }
            // Containers: recurse so a Function/Lambda nested inside also
            // gets isolated.
            Value::Array(arr) => Value::Array(
                arr.into_iter().map(|x| Self::dup_value(x, scope_map)).collect()),
            Value::Tuple(arr) => Value::Tuple(
                arr.into_iter().map(|x| Self::dup_value(x, scope_map)).collect()),
            Value::Object(o) => Value::Object(
                o.into_iter().map(|(k, v)| (k, Self::dup_value(v, scope_map))).collect()),
            Value::Set(arr) => Value::Set(
                arr.into_iter().map(|x| Self::dup_value(x, scope_map)).collect()),
            Value::Map(pairs) => Value::Map(
                pairs.into_iter()
                    .map(|(k, v)| (Self::dup_value(k, scope_map),
                                   Self::dup_value(v, scope_map)))
                    .collect()),
            Value::ResultOk(b) => Value::ResultOk(Box::new(Self::dup_value(*b, scope_map))),
            Value::ResultErr(b) => Value::ResultErr(Box::new(Self::dup_value(*b, scope_map))),
            Value::Some(b) => Value::Some(Box::new(Self::dup_value(*b, scope_map))),
            Value::Frozen(b) => Value::Frozen(Box::new(Self::dup_value(*b, scope_map))),
            // Stream: forbidden in template env. Caller (fork_for_serving)
            // asserts on first fork that template env contains no Stream.
            // Channel/TaskHandle: intentionally shared across forks.
            // - Channel: cross-request coordination is a legitimate use case.
            // - TaskHandle: holding one across requests is rare and
            //   ambiguous; documented as "shared but probably not what
            //   you want" rather than forbidden.
            other => other,
        }
    }
}
```

### Cycle handling -- why the placeholder works (concrete trace)

`src/interpreter/mod.rs:1130-1153` (`Stmt::FnDef` execution):
1. Build a Function value with closure = `self.env.clone()` -- closure Arcs share with `self.env`.
2. `self.env.define(name, func)` -- top scope X now contains `name -> Function value with closure containing X`.
3. Build the recursive variant with closure = `self.env.clone()` (X now has `name` already inside it).
4. `self.env.define(name, recursive_func)` -- overwrites with the recursive version.

Result: top scope X contains `f`, where `f.closure.scopes[-1]` is *the same Arc as X*. **Cycle.**

When `dup_scope(X)` runs:
1. `key = Arc::as_ptr(&X)`. Allocate empty `new_arc`. Insert `(key -> new_arc)` into `scope_map`.
2. Lock original X, snapshot its contents to `original`.
3. Walk `original`: hit `f` whose closure includes X.
4. `dup_value(Function value)` -> `deep_clone_env(closure, scope_map)`.
5. That walks `closure.scopes`, eventually hitting X's Arc.
6. `dup_scope(X)` is reentered. `scope_map.get(&key)` returns the placeholder. **Returned without recursing.**
7. The new Function value's closure now contains the placeholder Arc (at the right vec position).
8. We return up the stack. `dup_scope(X)` finishes its walk, writes `new_map` into `new_arc`'s Mutex.
9. The placeholder, now containing the populated map, is the same Arc the new Function value's closure references.

Topological identity preserved. No infinite recursion.

### Stream safety in template env

```rust
impl Interpreter {
    pub fn fork_for_serving(&self) -> Self {
        // Debug-build only: walk the template env once and panic if it
        // contains a Value::Stream. Streams are single-use; sharing
        // them across forks silently breaks (first fork to drain wins).
        #[cfg(debug_assertions)]
        Self::assert_no_streams_in_env(&self.env);
        // ... existing fork body, now using deep_clone_isolated ...
    }

    #[cfg(debug_assertions)]
    fn assert_no_streams_in_env(env: &Environment) { /* ... */ }
}
```

Release builds skip the check (perf). Debug builds catch the footgun
the moment it's planted.

### Cost analysis

Per-fork extra work:
- Allocate `ScopeMap` (HashMap, lazy: zero allocations until first insert).
- Walk every reachable Value once. Recursion is bounded by env depth times stdlib values per scope.
- Stdlib has no Function/Lambda -- all `Value::BuiltIn(String)`. Each clones in ~50ns.
- ~25 modules with ~10-20 entries each = ~400 BuiltIn clones = ~20us total.
- Programs with N captured closures pay O(N) extra allocations.

Cost gate tightened from 5ms to **1ms** per the review. Today's baseline
is 0.057ms; expect 0.1-0.2ms after. 1ms is 5-10x headroom.

## Tasks

| # | File | Change |
|---|---|---|
| 1 | `src/interpreter/mod.rs` | Add `Environment::deep_clone_isolated`, `dup_scope`, `dup_value` (private). Keep `deep_clone` unchanged. |
| 2 | `src/interpreter/mod.rs` | `fork_for_serving` switches to `env.deep_clone_isolated()`. |
| 3 | `src/interpreter/mod.rs` | Add `Interpreter::assert_no_streams_in_env` (debug-only). Call from `fork_for_serving`. |
| 4 | `src/interpreter/tests.rs` | Add 5 tests: lambda-isolation, captured-closure-isolation, recursive-fn-survives-fork, spawn-task-still-shares (regression for the explicit non-change), fork-cost <1ms. |
| 5 | `examples/bench_server_closure.fg` (new) | Server fixture with a captured-closure handler pattern. |
| 6 | `tests/server_concurrency.rs` | New test case using the closure pattern. Same ratio assertion shape. |
| 7 | `CLAUDE.md` | Update Server Concurrency Model: contract upgraded -- handlers may capture and mutate state; mutations are per-request. Spawn/schedule still share closures (intended). |
| 8 | `CHANGELOG.md` | `[Unreleased] -> Fixed` entry. |

## Acceptance criteria

- [ ] `cargo test --lib` passes (1479 baseline + new tests).
- [ ] `cargo test --test server_concurrency` passes -- both existing global-fn ratio test and the new closure-pattern test.
- [ ] `fork_for_serving` cost benchmark < 1ms median.
- [ ] No Function::closure scope Arc shared with the template after a fork (Arc-ptr-equality assertion in tests).
- [ ] No Lambda::closure Arc shared with the template after a fork.
- [ ] Recursive function works post-fork (terminates, returns correct result).
- [ ] `spawn_task` regression test passes -- captured Lambda mutations still shared between parent and spawned task.
- [ ] Debug build catches a `Value::Stream` in template env at first fork (test verifies the panic).
- [ ] CHANGELOG and CLAUDE.md updated.
- [ ] Existing `examples/api.fg` and `examples/bench_server_concurrent.fg` still work.

## Commit breakdown

```
feat(interpreter): deep_clone_isolated walks closures with cycle handling
feat(interpreter): assert no streams in template env on fork (debug-only)
feat(server): wire fork_for_serving to deep_clone_isolated
test(interpreter): closure isolation, lambda isolation, recursive-fn, spawn parity
test(server): integration test for closure-handler isolation under load
docs: update Server Concurrency Model + CHANGELOG
```

## Closure-pattern fixture sketch

```forge
// examples/bench_server_closure.fg
@server(port: 9090)

// Outer scope captures a config object that handler closures use.
let config = { multiplier: 200 }

// Helper defined at top level but captures `config` -- non-trivial closure.
fn make_compute() {
    return fn(n) {
        let mut total = 0
        repeat n * config.multiplier times {
            total = total + 1
        }
        return total
    }
}

let compute = make_compute()

@get("/cpu")
fn cpu() -> Json {
    let result = compute(1000)
    return { ok: true, work: result }
}
```

Pre-fix: two concurrent requests calling `cpu()` enter `compute`,
which is a Lambda whose closure Arc is shared. The lambda body's
`let mut total` and `repeat` push/pop scopes on the shared closure
env, lock-contending and possibly racing on `total`.

Post-fix: each fork's `compute` is a separate Lambda with its own
closure Arc. No contention, no race. Predicted improvement: similar
shape to PR #108's improvement on the global-fn case (~3.5x).

## Risks (post-revision)

| Risk | Mitigation |
|---|---|
| Cycle handling has a subtle bug | Dedicated test, rust-expert review of placeholder pattern, manual trace through `Stmt::FnDef` cycle. |
| `spawn_task` users assume PR #108 broke their semantics | We're not changing spawn_task. Add a test that explicitly verifies spawn_task still shares closures. |
| `fork_for_background_runtime` users similarly | Same -- not changing it. |
| Debug-build stream assertion makes `forge run` slower | Walk runs once per fork, not per request. ~20us. Imperceptible. |
| Tests in the existing 1479 suite that rely on lambda mutation across "logical sessions" but use the HTTP server might break | The run is the verification; review failures case-by-case. |
| Lambda writeback at line 4317 has subtle semantics I'm misreading | The reviewer's trace confirms intra-fork accumulation works (closure Arc stable across calls within a fork) and inter-fork isolation works (different Arcs). Verify with the test. |
