# Tracing Integration -- Replace eprintln with Structured Logs

## TL;DR

The Forge HTTP server's only observability is `eprintln!` with ANSI
color escape codes. Two `eprintln!` sites (panic + shutdown), nine
`println!` lines for the startup banner, and the user-facing `log`
stdlib module also `eprintln!`. None of this is consumable by a log
aggregator.

This PR introduces `tracing` + `tracing-subscriber`, migrates every
server-side log site, propagates span context across the
`spawn_blocking` boundary, and ships honest per-request HTTP spans.
Forge's `log` stdlib module continues to print human-friendly colored
output to TTY *and* additionally emits a `tracing` event so structured
sinks see it -- mirrors the banner-on-TTY decision for consistency.

> Reviewed by `rust-expert`: **REVISE -> addressed below in `Review-driven
> changes`**. Showstoppers: `TraceLayer`'s defaults are DEBUG (would be
> filtered out), `spawn_blocking` does not inherit span context (user
> `log.info` would have no HTTP fields), `info_span!().enter()` across
> `.await` is the canonical footgun. Missing decisions: `RUST_LOG`
> fallback, `with_ansi` gate on the fmt layer, user-visible color
> regression in interactive `log.info`.

## Scope (in)

1. **Add `tracing` and `tracing-subscriber` dependencies.** Features:
   `env-filter`, `json`, `fmt` (explicit even though default), and
   pin `tracing-subscriber >= 0.3.18` for stable JSON output.
2. **Single init point: `runtime::tracing_init::init_for_server()`.**
   Idempotent via `OnceLock` + `try_init`. Format detection from
   `FORGE_LOG_FORMAT`; filter from `FORGE_LOG` then `RUST_LOG` then
   default; ANSI gated on `IsTerminal::is_terminal(&stderr)`.
3. **Migrate the 2 server `eprintln!` sites and the startup banner.**
   Banner stays on TTY; an unconditional structured `info!` always
   fires too so non-interactive runs and TTY runs both produce a
   parseable startup record.
4. **Forge `log` stdlib emits `tracing` events AND keeps colored TTY
   output.** Same dual-output pattern as the banner. The `tracing`
   event uses `target: "forge.user"` (not the Rust module path) so
   users can filter their own output cleanly via
   `FORGE_LOG=forge.user=warn`.
5. **`tower_http::trace::TraceLayer` wired with INFO-level span and
   response events** so the default `forge_lang=info,...` filter
   actually sees per-request output. Custom configuration:
   `make_span_with(DefaultMakeSpan::new().level(Level::INFO))` and
   `on_response(DefaultOnResponse::new().level(Level::INFO))`.
6. **Per-request handler span via `#[tracing::instrument]`** on
   `run_handler`. Field `handler = %handler_name`. NOT
   `info_span!().enter()` -- that pattern is broken across `.await`
   and the reviewer flagged it as a showstopper.
7. **Span propagation across `spawn_blocking`.** Capture
   `Span::current()` on the async side, re-enter via `_g =
   span.enter()` on the blocking thread before calling
   `call_handler`. Without this, user `log.info` from inside a handler
   would have no HTTP request context.
8. **`debug!` event when `CancelOnDrop` fires** (client disconnect /
   shutdown), so debug-level runs see the cancel signal.
9. **Documentation:** new "Observability" subsection in CLAUDE.md
   covering the env vars, formats, dual-output behavior, the
   propagation pattern, and stable target names.

## Scope (out)

- `src/main.rs` and CLI error paths -- those `eprintln!`s are user-facing
  messages, not log records.
- Full `tracing` adoption across interpreter / VM / JIT internals.
- OpenTelemetry / OTLP exporter (separate follow-up; this PR is the
  precondition).
- Prometheus `/metrics` endpoint (separate follow-up).
- W3C traceparent header propagation in the HTTP client (separate
  follow-up).
- Forge syntax for named-field log events (`log.info("user_login",
  user_id: 42)`) -- needs parser changes; this PR ships only the
  plumbing.
- `tracing-panic` crate integration to turn arbitrary `panic!` into
  tracing events (out of scope; explicit follow-up).
- Migrating example files. They keep `say`/`println` for demo clarity.

## Review-driven changes (what changed from v1 of this plan)

| # | rust-expert finding | Resolution |
|---|---|---|
| 1 | **`TraceLayer` defaults are DEBUG**; with proposed filter `tower_http=info`, span+response events are filtered out. Acceptance criterion "panic shows up with method/uri/status" was unachievable. | **Configure** `TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().level(Level::INFO)).on_response(DefaultOnResponse::new().level(Level::INFO))`. Defaults match the filter. |
| 2 | **`spawn_blocking` doesn't propagate `tracing` span context.** User `log.info` events from inside a handler would have no HTTP request fields. | **Capture `Span::current()` before `spawn_blocking`; `_g = span.enter()` inside the blocking closure** before `call_handler`. Canonical pattern. |
| 3 | **`info_span!().enter()` across `.await` is broken.** The plan literally wrote that. | **Use `#[tracing::instrument(skip(state, body), fields(handler = %handler_name))]` on `run_handler` instead.** Idiomatic, async-correct, and gives us the field for free. |
| 4 | `log.info` would lose the colored TTY output that interactive users see today. | **Dual-output pattern, same as the banner.** TTY: keep the colored `eprintln!`. Always: emit a `tracing::info!(target: "forge.user", message = %message)`. Two outputs on TTY (color + structured); one (structured) when piped. |
| 5 | `log.info`'s tracing event would carry `target = "forge_lang::stdlib::log"` -- meaningless to Forge users. | **Hardcode `target: "forge.user"`.** Stable, semantic, filterable. |
| 6 | `RUST_LOG` interaction undefined. Most Rust developers reach for it first. | **Filter resolution: `FORGE_LOG` -> `RUST_LOG` -> default**. One extra `or_else` line. |
| 7 | `tracing-subscriber`'s `fmt` layer emits ANSI when piped unless explicitly disabled. The plan's whole motivation includes "ANSI codes leak into log files" -- would have re-introduced the same bug. | **`fmt::layer().with_ansi(IsTerminal::is_terminal(&std::io::stderr()))`.** ANSI only when stderr is a terminal. |
| 8 | Banner-vs-structured: plan picked conditional. Reviewer pushed back: emit structured event ALWAYS, keep banner only on TTY. Two outputs on TTY is fine. | **Adopted.** `tracing::info!(host, port, routes, cors, max_inflight, "Forge server listening")` always; banner additionally on TTY. |
| 9 | `cancel-on-drop` `debug!` mentioned in design summary but not in migration table. | **Added** to the migration table; spec is `tracing::debug!(handler = %handler_name, "client disconnected; cancel signaled")` inside the (debug-only) `Drop` impl. |
| 10 | Server-side `tracing::error!` for panics uses Rust target by default. | **Hardcode `target: "forge.server"`** for symmetry with `forge.user`. |
| 11 | Acceptance criterion missing "no ANSI in JSON output." | **Added.** |
| 12 | `tracing-panic` integration not addressed. | **Explicit follow-up.** Out of scope; ticket-able. |

## Design

### Initialization

```rust
// src/runtime/tracing_init.rs (new)

use std::io::IsTerminal;
use std::sync::OnceLock;
use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy)]
enum Format { Pretty, Compact, Json }

fn detect_format() -> Format {
    match std::env::var("FORGE_LOG_FORMAT").ok().as_deref() {
        Some("json") => Format::Json,
        Some("compact") => Format::Compact,
        Some("pretty") => Format::Pretty,
        _ => {
            if std::io::stderr().is_terminal() {
                Format::Pretty
            } else {
                Format::Compact
            }
        }
    }
}

fn build_filter() -> EnvFilter {
    EnvFilter::try_from_env("FORGE_LOG")
        .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
        .unwrap_or_else(|_| {
            EnvFilter::new("forge_lang=info,tower_http=info,axum=warn")
        })
}

/// Initialize the global tracing subscriber for server use.
///
/// Idempotent and panic-safe: calling twice is a no-op (`OnceLock`); if
/// a subscriber is already installed (test harness, embedder), `try_init`
/// returns Err and we silently move on.
///
/// Reads:
///   * FORGE_LOG_FORMAT = json | pretty | compact
///       - default: pretty when stderr is a TTY, compact otherwise.
///   * FORGE_LOG       = env-filter directive
///       - falls back to RUST_LOG, then to forge_lang=info,tower_http=info,axum=warn.
///
/// ANSI escapes are emitted only when stderr is a TTY -- never when piped.
pub fn init_for_server() {
    INIT.get_or_init(|| {
        let filter = build_filter();
        let ansi = std::io::stderr().is_terminal();
        match detect_format() {
            Format::Json => {
                let _ = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(std::io::stderr))
                    .try_init();
            }
            Format::Compact => {
                let _ = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().compact()
                        .with_ansi(ansi)
                        .with_writer(std::io::stderr))
                    .try_init();
            }
            Format::Pretty => {
                let _ = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().pretty()
                        .with_ansi(ansi)
                        .with_writer(std::io::stderr))
                    .try_init();
            }
        }
    });
}
```

### Per-request span via `#[instrument]` + propagation across `spawn_blocking`

```rust
// src/runtime/server.rs

#[tracing::instrument(
    skip(state, path_params, query_params, body),
    fields(handler = %handler_name)
)]
async fn run_handler(
    state: AppState,
    handler_name: String,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
    body: Option<JsonValue>,
) -> Response {
    // ... permit acquisition, cancel guard, fork ...

    // CRITICAL: propagate the span across the spawn_blocking boundary.
    // Without this, tracing events emitted by user log.info calls (which
    // run on the blocking thread) would have no HTTP request context --
    // no method, no uri, no handler field.
    let span = tracing::Span::current();

    let join = tokio::task::spawn_blocking(move || {
        // Re-enter the captured span on the blocking thread. The guard
        // is dropped at the end of the closure, ending the span scope.
        let _g = span.enter();
        let mut interp = template.fork();
        interp.cancelled = cancel_for_blocking;
        call_handler(&mut interp, &handler_name, &path_params, &query_params, body)
    });

    // ... await + JoinError handling ...
}
```

### TraceLayer with INFO defaults

```rust
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

let trace_layer = TraceLayer::new_for_http()
    .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
    .on_response(DefaultOnResponse::new().level(Level::INFO));

let app = app
    .layer(cors_layer)
    .layer(trace_layer)
    .with_state(state);
```

Now `forge_lang=info,tower_http=info` actually sees the per-request
span and the response event. `on_failure` defaults to ERROR level, so
500s are loud regardless of filter.

### Migration table (final)

| Site | Before | After |
|---|---|---|
| `server.rs:230` panic | `eprintln!("\x1B[31m[server panic]\x1B[0m handler panicked: {}", msg)` | `tracing::error!(target: "forge.server", handler = %handler_name, panic = %msg, "handler panicked")` |
| `server.rs:426` shutdown | `eprintln!("\x1B[33m[server] shutdown signal received, draining...\x1B[0m")` | `tracing::info!(target: "forge.server", "shutdown signal received, draining")` |
| `server.rs:373-387` startup banner | 9× `println!` with ANSI | (a) `tracing::info!(target: "forge.server", host = %config.host, port = config.port, routes = routes.len(), max_inflight = DEFAULT_MAX_INFLIGHT, "Forge server listening")` -- always. (b) On TTY only: keep the original 9-line banner. |
| `CancelOnDrop::drop` (new) | -- | `tracing::debug!(target: "forge.server", "client disconnected; cancel signaled")` |
| `stdlib/log.rs` 4 sites | `eprintln!("\x1B[..m[..]\x1B[0m  {}", message)` | (a) On TTY only: keep the original colored eprintln. (b) Always: `tracing::{info,warn,error,debug}!(target: "forge.user", message = %message)`. |

### Forge `log` stdlib: target = "forge.user"

The Forge `log` module is the user-facing logging surface. Events fire
with `target: "forge.user"` so a user can filter their own output
distinctly from runtime noise:

```bash
# All runtime logs warn-only, user logs at info:
FORGE_LOG=forge_lang=warn,tower_http=warn,forge.user=info forge run app.fg
```

This is stable, doesn't leak Rust module paths, and gives the user
control over their log volume independently from runtime volume.

### What the integration test must NOT break

`tests/server_concurrency.rs` boots the server in-process and asserts
a ratio between C=8 and C=1 wall times. After this PR:
- `init_for_server` is called via `start_server`; `OnceLock::get_or_init`
  fires once per test binary; `try_init` is a no-op on second call.
- Stderr will get tracing-formatted output, not the banner (test runs
  are non-TTY, so banner suppressed).
- The ratio assertion still holds.

Verify empirically before pushing.

## Tasks

| # | File | Change |
|---|---|---|
| 1 | `Cargo.toml` | Add `tracing = "0.1"` and `tracing-subscriber = { version = "0.3.18", features = ["env-filter", "json", "fmt"] }`. |
| 2 | `src/runtime/tracing_init.rs` (new) | `init_for_server()` per the design above. |
| 3 | `src/runtime/mod.rs` | `pub mod tracing_init;`. |
| 4 | `src/runtime/server.rs` | Call `tracing_init::init_for_server()` at top of `start_server`. |
| 5 | `src/runtime/server.rs` | Replace 2 eprintln + 9 println sites per the migration table. TTY-detect for the banner via `std::io::stdout().is_terminal()`. Always-emit the structured `info!`. |
| 6 | `src/runtime/server.rs` | `#[instrument]` on `run_handler`. Capture `Span::current()` before `spawn_blocking`; re-enter inside. |
| 7 | `src/runtime/server.rs` | Wire `TraceLayer` with INFO levels per the design. |
| 8 | `src/runtime/server.rs` | Add `tracing::debug!` in `CancelOnDrop::drop`. |
| 9 | `src/stdlib/log.rs` | Dual output: colored eprintln on TTY + always `tracing::{level}!(target: "forge.user", message = %message)`. |
| 10 | `tests/server_concurrency.rs` | Run + verify; tighten anything depending on the old eprintln output. |
| 11 | `examples/bench_server_concurrent.fg`, `bench_server_closure.fg`, `api.fg` | Smoke check; no source changes expected. |
| 12 | `CLAUDE.md` | New "Observability" subsection inside § Server Concurrency Model. Documents env vars, formats, dual-output behavior, target naming, propagation. |
| 13 | `CHANGELOG.md` | `[Unreleased] -> Added` entry. |

## Acceptance criteria

- [ ] `cargo test --lib` passes (1484 baseline; no new lib tests required).
- [ ] `cargo test --test server_concurrency` passes (ratio assertion intact).
- [ ] `forge run examples/api.fg` on a TTY: shows the colorful banner AND a structured `tracing` info event for the same startup.
- [ ] `forge run examples/api.fg 2>&1 | cat` (non-TTY): no ANSI codes anywhere; pretty/compact tracing only.
- [ ] `FORGE_LOG_FORMAT=json forge run examples/api.fg 2>&1 | head -5` emits valid JSON, no ANSI codes.
- [ ] `FORGE_LOG=forge_lang=debug` shows debug-level events.
- [ ] `RUST_LOG=forge.user=warn` (with no `FORGE_LOG`) is honored.
- [ ] A handler that panics: structured event with `level=ERROR`, `target=forge.server`, fields `handler` and `panic`, plus (via TraceLayer) `method`, `uri`, `status_code=500`.
- [ ] A user `log.info("hello")` from inside a handler: structured event with `target=forge.user`, field `message="hello"`, plus inherited `handler=<name>`, `method`, `uri` from the propagated span.
- [ ] `cargo fmt --check` clean.
- [ ] CLAUDE.md and CHANGELOG updated.

## Commit breakdown

```
feat(runtime): add tracing_init with FORGE_LOG / RUST_LOG fallback
feat(server): wire TraceLayer + #[instrument] + spawn_blocking span propagation
refactor(server): replace eprintln/println with tracing events; banner on TTY only
refactor(stdlib/log): dual TTY-color + tracing event with target "forge.user"
docs: document observability env-vars and dual-output behavior
```

## Risks remaining (post-revision)

| Risk | Mitigation |
|---|---|
| `try_init` Err is silent: a misconfigured subscriber goes unnoticed | OnceLock gates init; the JSON-output acceptance criterion verifies the subscriber is installed. |
| `Span::current()` captured-before-spawn pattern is subtle | Document inline; the integration test that calls `log.info` from a handler verifies it works. |
| Two outputs on TTY (banner + structured info, or color eprintln + tracing event) doubles output for interactive users | Acceptable: matches the banner-on-TTY decision the user already accepted; structured event is short. |
| Future `tracing-subscriber` major bump changes JSON layout | Pinned to `>= 0.3.18`; CHANGELOG entry calls out the format. |
| Default filter `tower_http=info` adds 2 events per request | That's the price for getting per-request observability. User can quiet via `FORGE_LOG=tower_http=warn`. |
| Test binary inherits a subscriber from whichever test ran first | Documented in the implementation: tests must not assert on log format details. |

## Why this is the right next item

PR #108 + PR #114 made the server architecturally sound. Without
`tracing` we cannot:
- Add a `/metrics` endpoint with per-handler counters.
- Emit OTel spans with traceparent propagation.
- Show structured per-request fields in any log aggregator.
- Add request-id middleware that correlates to user logs.

`tracing` is the substrate for all of those. The registry + layer
composition pattern this PR uses is what `opentelemetry-tracing` plugs
into. Doing it now keeps the production-grade roadmap unblocked.
