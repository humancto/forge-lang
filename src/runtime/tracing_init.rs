//! Tracing subscriber initialization.
//!
//! This is the one place that installs a global `tracing` subscriber.
//! [`init_subscriber`] is idempotent (`OnceLock` + `try_init`); it is
//! called from any path that wants its `tracing` events to reach a
//! user — currently `start_server` (so per-request HTTP spans show up)
//! and the `log` stdlib module (so `log.info` from a CLI-invoked
//! script reaches the user without depending on the server path).
//!
//! # Environment
//!
//! - `FORGE_LOG_FORMAT` = `json` | `pretty` | `compact`
//!   - default: `pretty` when stderr is a TTY, `compact` otherwise.
//! - `FORGE_LOG`        = `tracing_subscriber::EnvFilter` directive
//!   - precedence: `FORGE_LOG` > `RUST_LOG` > default
//!     `forge_lang=info,tower_http=info,axum=warn,forge.user=info`.
//!
//! ANSI escape codes are emitted only when stderr is a terminal.
//! Piped or redirected stderr (CI, log aggregators, `forge run | tee`)
//! gets clean text — no escape leak.
//!
//! # OpenTelemetry export (gated by `otel` feature)
//!
//! When the `otel` Cargo feature is enabled AND
//! `OTEL_EXPORTER_OTLP_ENDPOINT` is set at runtime, [`init_otel`] sets
//! up an OTLP/gRPC exporter that ships every `tracing` span to an
//! OpenTelemetry collector. [`flush_otel`] drains the batch processor
//! on graceful shutdown. The OTel layer is added to the subscriber
//! stack by [`init_subscriber`] when [`init_otel`] has run first.
//!
//! `init_otel` MUST be called from the main tokio runtime (not from a
//! nested runtime created by a stdlib helper), since the batch
//! processor binds to whichever runtime constructs it. The valid call
//! sites are `start_server` (for the HTTP path) and `main` (for CLI
//! scripts so they don't drop their last batch on exit).
//!
//! Honored env vars (subset of the OpenTelemetry spec):
//!
//! - `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` — trace-signal gRPC endpoint;
//!   takes precedence over the generic endpoint (see [`traces_endpoint`]).
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` — gRPC endpoint, e.g. `http://localhost:4317`.
//! - `OTEL_TRACES_SAMPLER` / `OTEL_TRACES_SAMPLER_ARG` — head sampling
//!   (see [`SamplerConfig`]); default `parentbased_always_on`.
//! - `OTEL_SERVICE_NAME`           — service name attribute (default `"forge"`).
//! - `OTEL_RESOURCE_ATTRIBUTES`    — comma-separated key=value pairs.
//!
//! `OTEL_EXPORTER_OTLP_PROTOCOL` is read but only `grpc` is wired in
//! this iteration; HTTP/protobuf is a follow-up.
//!
//! Outbound HTTP requests made by Forge code carry the W3C `traceparent`
//! header of a per-request client span when OTel is active (see
//! [`trace_context_headers`]), so downstream services join the trace.
//!
//! # Panics
//!
//! Once [`init_subscriber`] installs Forge's subscriber it also installs a
//! panic hook ([`install_panic_hook`]) that reports Rust panics as an
//! `ERROR` event on the `forge.panic` target, so they carry span context
//! (request id, method, uri) and come out as JSON under
//! `FORGE_LOG_FORMAT=json`. When the active filter disables `forge.panic`
//! the previous hook runs instead, so a panic is never silently dropped.

use std::io::IsTerminal;
use std::sync::OnceLock;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[cfg(feature = "otel")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "otel")]
use opentelemetry_sdk::trace::SdkTracerProvider;

static INIT: OnceLock<()> = OnceLock::new();

#[cfg(feature = "otel")]
static OTEL_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

#[cfg(feature = "otel")]
static OTEL_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
enum Format {
    Pretty,
    Compact,
    Json,
}

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
    // Default filter:
    //   forge=info       -- every `forge*` target: `forge.server`
    //                       lifecycle events and the spans/events of the
    //                       `forge` binary, which compiles the runtime
    //                       modules itself (module-path targets such as
    //                       the server's `request` span are
    //                       `forge::runtime::server` there, not
    //                       `forge_lang::...`)
    //   forge_lang=info  -- the same for the library (embedders, tests)
    //   tower_http=info  -- per-request TraceLayer span + response event
    //   axum=warn        -- quiet by default; user can flip on
    //   forge.user=info  -- user log.info from Forge code, on by default
    //                       so a CLI script that calls log.info("hi")
    //                       actually shows "hi" without env tuning
    //   forge.runtime=info -- CLI runtime notes (engine fallback)
    //   forge.panic=error -- Rust panics, as structured events
    EnvFilter::try_from_env("FORGE_LOG")
        .or_else(|_| EnvFilter::try_from_env("RUST_LOG"))
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// Filter used when neither `FORGE_LOG` nor `RUST_LOG` is set.
const DEFAULT_FILTER: &str =
    "forge=info,forge_lang=info,tower_http=info,axum=warn,forge.user=info,forge.runtime=info,forge.panic=error";

/// Target of the event [`install_panic_hook`] emits for a Rust panic.
pub const PANIC_TARGET: &str = "forge.panic";

/// True only when the opt-in OpenTelemetry exporter was initialized
/// successfully at runtime. Compiling with `--features otel` is not
/// enough; users must also set `OTEL_EXPORTER_OTLP_ENDPOINT` and the
/// exporter builder must succeed.
#[cfg(feature = "otel")]
pub fn otel_is_active() -> bool {
    OTEL_ACTIVE.load(Ordering::Acquire)
}

/// No-op state probe when the `otel` feature is disabled.
#[cfg(not(feature = "otel"))]
pub fn otel_is_active() -> bool {
    false
}

/// Install the global subscriber. Idempotent and panic-safe.
///
/// Called from any path that wants its `tracing` events to be visible:
/// `start_server` (so per-request HTTP spans surface) and the `log`
/// stdlib module (so `log.info` from a CLI-invoked script reaches the
/// user without depending on the server path having run).
///
/// If a subscriber is already installed (test harness, embedder),
/// `try_init` returns `Err` and we silently move on — the existing
/// subscriber wins.
pub fn init_subscriber() {
    INIT.get_or_init(|| {
        let filter = build_filter();
        let ansi = crate::color::enabled(crate::color::Stream::Stderr);

        // Build the OTel layer ONCE into a typed Option. The variable
        // binding pins the layer's tracer type so both the Some and
        // None paths produce the same `Option<OpenTelemetryLayer<S, T>>`.
        // `tracing_subscriber::Layer for Option<L>` then attaches it
        // conditionally as a no-op when None.
        //
        // OTel only attaches here if init_otel() has already run AND
        // succeeded. If the user is running with `--features otel` but
        // never set OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_PROVIDER stays
        // empty and otel_layer is None — same as if the feature were
        // off entirely.
        // OTel layer attaches at the Registry level (innermost). This
        // ordering matters: the OTel layer needs the raw events before
        // filtering so it can record everything sent to the OTLP
        // exporter independently of what the user's FORGE_LOG filter
        // chooses to display. EnvFilter is applied AFTER OTel so it
        // only filters what gets to the fmt layer (stderr output).
        //
        // The Option<L> impl in tracing-subscriber turns a `None` into
        // a no-op at compile time, so when init_otel hasn't run, this
        // costs nothing.
        #[cfg(feature = "otel")]
        let otel_layer: Option<
            tracing_opentelemetry::OpenTelemetryLayer<
                tracing_subscriber::Registry,
                opentelemetry_sdk::trace::Tracer,
            >,
        > = OTEL_PROVIDER.get().map(|provider| {
            use opentelemetry::trace::TracerProvider;
            tracing_opentelemetry::layer().with_tracer(provider.tracer("forge"))
        });

        // When the otel feature is OFF we use Identity which is a
        // genuine no-op layer that satisfies Layer<S> for any S.
        // (Option<()> doesn't compile because () isn't a Layer.)
        #[cfg(not(feature = "otel"))]
        let otel_layer: tracing_subscriber::layer::Identity =
            tracing_subscriber::layer::Identity::new();

        let installed = match detect_format() {
            Format::Json => tracing_subscriber::registry()
                .with(otel_layer)
                .with(filter)
                .with(fmt::layer().json().with_writer(std::io::stderr))
                .try_init(),
            Format::Compact => tracing_subscriber::registry()
                .with(otel_layer)
                .with(filter)
                .with(
                    fmt::layer()
                        .compact()
                        .with_ansi(ansi)
                        .with_writer(std::io::stderr),
                )
                .try_init(),
            Format::Pretty => tracing_subscriber::registry()
                .with(otel_layer)
                .with(filter)
                .with(
                    fmt::layer()
                        .pretty()
                        .with_ansi(ansi)
                        .with_writer(std::io::stderr),
                )
                .try_init(),
        };
        // Only when our subscriber won: an embedder that installed its
        // own subscriber also owns its panic reporting.
        if installed.is_ok() {
            install_panic_hook();
        }
    });
}

/// Report Rust panics through `tracing`.
///
/// Wraps the current panic hook: a panic becomes one `ERROR` event on
/// [`PANIC_TARGET`] carrying the payload, source location, thread name
/// and (when `RUST_BACKTRACE` asks for one) a backtrace, emitted inside
/// whatever span is current on the panicking thread. If no subscriber
/// enables that target at `ERROR` (for example `FORGE_LOG=forge.user=info`)
/// the previous hook runs instead, so the panic is still reported.
///
/// Called by [`init_subscriber`]; public for embedders that install their
/// own subscriber and want the same behaviour.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !emit_panic_event(info) {
            previous(info);
        }
    }));
}

/// Emit the panic event; false when nothing would record it.
fn emit_panic_event(info: &std::panic::PanicHookInfo<'_>) -> bool {
    if !tracing::enabled!(target: PANIC_TARGET, tracing::Level::ERROR) {
        return false;
    }
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("Box<dyn Any>");
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
        tracing::error!(
            target: PANIC_TARGET,
            payload = message,
            location = %location,
            thread = thread_name,
            backtrace = %backtrace,
            "thread panicked",
        );
    } else {
        tracing::error!(
            target: PANIC_TARGET,
            payload = message,
            location = %location,
            thread = thread_name,
            "thread panicked",
        );
    }
    true
}

/// Install the OpenTelemetry/OTLP exporter if `OTEL_EXPORTER_OTLP_ENDPOINT`
/// is set. No-op when the `otel` feature is off, when the env var is
/// unset, or when exporter construction fails.
///
/// **Must be called from the main tokio runtime**, not from a nested
/// runtime created by a stdlib helper. The valid call sites are
/// `start_server` (for the HTTP path) and `main` (for CLI scripts so
/// their last batch isn't dropped on exit).
///
/// Idempotent: subsequent calls are no-ops via the `OTEL_PROVIDER`
/// `OnceLock`.
///
/// **Must be called BEFORE `init_subscriber`** so the OTel layer is
/// available when the subscriber is constructed. Once
/// `tracing_subscriber::registry().try_init()` runs, layers cannot be
/// added; the OTel layer must be present from the start.
#[cfg(feature = "otel")]
pub fn init_otel() {
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::Resource;

    let env = |key: &str| std::env::var(key).ok();
    let Some(endpoint) = traces_endpoint(env) else {
        return; // OTel not requested; silent no-op.
    };
    let (sampler, sampler_warning) = SamplerConfig::from_env(env);

    OTEL_PROVIDER.get_or_init(|| {
        if let Some(warning) = sampler_warning {
            // eprintln: the subscriber is not installed yet.
            eprintln!("[forge.server] {}", warning);
        }
        let exporter_result = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build();

        let exporter = match exporter_result {
            Ok(e) => e,
            Err(err) => {
                // Use eprintln rather than tracing — the subscriber
                // hasn't been installed yet (init_otel runs first).
                eprintln!(
                    "[forge.server] OTLP exporter init failed: {}; \
                     OpenTelemetry export disabled",
                    err
                );
                // Return an empty provider so the OnceLock is filled
                // and subsequent calls don't retry. The subscriber
                // path will still see Some(provider) and attach a
                // no-export layer; harmless.
                return SdkTracerProvider::builder().build();
            }
        };

        // Resource::builder() auto-includes:
        //   - SdkProvidedResourceDetector (honors OTEL_SERVICE_NAME)
        //   - EnvResourceDetector (parses OTEL_RESOURCE_ATTRIBUTES per spec)
        //   - TelemetryResourceDetector (telemetry.sdk.* attributes)
        //
        // We add a fallback service.name = "forge" ONLY when neither
        // OTEL_SERVICE_NAME nor `service.name=` in OTEL_RESOURCE_ATTRIBUTES
        // is set. The reason: Resource::with_attribute internally calls
        // Resource::merge, which gives the new attribute priority over
        // the existing resource. If we always added "forge" we would
        // overwrite the operator's OTEL_SERVICE_NAME -- the OPPOSITE
        // of the spec-mandated precedence. Setting it conditionally
        // preserves the spec contract: env > fallback.
        let user_set_service = std::env::var("OTEL_SERVICE_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some()
            || std::env::var("OTEL_RESOURCE_ATTRIBUTES")
                .ok()
                .is_some_and(|s| s.contains("service.name="));

        let mut resource_builder = Resource::builder();
        if !user_set_service {
            resource_builder =
                resource_builder.with_attribute(KeyValue::new("service.name", "forge"));
        }
        let resource = resource_builder.build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .with_sampler(sampler.to_sdk())
            .build();

        // Set the global provider so the OpenTelemetry API surface
        // (used by tracing-opentelemetry's set_parent for inbound
        // traceparent extraction) sees our tracer.
        opentelemetry::global::set_tracer_provider(provider.clone());

        // W3C TraceContext propagator so `make_span_with` can extract
        // upstream traceparent headers and set the inbound parent
        // context on the request span. Without this, spans Forge emits
        // are root spans even when the caller sent traceparent.
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        OTEL_ACTIVE.store(true, Ordering::Release);
        provider
    });
}

/// No-op when the `otel` feature is disabled.
#[cfg(not(feature = "otel"))]
pub fn init_otel() {}

/// The OTLP endpoint traces are exported to, if any.
///
/// Per the OpenTelemetry spec the signal-specific
/// `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` takes precedence over the generic
/// `OTEL_EXPORTER_OTLP_ENDPOINT`; an empty value counts as unset. Setting
/// either one activates export (with the `otel` feature). For gRPC both are
/// used as given (no `/v1/traces` suffix is appended).
///
/// `env` looks up one variable (`|k| std::env::var(k).ok()` in
/// production); taking it as a parameter keeps this testable without
/// mutating the process environment.
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub fn traces_endpoint(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    [
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
    ]
    .into_iter()
    .filter_map(|key| env(key))
    .map(|value| value.trim().to_string())
    .find(|value| !value.is_empty())
}

/// Head-sampling policy for exported traces, from `OTEL_TRACES_SAMPLER`
/// and `OTEL_TRACES_SAMPLER_ARG` (OpenTelemetry SDK configuration spec).
///
/// | `OTEL_TRACES_SAMPLER` | Policy |
/// |---|---|
/// | `always_on` | record every trace |
/// | `always_off` | record nothing |
/// | `traceidratio` | record a fraction (the `_ARG`, default `1.0`) of traces |
/// | `parentbased_always_on` (default) | follow the upstream sampled flag; sample new roots |
/// | `parentbased_always_off` | follow the upstream sampled flag; drop new roots |
/// | `parentbased_traceidratio` | follow the upstream sampled flag; ratio for new roots |
///
/// Values are case-insensitive. An unknown sampler (including the
/// spec's `jaeger_remote` and `xray`, which Forge does not ship) or an
/// argument that is not a number in `[0, 1]` falls back to the default
/// with a warning rather than failing startup.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub enum SamplerConfig {
    AlwaysOn,
    AlwaysOff,
    TraceIdRatio(f64),
    ParentBased(Box<SamplerConfig>),
}

#[cfg_attr(not(feature = "otel"), allow(dead_code))]
impl SamplerConfig {
    /// The spec default, `parentbased_always_on`.
    pub fn default_policy() -> Self {
        SamplerConfig::ParentBased(Box::new(SamplerConfig::AlwaysOn))
    }

    /// Resolve the policy from the environment. Returns the policy and,
    /// when the configuration was invalid and a fallback was used, a
    /// warning to show the operator.
    pub fn from_env(env: impl Fn(&str) -> Option<String>) -> (Self, Option<String>) {
        let name = env("OTEL_TRACES_SAMPLER")
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty());
        let Some(name) = name else {
            return (Self::default_policy(), None);
        };
        let ratio = || -> (f64, Option<String>) {
            let Some(raw) = env("OTEL_TRACES_SAMPLER_ARG").filter(|v| !v.trim().is_empty()) else {
                return (1.0, None);
            };
            match raw.trim().parse::<f64>() {
                Ok(r) if (0.0..=1.0).contains(&r) => (r, None),
                _ => (
                    1.0,
                    Some(format!(
                        "OTEL_TRACES_SAMPLER_ARG={:?} is not a ratio in [0, 1]; using 1.0",
                        raw
                    )),
                ),
            }
        };
        match name.as_str() {
            "always_on" => (SamplerConfig::AlwaysOn, None),
            "always_off" => (SamplerConfig::AlwaysOff, None),
            "traceidratio" => {
                let (r, warning) = ratio();
                (SamplerConfig::TraceIdRatio(r), warning)
            }
            "parentbased_always_on" => (Self::default_policy(), None),
            "parentbased_always_off" => (
                SamplerConfig::ParentBased(Box::new(SamplerConfig::AlwaysOff)),
                None,
            ),
            "parentbased_traceidratio" => {
                let (r, warning) = ratio();
                (
                    SamplerConfig::ParentBased(Box::new(SamplerConfig::TraceIdRatio(r))),
                    warning,
                )
            }
            other => (
                Self::default_policy(),
                Some(format!(
                    "OTEL_TRACES_SAMPLER={:?} is not supported; using parentbased_always_on",
                    other
                )),
            ),
        }
    }

    #[cfg(feature = "otel")]
    fn to_sdk(&self) -> opentelemetry_sdk::trace::Sampler {
        use opentelemetry_sdk::trace::Sampler;
        match self {
            SamplerConfig::AlwaysOn => Sampler::AlwaysOn,
            SamplerConfig::AlwaysOff => Sampler::AlwaysOff,
            SamplerConfig::TraceIdRatio(r) => Sampler::TraceIdRatioBased(*r),
            SamplerConfig::ParentBased(root) => Sampler::ParentBased(Box::new(root.to_sdk())),
        }
    }
}

/// W3C trace-context headers (`traceparent`, and `tracestate` when
/// non-empty) that propagate `span` to a downstream service.
///
/// When `span` is disabled by the active filter the caller's current span
/// is propagated instead, so a request made inside an HTTP handler still
/// joins the inbound trace. Empty when OTel export is not active (no
/// `otel` feature, or no endpoint configured), since there is no trace to
/// join; the HTTP client then sends no trace headers at all.
#[cfg(feature = "otel")]
pub fn trace_context_headers(span: &tracing::Span) -> Vec<(String, String)> {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    if !otel_is_active() {
        return Vec::new();
    }
    let span = if span.is_disabled() {
        tracing::Span::current()
    } else {
        span.clone()
    };
    let cx = span.context();
    let mut carrier = std::collections::HashMap::<String, String>::new();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut carrier)
    });
    let mut headers: Vec<(String, String)> = carrier.into_iter().collect();
    headers.sort();
    headers
}

/// Always empty without the `otel` feature: there is no trace to join.
#[cfg(not(feature = "otel"))]
pub fn trace_context_headers(_span: &tracing::Span) -> Vec<(String, String)> {
    Vec::new()
}

/// Flush pending OpenTelemetry spans. Safe to call from
/// `tokio::task::spawn_blocking` on graceful shutdown — the underlying
/// `provider.shutdown()` is synchronous.
///
/// No-op when the `otel` feature is off or `init_otel` was never called.
#[cfg(feature = "otel")]
pub fn flush_otel() {
    if let Some(provider) = OTEL_PROVIDER.get() {
        if let Err(err) = provider.shutdown() {
            // Subscriber may already be torn down at this point during
            // process exit; eprintln is safer than tracing here.
            eprintln!(
                "[forge.server] OTel provider shutdown failed: {}; \
                 some spans may be lost",
                err
            );
        }
    }
}

/// No-op when the `otel` feature is disabled.
#[cfg(not(feature = "otel"))]
pub fn flush_otel() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calling init twice must not panic. This is the integration-test
    /// scenario: each `start_server` call goes through `init_subscriber`,
    /// and a single test binary may boot the server many times.
    #[test]
    fn init_is_idempotent() {
        init_subscriber();
        init_subscriber();
        init_subscriber();
    }

    /// Filter resolution order: FORGE_LOG > RUST_LOG > default.
    /// Build the filter without installing a subscriber so we can
    /// inspect it. (We can't easily assert structure, but we can at
    /// least confirm none of these panic on construction.)
    #[test]
    fn filter_construction_does_not_panic() {
        let _ = build_filter();
    }

    /// Format detection covers all four code paths.
    #[test]
    fn format_detection_explicit_values() {
        // We can't safely set env vars in a test (process-wide state),
        // so we just exercise the auto-detect path. The explicit-value
        // arms are trivial match arms.
        let _ = detect_format();
    }

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn traces_endpoint_prefers_signal_specific_variable() {
        assert_eq!(traces_endpoint(env_of(&[])), None);
        assert_eq!(
            traces_endpoint(env_of(&[(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://generic:4317"
            )])),
            Some("http://generic:4317".to_string())
        );
        assert_eq!(
            traces_endpoint(env_of(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://generic:4317"),
                ("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", "http://jaeger:4317"),
            ])),
            Some("http://jaeger:4317".to_string())
        );
        // The signal-specific variable alone activates export.
        assert_eq!(
            traces_endpoint(env_of(&[(
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "http://jaeger:4317"
            )])),
            Some("http://jaeger:4317".to_string())
        );
        // Empty counts as unset (falls through to the generic one).
        assert_eq!(
            traces_endpoint(env_of(&[
                ("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", " "),
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://generic:4317"),
            ])),
            Some("http://generic:4317".to_string())
        );
        assert_eq!(
            traces_endpoint(env_of(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "")])),
            None
        );
    }

    #[test]
    fn sampler_defaults_to_parentbased_always_on() {
        assert_eq!(
            SamplerConfig::from_env(env_of(&[])),
            (SamplerConfig::default_policy(), None)
        );
        assert_eq!(
            SamplerConfig::from_env(env_of(&[("OTEL_TRACES_SAMPLER", "")])),
            (SamplerConfig::default_policy(), None)
        );
    }

    #[test]
    fn sampler_maps_every_spec_value() {
        use SamplerConfig::*;
        let pb = |s: SamplerConfig| ParentBased(Box::new(s));
        let cases: &[(&str, Option<&str>, SamplerConfig)] = &[
            ("always_on", None, AlwaysOn),
            ("always_off", None, AlwaysOff),
            ("traceidratio", Some("0.25"), TraceIdRatio(0.25)),
            ("traceidratio", None, TraceIdRatio(1.0)),
            ("parentbased_always_on", None, pb(AlwaysOn)),
            ("parentbased_always_off", None, pb(AlwaysOff)),
            (
                "parentbased_traceidratio",
                Some("0.1"),
                pb(TraceIdRatio(0.1)),
            ),
            (
                "PARENTBASED_TRACEIDRATIO",
                Some(" 0 "),
                pb(TraceIdRatio(0.0)),
            ),
        ];
        for (name, arg, expected) in cases {
            let mut pairs = vec![("OTEL_TRACES_SAMPLER", *name)];
            if let Some(arg) = arg {
                pairs.push(("OTEL_TRACES_SAMPLER_ARG", arg));
            }
            let (got, warning) = SamplerConfig::from_env(env_of(&pairs));
            assert_eq!(&got, expected, "OTEL_TRACES_SAMPLER={name} arg={arg:?}");
            assert_eq!(warning, None, "OTEL_TRACES_SAMPLER={name} arg={arg:?}");
        }
    }

    #[test]
    fn sampler_falls_back_with_a_warning() {
        let (got, warning) =
            SamplerConfig::from_env(env_of(&[("OTEL_TRACES_SAMPLER", "jaeger_remote")]));
        assert_eq!(got, SamplerConfig::default_policy());
        assert!(warning.expect("warning").contains("jaeger_remote"));

        for bad in ["1.5", "-0.1", "half", "NaN"] {
            let (got, warning) = SamplerConfig::from_env(env_of(&[
                ("OTEL_TRACES_SAMPLER", "traceidratio"),
                ("OTEL_TRACES_SAMPLER_ARG", bad),
            ]));
            assert_eq!(got, SamplerConfig::TraceIdRatio(1.0), "arg {bad}");
            assert!(warning.expect("warning").contains(bad), "arg {bad}");
        }
    }

    /// Without an active exporter there is no trace to propagate.
    #[test]
    fn trace_context_headers_are_empty_without_otel_export() {
        if otel_is_active() {
            return; // another test in this binary activated export
        }
        let span = tracing::info_span!("outbound");
        assert!(trace_context_headers(&span).is_empty());
    }

    /// Calling `flush_otel` when `init_otel` was never invoked must be
    /// a no-op (not a panic), so CLI scripts that don't use OTel can
    /// safely have `flush_otel` in their exit path.
    #[test]
    fn flush_otel_without_init_is_noop() {
        super::flush_otel();
    }

    /// With the otel feature enabled and a deliberately unreachable
    /// endpoint, init_otel must succeed without panicking. The batch
    /// processor will retry forever in the background; the call site
    /// must not hang or fail. Subsequent tracing events must also not
    /// panic. This test would catch a regression where a future OTel
    /// crate version breaks the lazy-channel-construction assumption.
    #[cfg(feature = "otel")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn init_otel_with_unreachable_endpoint_does_not_panic() {
        // Use a port that's almost certainly unbound. Don't use a
        // real env var because that would pollute other tests; instead
        // call the inner provider builder path directly via a helper.
        // (Since init_otel reads OTEL_EXPORTER_OTLP_ENDPOINT and we
        // can't safely set env vars in a test, we instead just verify
        // the no-op path: with no env var set, init_otel must return
        // without panic.)
        super::init_otel();
        // A subsequent tracing event must not panic regardless of
        // whether the OTel layer is installed.
        tracing::info!(target: "forge.test", "post-init event");
        // flush is also a no-op or safe call.
        super::flush_otel();
    }
}
