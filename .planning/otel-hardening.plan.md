# OTel Hardening Plan

Target issues: #130, #131, #133.

## Goal

Make the optional `otel` feature harder to rot and cheaper when compiled-but-disabled:

- CI builds the `otel` feature and runs the tracing-init feature tests.
- A feature-gated integration smoke test exercises `init_otel -> subscriber -> span -> flush_otel` with an unreachable-but-valid OTLP endpoint.
- HTTP request span creation skips W3C `traceparent` extraction unless OTel export was actually activated at runtime.

## Files

- `.github/workflows/ci.yml`
  - Add an `otel-build` job on `ubuntu-latest`.
  - Include `Swatinem/rust-cache@v2`, matching the rest of CI.
  - Run `cargo build --features otel --verbose`.
  - Run `cargo test --lib --features otel runtime::tracing_init`.
  - Run `cargo test --test otel_smoke --features otel`.
- `src/runtime/tracing_init.rs`
  - Add a small `pub fn otel_is_active() -> bool` helper so the integration test can assert the public runtime contract.
  - Under `#[cfg(feature = "otel")]`, back it with a static flag set only after exporter/provider construction succeeds.
  - Under `#[cfg(not(feature = "otel"))]`, return `false`.
  - Keep `flush_otel()` behavior unchanged.
- `src/runtime/server.rs`
  - Wrap the existing per-request OTel `traceparent` extraction block in `if tracing_init::otel_is_active()`.
  - Keep the `#[cfg(feature = "otel")]` around imports and OTel-only code.
- `tests/otel_smoke.rs`
  - New integration test file gated with `#![cfg(feature = "otel")]`.
  - Set `OTEL_EXPORTER_OTLP_ENDPOINT` to an unreachable local endpoint.
  - Call `init_otel()`, `init_subscriber()`, emit and drop a span, then call `flush_otel()` through a timeout so a future exporter/runtime hang fails deterministically.
- `CHANGELOG.md`
  - Add an `[Unreleased]` fixed/changed entry for the OTel CI and request-path gating.

## Approach

1. Introduce an OTel activation flag:

   ```rust
   #[cfg(feature = "otel")]
   static OTEL_ACTIVE: AtomicBool = AtomicBool::new(false);

   #[cfg(feature = "otel")]
   pub fn otel_is_active() -> bool {
       OTEL_ACTIVE.load(Ordering::Acquire)
   }
   ```

   Set it to `true` only after `SpanExporter::builder().with_tonic().with_endpoint(...).build()` succeeds and the real `SdkTracerProvider` is installed. Do not set it in the exporter-init failure fallback, because that fallback intentionally disables export.

2. Gate request-span parent extraction:

   ```rust
   #[cfg(feature = "otel")]
   if tracing_init::otel_is_active() {
       // existing HeaderMapExtractor + set_parent block
   }
   ```

   This avoids propagator access and `HeaderMapExtractor::keys()` allocation for users who compile with `--features otel` but do not set `OTEL_EXPORTER_OTLP_ENDPOINT`.

3. Add the CI job exactly as the issue requests, with the feature build before the lib test.

4. Add the integration smoke test:

   - Preserve and restore any existing `OTEL_EXPORTER_OTLP_ENDPOINT`.
   - Set `OTEL_EXPORTER_OTLP_TIMEOUT=1000` and `OTEL_BSP_EXPORT_TIMEOUT=2000` so unreachable endpoints do not turn into long CI stalls.
   - Use a localhost endpoint on a deliberately unbound port.
   - Use a multi-thread Tokio test.
   - Assert `tracing_init::otel_is_active()` is `true` after `init_otel()`.
   - Use `tokio::time::timeout` around `spawn_blocking(flush_otel)` to catch hangs.

## Tests

Run locally:

- `cargo fmt`
- `cargo build --features otel`
- `cargo test --lib --features otel runtime::tracing_init`
- `cargo test --test otel_smoke --features otel`
- `cargo test`

## Edge Cases

- `otel` feature disabled: `otel_is_active()` compiles to `false`; server code still compiles without OTel crates.
- `otel` feature enabled but endpoint unset: no per-request extraction overhead.
- Endpoint set but exporter construction fails: `OTEL_ACTIVE` remains false.
- Endpoint set and exporter construction succeeds but collector is unreachable: test must complete or fail quickly rather than hang.

## Rollback

Remove the CI job, smoke test, activation helper, and request-path gate. Existing OTel export behavior from PR #129 is restored.
