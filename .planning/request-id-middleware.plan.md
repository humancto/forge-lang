# Per-Request Request-ID Middleware

## TL;DR

PR #118 wired `tower_http::trace::TraceLayer` and `#[tracing::instrument]`
on `run_handler` so every request has a span carrying `method`, `uri`,
`version`, `handler`, `status`, `latency`. Missing: a stable per-request
**request ID** that ties handler logs, response headers, and the upstream
client's logs together.

This PR adds:
1. `tower_http::request_id::SetRequestIdLayer` -- generates a UUID v4
   if the inbound request has no `X-Request-Id`, uses the inbound value
   otherwise.
2. `PropagateRequestIdLayer` -- echoes the resolved id back in the
   response `X-Request-Id` header so the client can correlate.
3. **A custom `make_span_with` closure on `TraceLayer`** that reads the
   resolved id from request extensions and records it on the OUTER
   `request` span, so the `on_response` event (the canonical per-request
   log line: status + latency) carries `request_id`.
4. The id is also recorded on the inner `forge.handler` span so events
   from inside the handler body show it explicitly. (Belt-and-suspenders
   -- the outer span is the load-bearing one because of `Span::record`'s
   per-span semantics.)

After this PR, every log line for a request carries `request_id`. A 500
in production logs can be grepped back to the exact upstream request.

> Reviewed by `rust-expert`: **REVISE -> applied below in `Review-driven
> changes`**. Showstopper: the original plan put the field only on the
> inner `forge.handler` span; `Span::record` does not propagate to
> parent spans, so the `tower_http::trace::on_response` event would
> have lacked the field.

## Scope (in)

1. Add `"request-id"` to the `tower-http` features list in `Cargo.toml`.
   This is a feature-list change, not a new direct dep -- `tower-http`
   is already a direct dependency. `MakeRequestUuid` and the layer
   types live behind this feature gate.
2. New imports in `src/runtime/server.rs`:
   - `axum::Extension`
   - `tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer}`
3. Stack `SetRequestIdLayer` (innermost) + existing `TraceLayer` (middle)
   + `PropagateRequestIdLayer` (outermost) per `tower-http`'s own
   documented composition order.
4. **Replace `DefaultMakeSpan` with a custom `make_span_with` closure**
   that reads `RequestId` from request extensions and records it on the
   outer `request` span. This is what makes the `on_response` event
   (and any tower-http-emitted event for this request) carry
   `request_id`.
5. Extend `run_handler`'s signature with `request_id: String`. Add
   `request_id = tracing::field::Empty` to `#[instrument]` fields. Call
   `Span::current().record("request_id", ...)` as the first body
   statement so handler-body events also carry it explicitly.
6. Each per-route closure (GET / POST / PUT / DELETE) extracts
   `Extension<RequestId>`, converts it to a `String` (with a
   defensive fallback that emits `tracing::warn!` on failure), and
   passes it into `run_handler`.
7. Length-cap the recorded `request_id` at 64 chars before recording
   on the span, so a hostile inbound 1KB header doesn't amplify log
   volume per event.
8. Integration test that asserts both: (a) response `X-Request-Id`
   echoes inbound or contains a valid UUID, and (b) the structured
   log captures the field in stderr (so we prove the full pipeline,
   not just the response-header path).
9. Document the contract in CLAUDE.md § Observability and CHANGELOG.

## Scope (out)

- W3C `traceparent` header support (separate follow-up; documented
  here as "future work").
- `X-Correlation-Id` as an alternative inbound header. Hardcoded to
  `X-Request-Id`.
- Allowing Forge handlers to read `request_id` directly via a
  `request.request_id` field. Today handlers receive `body`, `query`,
  and path params. A request object is a separate PR.
- Per-WS-message request_id. The WS upgrade request gets an id (the
  layer fires on it) and the upgrade-span events carry it; per-message
  events emitted from inside the `socket.recv().await` loop do not
  inherit the upgrade span (verified -- see § WS interaction).
- Configurable header name. Hardcoded to `X-Request-Id`.

## Review-driven changes (what changed from v1 of this plan)

| # | rust-expert finding | Resolution |
|---|---|---|
| 1 | **`Span::record` does not propagate to parents.** The plan recorded `request_id` only on the inner `forge.handler` span. The outer `tower_http` `request` span (and its `on_response` event) would lack the field. The TL;DR's "every log line carries request_id" was false as designed. | **Custom `make_span_with` closure on TraceLayer** that reads the id from extensions and includes it as a span field at outer-span construction. Inner-span recording stays as belt-and-suspenders. |
| 2 | Missing `axum::Extension` and `tower_http::request_id::*` import call-outs. | Spelled out in scope item 2. |
| 3 | "Already pulled in transitively" was misleading. `request-id` IS a feature-list change. | Re-worded in scope item 1. |
| 4 | The `to_str().unwrap_or("unknown")` fallback was silent. A hostile inbound `X-Request-Id` containing Latin-1 bytes that pass `HeaderValue::from_bytes` but fail `to_str` would silently substitute "unknown". | Wrap the fallback in `tracing::warn!(target: "forge.server", ...)` so operators see when it fires. |
| 5 | Test only proved the response-header path, not the structured-log path. | Test asserts both: response header AND that the structured log event captured by stderr contains `request_id`. |
| 6 | WS interaction not verified. | Audit confirms: `ws.on_upgrade(...)` returns a future that's polled inside the trace span, so events emitted DURING the upgrade carry the request_id. After upgrade, per-message work runs detached -- documented as out of scope. |
| 7 | No length cap on inbound `X-Request-Id`. A 1KB request_id in every log line is a log-amplification vector. | 64-char cap before recording (UUIDs are 36 chars; gives slack for client trace IDs but rejects pathological lengths). |
| 8 | `request_id` is in both `#[instrument(fields(...))]` and the function parameter -- a future reader will be confused. | Comment in code explaining the intent: `Empty` declaration wins for span creation, parameter is what we read in the body. |

## Design

### Layer stacking (verified against tower-http 0.6 docs)

axum applies `.layer()` outside-in: the LAST `.layer()` call wraps
everything below it. `tower-http`'s own request_id module docs
prescribe this exact composition:

```rust
let app = app
    .layer(cors_layer)                                          // innermost
    .layer(trace_layer)                                         // middle (uses make_span_with)
    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))    // sets id (runs before Trace on req path)
    .layer(PropagateRequestIdLayer::x_request_id())             // outermost (echoes on response)
    .with_state(state);
```

Request flow (outside-in): `Propagate -> Set -> Trace -> Cors -> handler`.
Response flow (inside-out): `handler -> Cors -> Trace -> Set -> Propagate`.

Critical: `Set` runs FIRST on the request path. By the time `Trace`
calls `make_span_with`, the request extensions already contain the
`RequestId`. Span creation reads it.

### Custom `make_span_with` for the OUTER span

```rust
let trace_layer = TraceLayer::new_for_http()
    .make_span_with(|req: &http::Request<_>| {
        let request_id = req
            .extensions()
            .get::<RequestId>()
            .and_then(|id| id.header_value().to_str().ok())
            // Cap to 64 chars to prevent inbound log amplification.
            .map(|s| if s.len() > 64 { &s[..64] } else { s })
            .unwrap_or("unknown");
        tracing::info_span!(
            "request",
            method = %req.method(),
            uri = %req.uri(),
            version = ?req.version(),
            request_id = request_id,
        )
    })
    .on_response(DefaultOnResponse::new().level(Level::INFO));
```

Now the outer `request` span has `request_id` as a first-class field.
The `on_response` event (emitted inside the outer span) carries it.

### Inner-span recording (belt and suspenders)

```rust
#[tracing::instrument(
    name = "forge.handler",
    level = "info",
    skip(state, path_params, query_params, body),
    fields(handler = %handler_name, request_id = tracing::field::Empty),
)]
async fn run_handler(
    state: AppState,
    handler_name: String,
    request_id: String,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
    body: Option<JsonValue>,
) -> Response {
    // Belt-and-suspenders: also record on the inner forge.handler span
    // so events emitted from this function (and via Span::current()
    // propagated into spawn_blocking) explicitly include the field.
    // The outer `request` span (set by TraceLayer::make_span_with above)
    // is the load-bearing place; this is for human-readable inner-span
    // events.
    tracing::Span::current().record("request_id", request_id.as_str());
    // ... rest unchanged ...
}
```

### Reading the id in per-route closures

```rust
use axum::Extension;
use tower_http::request_id::RequestId;

app.route(&axum_path, get(move |
    State(state): State<AppState>,
    Extension(rid): Extension<RequestId>,
    path: Option<Path<HashMap<String, String>>>,
    Query(query): Query<HashMap<String, String>>,
| async move {
    let request_id = match rid.header_value().to_str() {
        Ok(s) => {
            // Cap before passing into the handler -- log amplification defense.
            if s.len() > 64 { s[..64].to_string() } else { s.to_string() }
        }
        Err(_) => {
            tracing::warn!(
                target: "forge.server",
                "X-Request-Id header is not valid UTF-8; using \"unknown\""
            );
            "unknown".to_string()
        }
    };
    let params = path.map(|Path(p)| p).unwrap_or_default();
    run_handler(state, hn, request_id, params, query, None).await
}));
```

Repeat the extractor pattern for POST/PUT/DELETE. WS keeps its current
shape -- the `Extension<RequestId>` IS in the request because the layer
fires on the upgrade, but the WS handler doesn't pull it (per-message
work runs outside the trace span by design).

### Defensive fallback rationale

`SetRequestIdLayer` is non-optional: it's stacked on the router and
always runs. So the `Extension<RequestId>` extractor will always
succeed in practice. The `to_str()` fallback covers the rare case
where an inbound header parses as a valid HeaderValue but contains
non-ASCII bytes. The 64-char cap covers hostile inbound IDs that pass
all other validation. Both are belt-and-suspenders defaults.

### WS interaction (verified)

The WS handler today is:

```rust
get(move |State(state): State<AppState>, ws: axum::extract::WebSocketUpgrade| {
    let template = state.template.clone();
    let hn = hn.clone();
    async move {
        ws.on_upgrade(move |mut socket| async move {
            // ... per-message loop ...
        })
    }
})
```

`ws.on_upgrade(...)` returns a `Response` immediately; the closure
passed to it runs as a background tokio task after the upgrade
handshake completes. That background task is **detached from the
request future** -- it's not polled inside the TraceLayer span. So
per-message events don't inherit `request_id` (or any of `method`,
`uri`, etc.).

This is documented as out of scope: WS request_id is recorded only on
the upgrade request itself (which IS handled inside the trace span);
per-message events from the post-upgrade loop run detached. A separate
PR could add a per-connection span around the on_upgrade closure if
WS observability becomes a priority.

## Tasks

| # | File | Change |
|---|---|---|
| 1 | `Cargo.toml` | Add `"request-id"` to `tower-http` features. |
| 2 | `src/runtime/server.rs` | Add imports: `axum::Extension`, `tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer}`. |
| 3 | `src/runtime/server.rs` | Replace the existing `make_span_with(DefaultMakeSpan::...)` with the custom closure that reads the id from extensions and adds it as a span field. |
| 4 | `src/runtime/server.rs` | Stack `SetRequestIdLayer::x_request_id(MakeRequestUuid)` and `PropagateRequestIdLayer::x_request_id()` around the existing trace_layer in `start_server` per the documented order. |
| 5 | `src/runtime/server.rs` | Extend `run_handler` signature: add `request_id: String` parameter. Add `request_id = tracing::field::Empty` to `#[instrument]` fields. Call `Span::current().record("request_id", request_id.as_str())` as the first body statement. |
| 6 | `src/runtime/server.rs` | Each per-route closure (GET/POST/PUT/DELETE) extracts `Extension<RequestId>`, converts to `String` with the 64-char cap and the `to_str()` fallback that emits `tracing::warn!`. WS unchanged. |
| 7 | `tests/server_concurrency.rs` | New test `request_id_is_generated_and_propagated`: hits a handler twice (one without `X-Request-Id`, one with). Asserts: response carries `x-request-id`; with-header case echoes inbound; without-header case is a UUID. |
| 8 | `CLAUDE.md` § Observability | Document the `request_id` field, layer order, max-length cap, WS limitation, and the X-Request-Id header behavior. |
| 9 | `CHANGELOG.md` | `[Unreleased] -> Added` entry with performance footnote (~1µs/request from getrandom). |

## Acceptance criteria

- [ ] `cargo test --lib` passes (1487 baseline).
- [ ] `cargo test --test server_concurrency` passes (3 tests now: existing 2 ratio + new request-id).
- [ ] `forge run examples/bench_server_concurrent.fg` and
      `curl -i http://127.0.0.1:9090/ping` shows `x-request-id: <uuid>`
      in the response.
- [ ] `curl -H "X-Request-Id: my-trace-123" http://127.0.0.1:9090/ping -i`
      shows `x-request-id: my-trace-123` in response.
- [ ] `FORGE_LOG_FORMAT=json forge run ... 2>&1 | grep request_id` shows
      structured events carrying the field on BOTH the outer
      `tower_http::trace::on_response` event AND inner handler events.
- [ ] `cargo fmt --check` clean.
- [ ] CLAUDE.md and CHANGELOG updated.

## Commit breakdown

```
feat(server): wire SetRequestIdLayer + PropagateRequestIdLayer
feat(server): record request_id on outer trace span via custom make_span_with
feat(server): plumb request_id through run_handler with length cap and warn-on-fallback
test(server): integration test for X-Request-Id generation and echo
docs: document request_id observability behavior
```

## Risks (post-revision)

| Risk | Mitigation |
|---|---|
| `Span::record` semantics misunderstood (the showstopper from review) | `make_span_with` adds the field at outer-span CREATION, not via `record`. The inner-span `record` is belt-and-suspenders. |
| Layer order off-by-one | Documented inline + integration test asserts the field is present in stderr. |
| Hostile 1KB `X-Request-Id` | 64-char cap before recording. |
| Hostile non-ASCII `X-Request-Id` that passes HeaderValue parsing | `to_str()` fallback to "unknown" + `tracing::warn!` so operators notice. |
| WS per-message events lose request_id | Documented as out of scope; the upgrade request itself IS captured. |
| `MakeRequestUuid` panic on RNG failure | `getrandom` syscall on Linux is essentially infallible; if it fails, panic propagates and is caught by the existing `JoinError::is_panic` path -> 500 to client. |
