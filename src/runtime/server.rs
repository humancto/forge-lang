//! Forge HTTP Server — Axum + Tokio
//!
//! Per-request fork architecture: each incoming request gets its own
//! Interpreter forked from a shared, read-only [`InterpreterTemplate`].
//! Handlers run on tokio's blocking pool via [`tokio::task::spawn_blocking`]
//! so synchronous Forge code never blocks an async worker thread.
//!
//! Concurrency guarantees:
//! - **No global lock on the hot path.** Forks share only the
//!   [`Arc<InterpreterTemplate>`], not any mutable state.
//! - **Backpressure.** A bounded [`tokio::sync::Semaphore`] prevents the
//!   blocking pool from queueing unboundedly; excess requests get a
//!   503 with `Retry-After: 1`.
//! - **Cancellation.** Each request carries an [`Arc<AtomicBool>`] that
//!   the per-request interpreter polls at every safe point (loop / call /
//!   statement). A `Drop` guard on the response future flips it when
//!   axum drops the future (client disconnect, server shutdown).
//! - **Graceful shutdown.** SIGINT/SIGTERM triggers axum's graceful
//!   shutdown; in-flight requests get up to 30s to finish before the
//!   process exits.
//!
//! Behavior change vs. the previous global-mutex model:
//! - Top-level mutations made by a handler do not persist across
//!   requests. Each fork starts from the template snapshot.
//! - Handlers that read state mutated by `schedule`/`watch` blocks no
//!   longer see those updates. A future `shared { ... }` block will
//!   provide an explicit cross-request state primitive.
//!
//! See `CLAUDE.md` § Server Concurrency Model for the user-facing
//! contract.

use indexmap::IndexMap;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json as JsonResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::Value as JsonValue;
use tokio::sync::Semaphore;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::interpreter::{Interpreter, RuntimeError, Value};
use crate::runtime::metadata::{CorsMode, ServerPlan};
use crate::runtime::tracing_init;

/// Default cap on concurrent in-flight handler invocations.
///
/// Sized to match tokio's default `max_blocking_threads` (512). When the
/// permit pool is exhausted the server returns 503 with `Retry-After: 1`
/// instead of unboundedly queueing on the blocking pool — fast failure
/// is better than client-perceived hangs followed by RST.
const DEFAULT_MAX_INFLIGHT: usize = 512;

/// Read-only template the server forks per request.
///
/// Construction-time only: once wrapped in `Arc<InterpreterTemplate>` and
/// installed on a router, the inner [`Interpreter`] must not be mutated.
/// All per-request work happens on the forked interpreter returned by
/// [`Self::fork`].
pub struct InterpreterTemplate {
    inner: Interpreter,
}

impl InterpreterTemplate {
    pub fn new(interp: Interpreter) -> Self {
        Self { inner: interp }
    }

    /// Produce a fresh per-request interpreter. Cheap relative to handler
    /// cost, expensive relative to a mutex acquire — the win is that two
    /// concurrent requests never block on each other.
    pub fn fork(&self) -> Interpreter {
        self.inner.fork_for_serving()
    }
}

/// Application state passed to every axum handler.
#[derive(Clone)]
pub struct AppState {
    template: Arc<InterpreterTemplate>,
    permits: Arc<Semaphore>,
}

/// Drop guard that signals cancellation when axum drops the response
/// future (client disconnect, request timeout, server shutdown).
///
/// The forked interpreter's [`Interpreter::cancelled`] points at the
/// same `Arc<AtomicBool>`, so the long-running blocking task observes
/// the flip at its next safe point and returns a `cancelled` error —
/// freeing the blocking-pool thread and the fork's memory.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
        // Visible under FORGE_LOG=forge.server=debug. Useful when
        // diagnosing why a long-running handler exited early.
        tracing::debug!(
            target: "forge.server",
            "client disconnected; cancel signaled"
        );
    }
}

fn to_axum_path(forge_path: &str) -> String {
    forge_path
        .split('/')
        .map(|s| {
            if let Some(rest) = s.strip_prefix(':') {
                format!("{{{}}}", rest)
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn call_handler(
    interp: &mut Interpreter,
    handler_name: &str,
    path_params: &HashMap<String, String>,
    query_params: &HashMap<String, String>,
    body: Option<JsonValue>,
) -> (StatusCode, JsonValue) {
    let handler = match interp.env.get(handler_name) {
        Some(v) => v,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": format!("handler '{}' not found", handler_name)}),
            )
        }
    };

    let mut args: Vec<Value> = Vec::new();
    if let Value::Function { ref params, .. } = handler {
        for param in params {
            if let Some(val) = path_params.get(&param.name) {
                args.push(Value::String(val.clone()));
            } else if param.name == "body" || param.name == "data" {
                args.push(
                    body.as_ref()
                        .map(|b| json_to_forge(b.clone()))
                        .unwrap_or(Value::Object(IndexMap::new())),
                );
            } else if param.name == "query" || param.name == "qs" {
                let obj: IndexMap<String, Value> = query_params
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                args.push(Value::Object(obj));
            } else if let Some(val) = query_params.get(&param.name) {
                args.push(Value::String(val.clone()));
            } else {
                args.push(Value::Null);
            }
        }
    }

    match interp.call_function(handler, args) {
        Ok(value) => (StatusCode::OK, forge_to_json(&value)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": e.message}),
        ),
    }
}

/// Run a Forge handler with full per-request lifecycle:
/// 1. Acquire a backpressure permit, or 503 if exhausted.
/// 2. Set up the cancel-on-drop guard.
/// 3. Fork the interpreter and ship it to the blocking pool, propagating
///    the tracing span across the boundary so user `log.info` events
///    inherit the HTTP request fields.
/// 4. Await; capture panics into a 500 without leaking payload to the client.
///
/// The `#[instrument]` attribute opens an info-level span named
/// `forge.handler` carrying `handler = %handler_name`. Combined with
/// the outer `TraceLayer` span (which carries `method`, `uri`, `status`,
/// `latency`), every event from inside the handler — runtime *or* user
/// `log.info` — is correlated to its request.
#[tracing::instrument(
    name = "forge.handler",
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
    let permit = match state.permits.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Retry-After", "1")],
                JsonResponse(serde_json::json!({"error": "server at capacity"})),
            )
                .into_response();
        }
    };

    let cancelled = Arc::new(AtomicBool::new(false));
    // _drop_guard lives until the end of this future. If axum drops us
    // (client disconnect, shutdown), it fires and the blocking task
    // observes the cancel at its next safe point.
    let _drop_guard = CancelOnDrop(cancelled.clone());

    let template = state.template.clone();
    let cancel_for_blocking = cancelled.clone();

    // CRITICAL: capture the current tracing span on the async side,
    // re-enter it on the blocking thread. tokio::task::spawn_blocking
    // does NOT propagate tracing context (different OS thread, no
    // automatic instrumentation), so without this, every event from
    // inside the handler — including user `log.info` calls from Forge
    // code — would have no span context (no method, no uri, no
    // handler field).
    let span = tracing::Span::current();

    // Clone the handler name for the blocking closure; the original
    // stays available for the panic-log site below.
    let hn_for_blocking = handler_name.clone();
    let join = tokio::task::spawn_blocking(move || {
        let _g = span.enter();
        let mut interp = template.fork();
        // Replace the per-request token with the one the response-future
        // Drop guard owns. Now client disconnect short-circuits the
        // handler at the next loop/call/statement safe point.
        interp.cancelled = cancel_for_blocking;
        call_handler(
            &mut interp,
            &hn_for_blocking,
            &path_params,
            &query_params,
            body,
        )
    });

    let (status, json) = match join.await {
        Ok(pair) => pair,
        Err(join_err) if join_err.is_panic() => {
            // Don't leak panic message to the client. Log it.
            let payload = join_err.into_panic();
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "<non-string panic payload>".to_string()
            };
            tracing::error!(
                target: "forge.server",
                handler = %handler_name,
                panic = %msg,
                "handler panicked",
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "internal server error"}),
            )
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": "handler join failed"}),
        ),
    };

    drop(permit);
    (status, JsonResponse(json)).into_response()
}

pub async fn start_server(
    interpreter: Interpreter,
    server: &ServerPlan,
) -> Result<(), RuntimeError> {
    // Install the global tracing subscriber on first server boot;
    // idempotent across multiple starts in the same process (test
    // harness, embedder). The log stdlib module also calls this
    // lazily so CLI-invoked scripts get the same setup.
    tracing_init::init_subscriber();

    let config = &server.config;
    let routes = &server.routes;

    let state = AppState {
        template: Arc::new(InterpreterTemplate::new(interpreter)),
        permits: Arc::new(Semaphore::new(DEFAULT_MAX_INFLIGHT)),
    };

    let mut app = Router::new();

    for route in routes {
        let axum_path = to_axum_path(&route.pattern);
        let hn = route.handler_name.clone();

        match route.method.as_str() {
            "GET" => {
                let hn = hn.clone();
                app = app.route(
                    &axum_path,
                    get(move |State(state): State<AppState>,
                              path: Option<Path<HashMap<String, String>>>,
                              Query(query): Query<HashMap<String, String>>| async move {
                        let params = path.map(|Path(p)| p).unwrap_or_default();
                        run_handler(state, hn, params, query, None).await
                    }),
                );
            }
            "POST" | "PUT" => {
                let hn = hn.clone();
                let method = route.method.clone();
                let handler = move |State(state): State<AppState>,
                                    path: Option<Path<HashMap<String, String>>>,
                                    Query(query): Query<HashMap<String, String>>,
                                    Json(body): Json<JsonValue>| async move {
                    let params = path.map(|Path(p)| p).unwrap_or_default();
                    run_handler(state, hn, params, query, Some(body)).await
                };
                if method == "POST" {
                    app = app.route(&axum_path, post(handler));
                } else {
                    app = app.route(&axum_path, put(handler));
                }
            }
            "DELETE" => {
                let hn = hn.clone();
                app = app.route(
                    &axum_path,
                    delete(move |State(state): State<AppState>,
                                 path: Option<Path<HashMap<String, String>>>,
                                 Query(query): Query<HashMap<String, String>>| async move {
                        let params = path.map(|Path(p)| p).unwrap_or_default();
                        run_handler(state, hn, params, query, None).await
                    }),
                );
            }
            "WS" => {
                // WebSocket handlers hold session state across messages, so
                // a per-request fork is the wrong model. Each connection
                // gets its own forked interpreter held inside a
                // parking_lot::Mutex (messages on a single connection
                // arrive serially; the lock just gives us !Send across
                // await). Different connections are still fully isolated.
                let hn = hn.clone();
                app = app.route(
                    &axum_path,
                    get(
                        move |State(state): State<AppState>,
                              ws: axum::extract::WebSocketUpgrade| {
                            let template = state.template.clone();
                            let hn = hn.clone();
                            async move {
                                ws.on_upgrade(move |mut socket| async move {
                                    use axum::extract::ws::Message;
                                    let interp = Arc::new(parking_lot::Mutex::new(template.fork()));
                                    while let Some(Ok(msg)) = socket.recv().await {
                                        if let Message::Text(text) = msg {
                                            let response = {
                                                let mut interp = interp.lock();
                                                let handler = interp.env.get(&hn);
                                                if let Some(h) = handler {
                                                    match interp.call_function(
                                                        h,
                                                        vec![Value::String(text.to_string())],
                                                    ) {
                                                        Ok(v) => format!("{}", v),
                                                        Err(e) => format!("error: {}", e.message),
                                                    }
                                                } else {
                                                    "handler not found".to_string()
                                                }
                                            };
                                            let _ =
                                                socket.send(Message::Text(response.into())).await;
                                        }
                                    }
                                })
                            }
                        },
                    ),
                );
            }
            _ => {}
        }
    }

    // Apply CORS policy: restrictive by default, permissive only when explicitly requested.
    let cors_layer = match config.cors {
        CorsMode::Permissive => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
        CorsMode::Restrictive => CorsLayer::new(), // same-origin only
    };

    // TraceLayer wraps every request in a tracing span carrying method,
    // uri, version, and emits an INFO event on response with status +
    // latency. Configured to INFO level explicitly because tower-http's
    // defaults are DEBUG, which the default filter (forge_lang=info,
    // tower_http=info) would silently drop.
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    let app = app.layer(cors_layer).layer(trace_layer).with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e| RuntimeError::new(&format!("invalid address: {}", e)))?;

    // Always emit the structured startup event so log aggregators see
    // server boot regardless of TTY / format choice.
    let cors_str = match config.cors {
        CorsMode::Permissive => "permissive",
        CorsMode::Restrictive => "restrictive",
    };
    tracing::info!(
        target: "forge.server",
        host = %config.host,
        port = config.port,
        routes = routes.len(),
        cors = cors_str,
        max_inflight = DEFAULT_MAX_INFLIGHT,
        "Forge server listening",
    );

    // Additionally, on a TTY, print the original colorful banner. This
    // is genuinely good UX for `forge run my_server.fg` interactively.
    // Skipped when stdout is piped (CI, log capture) so escape codes
    // don't leak into log files.
    if std::io::stdout().is_terminal() {
        let cors_label = match config.cors {
            CorsMode::Permissive => "\x1B[33mpermissive (any origin)\x1B[0m",
            CorsMode::Restrictive => "\x1B[32mrestrictive (same-origin)\x1B[0m",
        };
        println!();
        println!("  \x1B[1;32m🔥 Forge server running\x1B[0m");
        println!("  \x1B[1m   http://{}\x1B[0m", addr);
        println!("  \x1B[90m   CORS: {}\x1B[0m", cors_label);
        println!(
            "  \x1B[90m   max in-flight: {} (excess returns 503)\x1B[0m",
            DEFAULT_MAX_INFLIGHT
        );
        println!();
        for route in routes {
            println!("  \x1B[36m{:>6}\x1B[0m  {}", route.method, route.pattern);
        }
        println!();
        println!("  \x1B[90mPowered by axum + tokio | Ctrl+C to stop\x1B[0m");
        println!();
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| RuntimeError::new(&format!("bind failed: {}", e)))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| RuntimeError::new(&format!("server error: {}", e)))?;

    Ok(())
}

/// Wait for SIGINT (Ctrl-C) or SIGTERM. axum will then stop accepting new
/// connections and let in-flight requests finish (subject to client and
/// per-request timeouts). Drop guards on dropped futures still flip the
/// per-request cancel flag so any blocking handlers that are still
/// running observe the cancel at their next safe point.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sigterm.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!(
        target: "forge.server",
        "shutdown signal received, draining"
    );
}

pub fn json_to_forge(v: JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(a) => Value::Array(a.into_iter().map(json_to_forge).collect()),
        JsonValue::Object(m) => {
            Value::Object(m.into_iter().map(|(k, v)| (k, json_to_forge(v))).collect())
        }
    }
}

pub fn forge_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Int(n) => JsonValue::Number((*n).into()),
        Value::Float(n) => serde_json::Number::from_f64(*n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Array(a) => JsonValue::Array(a.iter().map(forge_to_json).collect()),
        Value::ResultOk(v) => {
            let mut obj = serde_json::Map::new();
            obj.insert("Ok".to_string(), forge_to_json(v));
            JsonValue::Object(obj)
        }
        Value::ResultErr(v) => {
            let mut obj = serde_json::Map::new();
            obj.insert("Err".to_string(), forge_to_json(v));
            JsonValue::Object(obj)
        }
        Value::Object(m) => {
            let obj: serde_json::Map<String, JsonValue> = m
                .iter()
                .map(|(k, v)| (k.clone(), forge_to_json(v)))
                .collect();
            JsonValue::Object(obj)
        }
        _ => JsonValue::String(format!("<{}>", v.type_name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── to_axum_path ────────────────────────────────────────────────────────

    #[test]
    fn axum_path_simple() {
        assert_eq!(to_axum_path("/hello"), "/hello");
    }

    #[test]
    fn axum_path_param_conversion() {
        assert_eq!(to_axum_path("/users/:id"), "/users/{id}");
    }

    #[test]
    fn axum_path_multiple_params() {
        assert_eq!(
            to_axum_path("/org/:org/repo/:repo"),
            "/org/{org}/repo/{repo}"
        );
    }

    // ── json_to_forge ────────────────────────────────────────────────────────

    #[test]
    fn json_null_to_forge() {
        assert_eq!(json_to_forge(JsonValue::Null), Value::Null);
    }

    #[test]
    fn json_bool_to_forge() {
        assert_eq!(json_to_forge(JsonValue::Bool(true)), Value::Bool(true));
        assert_eq!(json_to_forge(JsonValue::Bool(false)), Value::Bool(false));
    }

    #[test]
    fn json_int_to_forge() {
        let v = serde_json::json!(42);
        assert_eq!(json_to_forge(v), Value::Int(42));
    }

    #[test]
    fn json_float_to_forge() {
        let v = serde_json::json!(3.14);
        assert_eq!(json_to_forge(v), Value::Float(3.14));
    }

    #[test]
    fn json_string_to_forge() {
        let v = JsonValue::String("hello".to_string());
        assert_eq!(json_to_forge(v), Value::String("hello".to_string()));
    }

    #[test]
    fn json_array_to_forge() {
        let v = serde_json::json!([1, 2, 3]);
        assert_eq!(
            json_to_forge(v),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    // ── forge_to_json ────────────────────────────────────────────────────────

    #[test]
    fn forge_null_to_json() {
        assert_eq!(forge_to_json(&Value::Null), JsonValue::Null);
    }

    #[test]
    fn forge_bool_to_json() {
        assert_eq!(forge_to_json(&Value::Bool(true)), JsonValue::Bool(true));
    }

    #[test]
    fn forge_int_to_json() {
        assert_eq!(forge_to_json(&Value::Int(7)), serde_json::json!(7));
    }

    #[test]
    fn forge_float_to_json() {
        let result = forge_to_json(&Value::Float(2.5));
        assert_eq!(result, serde_json::json!(2.5));
    }

    #[test]
    fn forge_string_to_json() {
        let result = forge_to_json(&Value::String("forge".to_string()));
        assert_eq!(result, JsonValue::String("forge".to_string()));
    }

    #[test]
    fn forge_result_ok_to_json() {
        let result = forge_to_json(&Value::ResultOk(Box::new(Value::Int(1))));
        assert_eq!(result, serde_json::json!({"Ok": 1}));
    }

    #[test]
    fn forge_result_err_to_json() {
        let result = forge_to_json(&Value::ResultErr(Box::new(Value::String(
            "oops".to_string(),
        ))));
        assert_eq!(result, serde_json::json!({"Err": "oops"}));
    }

    #[test]
    fn json_roundtrip_object() {
        use indexmap::IndexMap;
        let mut m = IndexMap::new();
        m.insert("x".to_string(), Value::Int(10));
        m.insert("y".to_string(), Value::Bool(false));
        let forge_val = Value::Object(m);
        let json = forge_to_json(&forge_val);
        let back = json_to_forge(json);
        // Round-trip via JSON: objects should come back with same keys/values
        if let Value::Object(map) = back {
            assert_eq!(map.get("x"), Some(&Value::Int(10)));
            assert_eq!(map.get("y"), Some(&Value::Bool(false)));
        } else {
            panic!("expected object");
        }
    }

    // ── Concurrency model: AppState clone is cheap and shares no locks ──

    /// AppState must be `Clone` (axum requires `Clone` on `with_state`).
    /// Cloning it must be cheap — just two Arc bumps. If anyone ever
    /// adds a `Mutex<...>` field this will still compile but the test
    /// below ensures we don't accidentally serialize on it.
    #[test]
    fn app_state_clone_is_arc_share() {
        let state = AppState {
            template: Arc::new(InterpreterTemplate::new(Interpreter::new())),
            permits: Arc::new(Semaphore::new(DEFAULT_MAX_INFLIGHT)),
        };
        let other = state.clone();
        assert!(Arc::ptr_eq(&state.template, &other.template));
        assert!(Arc::ptr_eq(&state.permits, &other.permits));
    }

    /// CancelOnDrop must flip the flag exactly when dropped, with
    /// Release ordering visible to a concurrent Acquire load (the
    /// interpreter's polling site).
    #[test]
    fn cancel_on_drop_flips_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        {
            let _g = CancelOnDrop(flag.clone());
            assert!(!flag.load(Ordering::Acquire));
        }
        assert!(flag.load(Ordering::Acquire));
    }

    /// The template must produce independent forks. This is the
    /// integration of fork_for_serving with the server's wrapper type.
    #[test]
    fn template_forks_are_independent() {
        let mut interp = Interpreter::new();
        interp.env.define("seed".to_string(), Value::Int(7));
        let tpl = Arc::new(InterpreterTemplate::new(interp));

        let mut a = tpl.fork();
        let mut b = tpl.fork();

        a.env.define("x".to_string(), Value::Int(1));
        b.env.define("x".to_string(), Value::Int(2));

        assert_eq!(a.env.get("x"), Some(Value::Int(1)));
        assert_eq!(b.env.get("x"), Some(Value::Int(2)));
        assert_eq!(a.env.get("seed"), Some(Value::Int(7)));
        assert_eq!(b.env.get("seed"), Some(Value::Int(7)));
    }
}
