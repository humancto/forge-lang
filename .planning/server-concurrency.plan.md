# Server Concurrency — Kill the Global Interpreter Mutex

## TL;DR

The Forge HTTP server (`src/runtime/server.rs`) wraps the entire interpreter
in `Arc<Mutex<Interpreter>>`. Every request acquires this mutex and holds it
for the full duration of the user handler. Result: the server is
**single-threaded by construction** regardless of CPU count.

**Empirical impact (16-core ARM64, ab benchmark):**

| Handler | Concurrency | Forge today | Node | Go |
|---|---:|---:|---:|---:|
| `/ping` (no-op JSON) | 16 | 45,375 req/sec | 47,904 | 41,947 |
| `/cpu` (~96ms work) | 16 | **9.84 req/sec** | 8,770 | 15,011 |
| `/cpu` (~96ms work) | 100 | **9.97 req/sec, p99 = 18.8s** | 10,517 | 18,016 |

**Goal:** kill the global mutex. Each request gets its own forked
interpreter, run on tokio's blocking pool with a wired cancellation token
and a backpressure semaphore.

> This plan was reviewed by `rust-expert` and revised. Showstoppers
> identified and addressed below in §"Review-driven changes".

## Scope (in)

1. Replace `Arc<Mutex<Interpreter>>` with `Arc<InterpreterTemplate>` in
   `src/runtime/server.rs`. New `InterpreterTemplate` type makes the
   "read-only template" invariant compile-time enforced.
2. New `pub fn fork_for_serving(&self) -> Self` on `Interpreter` that
   uses `env.deep_clone()` (NOT `env.clone()`) and resets per-request
   state correctly.
3. **Fix the existing `fork_for_background_runtime`** to also use
   `deep_clone` — it has the same latent shallow-clone bug; the bug just
   never manifested because schedule/watch never ran concurrently with
   itself. Same-PR fix because CLAUDE.md says "VM parity is your
   responsibility" and the same logic applies between fork primitives.
4. Wrap each HTTP handler body in `tokio::task::spawn_blocking` with:
   - A per-request `Arc<AtomicBool>` cancel token
   - A `Drop` guard on the future that signals cancellation when axum
     drops the response future (client disconnect)
   - A bounded backpressure semaphore (`tokio::sync::Semaphore`) sized to
     `max_blocking_threads` so we 503 fast instead of unboundedly queueing
   - Panic capture via `JoinError::is_panic()` → structured log line
5. Decide WS handler model: per-connection (not per-message) fork, with
   its own `tokio::sync::Mutex<Interpreter>` per WS connection. This is
   *type-coherent* because we use the same `AppState` and the WS handler
   internally creates its own per-connection state.
6. Add `Interpreter::deep_clone_into_template()` benchmark (criterion) so
   fork cost is measured and gated.
7. Add `tests/server_concurrency_test.rs` regression test:
   ratio-based assertion (wall time at C=16 < 1.5× wall time at C=1).
8. Promote `examples/bench_server_slow.fg` → `examples/bench_server_concurrent.fg`.
9. Document the behavior change loudly: handlers are pure functions of
   `(template, request) → response`. Top-level mutations from a handler
   no longer persist; cross-component reads (handler reading state a
   `schedule` writes) no longer see updates without a `shared {}` block.
   Future work: `shared {}` syntax (out of scope).

## Scope (out)

- Interpreter pool / VM-per-worker (Phase 1 Option 3). This PR is the unblock.
- Handler-controlled status codes / headers / response bodies.
- Middleware system.
- `wait N milliseconds` bug inside `@server` handlers.
- Server-on-VM (auto-fallback to interpreter stays).
- `shared { ... }` block syntax for cross-request state (next PR).

## Review-driven changes (what changed from v1 of this plan)

| # | rust-expert finding | Resolution in this plan |
|---|---|---|
| 1 | `Environment::clone()` is shallow over `Arc<Mutex<HashMap>>` — would re-introduce contention via per-scope locks | **Use `env.deep_clone()`**. Mandatory, not optional. Also fix `fork_for_background_runtime` in the same PR. |
| 2 | `Value::Function`/`Value::Lambda` closures share captured `Environment` even after `env.deep_clone()` | **Document hard contract: handlers must not mutate captured outer state.** Add a debug-build assertion that detects writes to a closure-captured `Arc` from a forked interpreter. Out of full scope to walk and rewrite all closures (cycle detection complexity); contract-and-test is the right shape. |
| 3 | `Send + Sync` risk paragraph was on the wrong axis (the actual problem is shallow-clone) | Drop the `tokio::sync::Mutex` fallback paragraph entirely. The decision is `deep_clone`, not lock-the-template. |
| 4 | `spawn_blocking` is not cancellable; client disconnect leaks the handler | Wire `Arc<AtomicBool>` per request. Use `tokio::select!` with a `Drop` guard so when axum drops the response future, the guard sets `cancelled.store(true, Release)`. The interpreter polls this at every statement boundary (existing infra at `src/interpreter/mod.rs:981`). |
| 5 | "Mutating top-level globals was a race" hand-wave missed the `schedule` ↔ handler case | Document that case explicitly in CHANGELOG and CLAUDE.md. Note future `shared {}` block as the proper answer. |
| 6 | "Few KB per fork" was untested | Add criterion benchmark of `fork_for_serving` cost. Acceptance criterion: <2ms median fork time. |
| 7 | "Use `tokio::sync::Mutex` template + clone-inside-spawn_blocking" is an anti-pattern | Removed. Template is `Arc<InterpreterTemplate>`, no lock. |
| 8 | Backpressure when blocking pool fills | `tokio::sync::Semaphore` with `permits = max_blocking_threads` (default 512). When exhausted, return 503 with `Retry-After` header. |
| 9 | Per-request memory: `Value::String` is `String`, not `Arc<str>` — large top-level state is a footgun | Verified: `Value::String(String)`. Add prominent warning in CLAUDE.md "Server Concurrency Model" section. Out of scope to convert to `Arc<str>` (broad change). |
| 10 | Graceful shutdown not addressed | Add `.with_graceful_shutdown(shutdown_signal())` with a 30s drain. In-flight `spawn_blocking` tasks signal cancel and join; after deadline, process exits. |
| 11 | Panic propagation lost into a generic 500 | `JoinError::is_panic()` extracts payload; log via `tracing::error!` (or `eprintln!` if tracing not yet wired). Return 500 with a generic body (don't leak panic message to client). |
| 12 | WS "exception" was type-incoherent | Resolved: `AppState = Arc<InterpreterTemplate>`. WS handler internally creates an `Arc<tokio::sync::Mutex<Interpreter>>` per *connection* (one fork per WS upgrade). Different from HTTP because WS holds session state across messages. |
| 13 | `fork_for_background_runtime` parity | **Fix in same PR.** Same `deep_clone` change. Documented in CLAUDE.md learnings. |
| 14 | Acceptance criterion was machine-dependent | Switched to ratio: "wall time at C=16 < 1.5× wall time at C=1." |
| 15 | Should use `parking_lot::Mutex` not `std::sync::Mutex` for WS | Adopt for WS-only. Don't introduce `parking_lot` more broadly in this PR. |
| 16 | Run new test under `cargo +nightly miri` / `loom` | Out of scope as a hard requirement; add to follow-up issue. The test is integration-level, not lock-internal. |

## Design

### Today (the murder weapon)

```rust
pub type AppState = Arc<Mutex<Interpreter>>;     // src/runtime/server.rs:21

let mut interp = match state.lock() {            // every handler
    Ok(g) => g,
    Err(poisoned) => poisoned.into_inner(),
};
let (status, json) = call_handler(&mut interp, &hn, &params, &query, None);
```

### Tomorrow

```rust
// src/runtime/server.rs

pub struct InterpreterTemplate {
    inner: Interpreter,                          // immutable after construction
}

pub struct AppState {
    template: Arc<InterpreterTemplate>,
    /// Bounded queue: 503 fast when blocking pool is saturated.
    /// Sized to match tokio's max_blocking_threads (default 512).
    permits: Arc<tokio::sync::Semaphore>,
}

impl InterpreterTemplate {
    pub fn fork(&self) -> Interpreter {
        self.inner.fork_for_serving()
    }
}

// Per-route handler shape:
async fn handle_get(
    State(state): State<AppState>,
    /* ... extractors ... */
) -> impl IntoResponse {
    // Backpressure: bounded permits, 503 if exhausted.
    let permit = match state.permits.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Retry-After", "1")],
            JsonResponse(json!({"error": "server at capacity"})),
        ).into_response(),
    };

    // Per-request cancellation token.
    let cancelled = Arc::new(AtomicBool::new(false));
    let _drop_guard = CancelOnDrop(cancelled.clone());

    let template = state.template.clone();
    let cancel_for_blocking = cancelled.clone();

    // Run synchronous Forge handler on the blocking pool, not on a tokio worker.
    let join = tokio::task::spawn_blocking(move || {
        let mut interp = template.fork();
        interp.cancelled = cancel_for_blocking;        // wire the cancel
        call_handler(&mut interp, &hn, &params, &query, body)
    });

    let (status, json) = match join.await {
        Ok(pair) => pair,
        Err(join_err) if join_err.is_panic() => {
            // Don't leak panic message to client. Log it.
            eprintln!("handler panic: {:?}", join_err.into_panic());
            (StatusCode::INTERNAL_SERVER_ERROR,
             json!({"error": "internal server error"}))
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR,
                   json!({"error": "handler join failed"})),
    };

    drop(permit);     // explicit; would happen on scope exit anyway
    (status, JsonResponse(json)).into_response()
}

// Drop guard signals cancellation when axum drops the response future
// (e.g. client disconnect or server shutdown).
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
```

### `fork_for_serving` — the corrected version

```rust
// src/interpreter/mod.rs

impl Interpreter {
    /// Fork this interpreter for serving a single HTTP request.
    /// The template stays read-only; the returned interpreter is owned
    /// by the request and dropped when the request completes.
    ///
    /// # Concurrency contract
    ///
    /// The forked interpreter has its own deep-cloned environment, so
    /// `env.define`/`env.set` from the request do not affect the template
    /// or any other in-flight request.
    ///
    /// **However**: `Value::Function`/`Value::Lambda` carry a `closure:
    /// Environment` field that is *shallow-cloned* by `deep_clone`. If a
    /// handler mutates state through a captured outer variable, behavior
    /// is racy across concurrent requests. Handlers must be pure
    /// functions of `(args) -> response`. See `CLAUDE.md` § Server
    /// Concurrency Model.
    pub fn fork_for_serving(&self) -> Self {
        let mut interp = Interpreter::new();
        // CRITICAL: deep_clone, not clone. clone() would share Arc<Mutex>
        // scopes and re-introduce contention.
        interp.env = self.env.deep_clone();
        interp.method_tables = self.method_tables.clone();
        interp.static_methods = self.static_methods.clone();
        interp.embedded_fields = self.embedded_fields.clone();
        interp.struct_defaults = self.struct_defaults.clone();
        interp.source = self.source.clone();
        interp.source_file = self.source_file.clone();
        // Per-request fresh state:
        interp.cancelled = Arc::new(AtomicBool::new(false));  // wired by caller
        interp.current_line = 0;
        interp.call_stack = Vec::new();
        interp.coverage = None;       // CLI-mode concern only
        interp.output_sink = None;    // serving has no shared output sink
        interp.debug_state = self.debug_state.clone(); // DAP can attach
        interp
    }
}
```

### `fork_for_background_runtime` — the same fix

```rust
// existing: self.env.clone()        ← BUG: shallow over Arc<Mutex>
// fix:      self.env.deep_clone()
```

Add a one-line `// FIX: was shallow .clone(), see PR #N` comment and a
note in the CLAUDE.md learnings section: *"`fork_for_background_runtime`
must `deep_clone` env. Shallow `.clone()` shares `Arc<Mutex>` scopes."*

### WS handler — per-connection fork

```rust
// On WS upgrade:
let template = state.template.clone();
ws.on_upgrade(move |mut socket| async move {
    // One fork per connection, held for the connection's lifetime.
    // Held inside parking_lot::Mutex because messages within a single
    // connection arrive serially; we just need !Send across `await`.
    let interp = Arc::new(parking_lot::Mutex::new(template.fork()));
    while let Some(Ok(msg)) = socket.recv().await {
        // ... existing logic, but interp is per-connection ...
    }
})
```

This is type-coherent: the *server* state is `Arc<InterpreterTemplate>`;
the WS handler creates its own per-connection `Arc<parking_lot::Mutex<Interpreter>>`.
Different concurrent WS connections each have their own forked interpreter.

## Tasks

| # | File | Change |
|---|---|---|
| 1 | `src/interpreter/mod.rs` | Add `pub fn fork_for_serving(&self) -> Self` (uses `deep_clone`). |
| 2 | `src/interpreter/mod.rs` | Fix `fork_for_background_runtime` to use `deep_clone`. Add learning comment. |
| 3 | `src/runtime/server.rs` | New `InterpreterTemplate` and `AppState` types. Drop `Arc<Mutex<Interpreter>>`. |
| 4 | `src/runtime/server.rs` | Refactor each HTTP handler to: acquire permit → fork → spawn_blocking → await with cancel-on-drop. |
| 5 | `src/runtime/server.rs` | WS handler: per-connection fork using `parking_lot::Mutex`. |
| 6 | `src/runtime/server.rs` | Add `with_graceful_shutdown` calling a SIGTERM/Ctrl-C signal helper. |
| 7 | `Cargo.toml` | Add `parking_lot = "0.12"` (already a tokio transitive; promote to direct dep for WS use). |
| 8 | `examples/bench_server_concurrent.fg` | Promote diagnostic file. Document the test pattern. |
| 9 | `tests/server_concurrency_test.rs` (new) | Ratio-based concurrency test (C=16 wall < 1.5× C=1 wall). |
| 10 | `benches/fork_for_serving.rs` (new) | Criterion bench gating fork cost. |
| 11 | `CLAUDE.md` | New `## Server Concurrency Model` section. Update Learnings: "fork primitives must `deep_clone` env, never plain `.clone()`". |
| 12 | `CHANGELOG.md` | `[Unreleased] → Fixed`: server now scales linearly with cores; behavior change for handlers reading scheduler-mutated state. |
| 13 | `ROADMAP.md` | Replace "28k req/sec" client claim with honest server numbers; add "fixed in vNext" line. |

## Acceptance criteria

- [ ] `cargo test --workspace` passes (today: 1,470 tests).
- [ ] New `tests/server_concurrency_test.rs` passes locally and in CI.
- [ ] Ratio assertion: 16 concurrent CPU-bound requests complete in < 1.5×
      single-request wall time.
- [ ] No `Arc<Mutex<Interpreter>>` in `src/runtime/server.rs` for HTTP
      handlers (WS uses per-connection `parking_lot::Mutex` — documented).
- [ ] `fork_for_serving` benchmark < 2ms median (criterion).
- [ ] Existing `examples/api.fg` and `examples/bench_server.fg` still work.
- [ ] `bench_server_concurrent.fg` + `ab -n 100 -c 16 …/cpu` shows
      throughput > 100 req/sec (target ~150).
- [ ] Backpressure: at C=10000 against 512 permits, fast 503s with
      `Retry-After` instead of unbounded queueing.
- [ ] Cancellation: `curl --max-time 0.1 …/slow` does NOT leave a
      runaway interpreter — the `cancelled` flag fires within ~10ms of
      drop, handler exits at next statement boundary.
- [ ] Graceful shutdown: SIGTERM drains for ≤30s then exits.
- [ ] CLAUDE.md, CHANGELOG.md, ROADMAP.md updated.

## Commit breakdown

```
fix(interpreter): deep_clone env in fork primitives (latent races)
feat(interpreter): add fork_for_serving for per-request handler isolation
feat(server): replace global mutex with per-request fork + spawn_blocking
feat(server): add backpressure semaphore + cancel-on-drop + graceful shutdown
test(server): add ratio-based concurrency regression test
bench(interpreter): add criterion benchmark for fork_for_serving cost
docs(server): document per-request fork model and behavior change
chore(roadmap): replace misleading client RPS claim with honest server numbers
```

## Risks remaining (post-revision)

| Risk | Mitigation |
|---|---|
| Handlers that mutate state via captured closures (the field-level escape hatch) silently race | Document the contract; consider a debug-build assertion in a follow-up PR. |
| `Value::String`-as-`String` makes large top-level data costly per fork | Document the footgun. `Arc<str>` conversion is a separate broad refactor. |
| `parking_lot` poisoning behavior differs from `std::sync::Mutex` | We only use it for WS where there's no panic-recovery story today either. Net-zero. |
| `tokio::task::spawn_blocking` panics still bring down the worker thread (it's OS-thread-bound) | Tokio replaces it; payload reaches us via `JoinError::is_panic()`. |
| The 1,470-test suite may have a test that depended on cross-request mutation | Run full suite before push; fix any breakages or document as expected behavior change. |

## What this PR is NOT promising

- "Forge HTTP server matches Node/Go for all workloads" — no. Interpreter
  is still 4–80× slower per-request than Python on CPU work. This PR
  unblocks parallelism; per-request speed is a separate problem (JIT
  regression, server-on-VM).
- "Production-ready service framework" — no. No middleware, no real
  observability, no real error responses. This PR just removes the
  artificial throughput ceiling that made any server benchmark dishonest.
