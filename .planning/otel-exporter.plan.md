# OpenTelemetry/OTLP Exporter (behind `otel` feature)

## TL;DR

PRs #108, #114, #118, #124 made the Forge HTTP server architecturally
sound and observable: parallel handlers, isolated closures, structured
logs via `tracing`, per-request `X-Request-Id`. Each request span carries
`method`, `uri`, `version`, `status`, `latency`, `request_id`, plus the
inner `forge.handler` span with `handler` and any user `log.info` events.

**This PR exports those spans to an OpenTelemetry collector** (Jaeger,
Tempo, Honeycomb, Datadog, OTel Collector, etc.) over OTLP/gRPC. Gated
behind a `otel` Cargo feature (default off) so the dep cost is opt-in.
Activated at runtime when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

After this PR, a Forge service can be plumbed into any vendor-neutral
observability stack with one env var:
```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
OTEL_SERVICE_NAME=my-forge-app \
forge run app.fg
```

> Reviewed by `rust-expert`: **REVISE -> all 5 showstoppers addressed
> below**. The original draft used `opentelemetry`/`opentelemetry_otlp`
> APIs from the 0.27 era that were removed before 0.31. Rewritten
> against the actual 0.31 builder API.

## Scope (in)

1. **New `otel` Cargo feature** (default off) that pulls in the 0.31
   ecosystem stack:
   - `opentelemetry = "0.31"` (trace feature)
   - `opentelemetry_sdk = "0.31"` (rt-tokio, trace features)
   - `opentelemetry-otlp = "0.31"` (grpc-tonic, trace features)
   - `tracing-opentelemetry = "0.32"`
2. **`tracing_init::init_subscriber`** gains an OTel layer when:
   - The `otel` feature is enabled at compile time, AND
   - `OTEL_EXPORTER_OTLP_ENDPOINT` is set at runtime.
3. **Standard OTel env vars honored** via `Resource::builder()` which
   already reads them per the OpenTelemetry spec:
   - `OTEL_EXPORTER_OTLP_ENDPOINT` -- where to send.
   - `OTEL_SERVICE_NAME` -- service identifier (default `"forge"`).
   - `OTEL_RESOURCE_ATTRIBUTES` -- comma-separated key=value resource
     attrs (parsed by `EnvResourceDetector`, not us).
4. **Owned `SdkTracerProvider` for shutdown.** Stored in a static
   `OnceLock<SdkTracerProvider>` next to the existing `INIT` lock,
   exposed via a `pub fn flush_otel()` helper that calls
   `provider.shutdown()`.
5. **Shutdown drain happens AFTER `axum::serve` returns**, not inside
   `shutdown_signal()`. Wrapped in `tokio::task::spawn_blocking` so
   the blocking `provider.shutdown()` call doesn't pin a worker.
6. **CLI shutdown drain** -- `main.rs` calls `flush_otel()` on
   normal exit (via a guard or explicit call after the entry point
   returns) so CLI scripts don't drop their last batch.
7. **Inbound W3C `traceparent` extraction.** ~15-line addition inside
   `TraceLayer::make_span_with` using the OTel global propagator. With
   this, Forge spans become children of upstream caller spans -- end-
   to-end distributed tracing works. Outbound `traceparent` injection
   in the HTTP client stays as a follow-up.
8. **Protocol selection.** Honor `OTEL_EXPORTER_OTLP_PROTOCOL` only
   for `grpc` (the default). Anything else: silently use grpc and
   document the limitation. (No warn -- silently picking the supported
   protocol is less hostile than a boot warning.)
9. **Documentation** in CLAUDE.md § Observability and CHANGELOG.
10. **CI: add `cargo build --features otel` to the workflow** so OTel
    breakage is caught immediately, not on the next contributor's PR.

## Scope (out)

- **Outbound `traceparent` injection in the HTTP client.** Separate PR.
- **Metrics export.** No metric instrumentation in Forge today; the
  `otel` feature shape leaves room for a future metrics dep.
- **Logs export via OTLP.** `opentelemetry-appender-tracing` is a
  separate story; logs already go to stderr via `fmt::layer()`.
- **HTTP/protobuf exporter.** gRPC only this PR; HTTP follow-up.
- **Sampling configuration.** Default exports everything. Documented
  with a CHANGELOG note: "configure your collector to sample, or
  see #N for `OTEL_TRACES_SAMPLER` follow-up."

## Review-driven changes (what changed from v1 of this plan)

| # | rust-expert finding | Resolution |
|---|---|---|
| 1 | **`opentelemetry::global::shutdown_tracer_provider()` does not exist in 0.31.** Must hold `SdkTracerProvider` and call `.shutdown()`. | New static `OnceLock<SdkTracerProvider>` next to `INIT`; new `flush_otel()` helper. |
| 2 | **`new_pipeline().tracing().with_exporter().install_batch(Tokio)` was deleted.** | Rewrote against builder API: `SpanExporter::builder().with_tonic().with_endpoint(...).build()` + `SdkTracerProvider::builder().with_batch_exporter(...).with_resource(...).build()`. |
| 3 | **`Config::default().with_resource(...)` is gone.** | Resource is set on the `SdkTracerProvider` builder directly. |
| 4 | **`Resource::new(vec![...])` is gone.** | Use `Resource::builder().with_attribute(KeyValue::new("service.name", "forge")).build()` -- the builder auto-includes `EnvResourceDetector` (handles `OTEL_RESOURCE_ATTRIBUTES`) and `SdkProvidedResourceDetector` (handles `OTEL_SERVICE_NAME` precedence per spec). Drops the manual env-var parsing entirely. |
| 5 | **`match` arms with `Option<OpenTelemetryLayer<S, T>>` don't unify** because the layer is generic over the tracer type. | Compute the option ONCE into a typed variable: `let otel: Option<OpenTelemetryLayer<_, opentelemetry_sdk::trace::SdkTracer>> = build_otel_layer(); registry.with(otel)`. Variable binding pins `T`; `tracing_subscriber::Layer for Option<L>` does the rest. |
| 6 | **`shutdown_signal()` is the wrong shutdown site** -- it delays request drain. | Call `flush_otel()` AFTER `axum::serve(...).await` returns, wrapped in `tokio::task::spawn_blocking`. |
| 7 | **CLI scripts drop their last batch on normal exit.** | `main.rs` calls `flush_otel()` after the entry point returns (via a guard or explicit call). |
| 8 | **Inbound `traceparent` extraction missing -- distributed traces don't connect.** | Added to scope: ~15 lines in `make_span_with` using `tracing-opentelemetry`'s `OpenTelemetrySpanExt::set_parent`. |
| 9 | **`OTEL_EXPORTER_OTLP_PROTOCOL` warn-on-non-grpc would fire on every server boot for users with stock OTel env.** | Silently use grpc, document the limitation. (HTTP follow-up issue.) |
| 10 | **Sampling default is "send everything"** -- floods collectors at high RPS. | Document loudly; defer `OTEL_TRACES_SAMPLER` to a follow-up. |
| 11 | **`init_subscriber` may be called from a nested tokio runtime** (stdlib helpers create their own), causing `SdkTracerProvider` to bind to a runtime that's torn down. | Move OTel init OUT of `init_subscriber`; call from `main.rs` and `start_server` only via a new `init_otel()` function. The plain `tracing` layer stays in `init_subscriber`. |
| 12 | **`opentelemetry-otlp::SpanExporter::builder().build()` blocks on lazy gRPC channel construction; tests need to verify "init doesn't panic with unreachable endpoint."** | Test uses `http://127.0.0.1:1` (no listener); asserts `init_otel` returns without panic and a subsequent `tracing::info!()` doesn't panic. |
| 13 | **CI `--features otel` not blocking** -- pinned versions rot the moment a new release lands. | Add to ci.yml as a required job (separate `cargo build --features otel` step). |

## Design

### Cargo feature

```toml
# Cargo.toml — diff (only the otel feature is added)
[features]
default = ["jit", "postgres", "mysql"]   # unchanged
# ...
otel = [
    "dep:opentelemetry",
    "dep:opentelemetry_sdk",
    "dep:opentelemetry-otlp",
    "dep:tracing-opentelemetry",
]

[dependencies]
# ... existing deps ...
opentelemetry = { version = "0.31", features = ["trace"], optional = true }
opentelemetry_sdk = { version = "0.31", features = ["rt-tokio", "trace"], optional = true }
opentelemetry-otlp = { version = "0.31", features = ["grpc-tonic", "trace"], optional = true }
tracing-opentelemetry = { version = "0.32", optional = true }
```

### `init_otel` (new, separate from `init_subscriber`)

```rust
// src/runtime/tracing_init.rs

#[cfg(feature = "otel")]
use opentelemetry_sdk::trace::SdkTracerProvider;

#[cfg(feature = "otel")]
static OTEL_PROVIDER: std::sync::OnceLock<SdkTracerProvider> = std::sync::OnceLock::new();

/// Install the OpenTelemetry tracing layer if `OTEL_EXPORTER_OTLP_ENDPOINT`
/// is set. MUST be called from the main tokio runtime (not a nested
/// runtime created by a stdlib helper). Idempotent via OnceLock.
///
/// On success, a global `SdkTracerProvider` is owned by `OTEL_PROVIDER`
/// and can be flushed via [`flush_otel`] on graceful shutdown.
///
/// No-op when:
///   - The `otel` feature is not enabled at compile time, OR
///   - `OTEL_EXPORTER_OTLP_ENDPOINT` is not set, OR
///   - `OTEL_EXPORTER_OTLP_PROTOCOL` is set to anything other than `grpc`
///     (we silently use grpc; HTTP/protobuf is a follow-up).
#[cfg(feature = "otel")]
pub fn init_otel() {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::prelude::*;

    let endpoint = match std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        Ok(e) => e,
        Err(_) => return,  // OTel not requested; silent no-op
    };

    OTEL_PROVIDER.get_or_init(|| {
        let exporter = match SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
        {
            Ok(e) => e,
            Err(err) => {
                tracing::error!(
                    target: "forge.server",
                    error = %err,
                    "OTLP exporter init failed; OpenTelemetry export disabled"
                );
                return SdkTracerProvider::builder().build();  // dummy provider, won't export
            }
        };

        // Resource: builder auto-includes EnvResourceDetector (parses
        // OTEL_RESOURCE_ATTRIBUTES per spec) and SdkProvidedResourceDetector
        // (handles OTEL_SERVICE_NAME precedence). We add a fallback
        // service.name in case the user didn't set OTEL_SERVICE_NAME.
        let resource = Resource::builder()
            .with_attribute(KeyValue::new("service.name", "forge"))
            .build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();

        let tracer = provider.tracer("forge");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        // Add the OTel layer to the existing subscriber. The plain
        // tracing setup is already installed by init_subscriber; we
        // ADD a new layer dynamically. This requires the subscriber
        // to be re-initialized... actually no, tracing-subscriber
        // doesn't support adding layers post-init. So init_otel must
        // be called BEFORE init_subscriber, and init_subscriber must
        // know about it.

        // Set the global provider for the OpenTelemetry API surface.
        opentelemetry::global::set_tracer_provider(provider.clone());

        // Set up W3C TraceContext propagator so we can extract
        // traceparent from inbound requests.
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        provider
    });
}

#[cfg(not(feature = "otel"))]
pub fn init_otel() {}

/// Flush pending OpenTelemetry spans. No-op if OTel was never
/// initialized. Blocks; safe to call from `tokio::task::spawn_blocking`
/// on graceful shutdown.
#[cfg(feature = "otel")]
pub fn flush_otel() {
    if let Some(provider) = OTEL_PROVIDER.get() {
        if let Err(err) = provider.shutdown() {
            tracing::warn!(
                target: "forge.server",
                error = %err,
                "OTel provider shutdown failed; some spans may be lost"
            );
        }
    }
}

#[cfg(not(feature = "otel"))]
pub fn flush_otel() {}
```

### `init_subscriber` integration

```rust
pub fn init_subscriber() {
    INIT.get_or_init(|| {
        let filter = build_filter();
        let ansi = std::io::stderr().is_terminal();

        // OTel layer: built once, then attached as a typed Option.
        // The Option<Layer<S>> impl in tracing-subscriber handles the
        // None case as a no-op layer.
        #[cfg(feature = "otel")]
        let otel_layer: Option<
            tracing_opentelemetry::OpenTelemetryLayer<
                _,
                opentelemetry_sdk::trace::SdkTracer,
            >,
        > = OTEL_PROVIDER.get().map(|provider| {
            use opentelemetry::trace::TracerProvider;
            tracing_opentelemetry::layer().with_tracer(provider.tracer("forge"))
        });
        #[cfg(not(feature = "otel"))]
        let otel_layer: Option<()> = None;

        let registry = tracing_subscriber::registry()
            .with(filter)
            .with(otel_layer);   // Option<Layer> attaches conditionally

        // ... existing format dispatch (Json/Compact/Pretty) ...
    });
}
```

### Inbound `traceparent` extraction in `TraceLayer::make_span_with`

```rust
// src/runtime/server.rs - inside the existing make_span_with closure
.make_span_with(|req: &http::Request<_>| {
    // ... existing request_id extraction ...

    let span = tracing::info_span!(
        "request",
        method = %req.method(),
        uri = %req.uri(),
        version = ?req.version(),
        request_id = request_id,
    );

    // Extract upstream traceparent (if present) and attach as the
    // span's parent context. Without this, every Forge span is a
    // root span -- distributed traces don't connect across services.
    #[cfg(feature = "otel")]
    {
        use opentelemetry::propagation::Extractor;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        struct HeaderMapExtractor<'a>(&'a http::HeaderMap);
        impl<'a> Extractor for HeaderMapExtractor<'a> {
            fn get(&self, key: &str) -> Option<&str> {
                self.0.get(key).and_then(|v| v.to_str().ok())
            }
            fn keys(&self) -> Vec<&str> {
                self.0.keys().map(|k| k.as_str()).collect()
            }
        }

        let parent_cx = opentelemetry::global::get_text_map_propagator(
            |propagator| propagator.extract(&HeaderMapExtractor(req.headers())),
        );
        span.set_parent(parent_cx);
    }

    span
})
```

### Shutdown call sites

```rust
// src/runtime/server.rs - start_server
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|e| RuntimeError::new(&format!("server error: {}", e)))?;

// AFTER axum has finished draining: flush OTel.
// spawn_blocking so we don't pin a worker on the synchronous shutdown.
tokio::task::spawn_blocking(crate::runtime::tracing_init::flush_otel)
    .await
    .ok();

Ok(())
```

```rust
// src/main.rs - at the end of main, after CLI work completes
// Wrapped because main is #[tokio::main] async; flush_otel is sync.
tokio::task::spawn_blocking(forge_lang::runtime::tracing_init::flush_otel)
    .await
    .ok();
```

### Why init_otel is separate from init_subscriber

The reviewer flagged that `init_subscriber` is called from
`stdlib/log::call`, which can be invoked from inside a stdlib helper
function that has constructed its own nested `tokio::runtime::Runtime`.
If OTel init fired in that path, the `SdkTracerProvider` would bind to
the nested runtime's reactor and become a dangling reference when the
nested runtime is torn down.

`init_otel` therefore lives separately and is called ONLY from:
- `start_server` (top of the function, before any handler runs).
- `main.rs` entry point (so CLI scripts get OTel too).

Both paths run on the main `#[tokio::main]` runtime. `init_subscriber`
keeps doing its plain-tracing work and is safe to call from anywhere.

## Tasks

| # | File | Change |
|---|---|---|
| 1 | `Cargo.toml` | Add `otel` feature + 4 optional deps. |
| 2 | `src/runtime/tracing_init.rs` | Add `init_otel()`, `flush_otel()`, `OTEL_PROVIDER` static. Update `init_subscriber()` to consult `OTEL_PROVIDER` and attach the OTel layer if present. |
| 3 | `src/runtime/server.rs` | Call `tracing_init::init_otel()` at top of `start_server`. After `axum::serve(...).await` returns, `tokio::task::spawn_blocking(flush_otel).await.ok()`. |
| 4 | `src/runtime/server.rs` | In `make_span_with`, extract upstream `traceparent` and `set_parent` on the span (gated `cfg(feature = "otel")`). |
| 5 | `src/main.rs` | After CLI work completes, `tokio::task::spawn_blocking(flush_otel).await.ok()`. |
| 6 | `src/runtime/tracing_init.rs` tests | New gated unit test: set `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:1`, call `init_otel()`, assert no panic, emit a `tracing::info!()`, assert no panic. |
| 7 | `examples/otel-quickstart.md` (new) | Show how to point at a local OTel Collector + Jaeger UI via Docker Compose. |
| 8 | `CLAUDE.md` § Observability | New "OpenTelemetry export" subsection: feature flag, env vars, shutdown drain, distributed tracing prerequisites, sampling default. |
| 9 | `CHANGELOG.md` | `[Unreleased] -> Added` entry calling out the feature flag, the env-var contract, the no-sampling default, and the inbound `traceparent` support. |
| 10 | `.github/workflows/ci.yml` | Add `cargo build --features otel` step (blocking). |

## Acceptance criteria

- [ ] `cargo build` (no features) succeeds, no new dep cost.
- [ ] `cargo build --features otel` succeeds.
- [ ] `cargo test --lib` passes (1487 baseline).
- [ ] `cargo test --lib --features otel` passes; new gated test confirms init doesn't panic with unreachable endpoint.
- [ ] `cargo test --test server_concurrency` passes (no behavior change for non-OTel paths).
- [ ] With `--features otel` and no env var: server starts normally, no OTLP attempts, no warnings.
- [ ] With `--features otel` and `OTEL_EXPORTER_OTLP_ENDPOINT` set: spans get exported (verified via local Jaeger smoke test).
- [ ] Inbound request with `traceparent: 00-<trace_id>-<span_id>-01` results in a span whose parent is the upstream context (verified via the collector showing connected traces).
- [ ] Graceful shutdown flushes pending spans (Ctrl-C immediately after a request → request span shows up in collector).
- [ ] CLI script (`forge run script.fg` with `log.info(...)`) flushes spans on normal exit.
- [ ] `cargo fmt --check` clean.
- [ ] CHANGELOG and CLAUDE.md updated.

## Commit breakdown

```
feat(deps): add otel Cargo feature with opentelemetry 0.31 stack
feat(runtime): add init_otel + flush_otel with OnceLock-owned provider
feat(server): wire OTel layer + traceparent extraction + shutdown flush
feat(cli): flush OTel on main exit so CLI scripts don't drop spans
test(runtime): otel init handles unreachable endpoint gracefully
ci: add cargo build --features otel to required jobs
docs: document OpenTelemetry export feature, env vars, distributed tracing
```

## Risks (post-revision)

| Risk | Mitigation |
|---|---|
| OTel crate version churn breaks the build over time | Pinned to 0.31 stack; CI `--features otel` build step catches breakage immediately. CHANGELOG calls out the pin so upgrades are deliberate. |
| Default-off feature means users have to remember to enable | Documented in CLAUDE.md. The `OTEL_EXPORTER_OTLP_ENDPOINT` env-var gate is a soft hint -- if set without the feature, a warn would fire... but only if init_subscriber knew about the env var, which it doesn't when feature is off. Acceptable: docs do the work. |
| Unbounded sampling floods collectors at high RPS | Documented loudly in CHANGELOG and CLAUDE.md: "all spans exported by default; configure your collector to sample, or wait for `OTEL_TRACES_SAMPLER` follow-up." |
| Inbound `traceparent` extraction parses untrusted headers | The OTel propagator is well-tested; malformed `traceparent` produces an empty context (no panic). Acceptable. |
| Graceful shutdown takes 5s extra for OTel batch flush | Documented; axum default drain is 30s so well within budget. |
| Provider double-init via `init_otel` from both `start_server` and `main.rs` | OnceLock guarantees single init; second call is no-op. |
| `set_text_map_propagator` is a global — interferes with embedder code | Embedders that need a different propagator install it after Forge boot; Forge's set is a sensible default for vanilla deployments. Documented. |
| nested-runtime concern from review | `init_otel` only callable from main runtime paths (`start_server`, `main.rs`); `init_subscriber` (which can be called from anywhere) does NOT touch OTel. |
