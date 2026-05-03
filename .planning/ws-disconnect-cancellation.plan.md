# WebSocket Disconnect Cancellation Plan

## Goal

Close issue #111 by making WebSocket handlers observe client disconnects at interpreter safe points, matching the cancellation behavior added for normal HTTP handlers.

## Current State

- HTTP handlers use a per-request `Arc<AtomicBool>` plus `CancelOnDrop`; the forked interpreter polls that flag and exits when axum drops the response future.
- WebSocket routes currently fork one interpreter per connection and hold it in a `parking_lot::Mutex`.
- The WS loop calls the Forge handler synchronously inside the async upgrade task. While a long handler is running, the task is not polling `socket.recv()`, so it cannot observe a closed socket or `Message::Close`.
- The forked WS interpreter currently keeps its default cancellation token instead of a connection-scoped token wired to socket lifecycle.

## Implementation

1. In the `"WS"` route branch in `src/runtime/server.rs`, create one connection-scoped `Arc<AtomicBool>` at upgrade time and assign it to the forked interpreter before wrapping it in the connection mutex.
2. Split the WebSocket into sender and receiver halves.
3. Spawn a lightweight receiver task that:
   - forwards `Message::Text` payloads to the main per-connection loop through a bounded Tokio channel with capacity 1,
   - uses non-blocking `try_send`; if a client sends more than one queued message while the previous handler is still running, treat that as connection backpressure overflow, set the cancel flag, and stop the receiver,
   - treats `Message::Close`, receive errors, and stream end as disconnect,
   - stores `true` into the connection cancel flag on disconnect.
4. Process text messages sequentially in the main loop:
   - run the Forge handler inside `tokio::task::spawn_blocking`, entering the current tracing span like the HTTP handler path,
   - clone the `Arc<parking_lot::Mutex<Interpreter>>` into the blocking closure; acquire and drop the `MutexGuard` entirely inside that closure. The guard must never be held across an `.await` or acquired on the async side before entering `spawn_blocking`,
   - after the handler returns, skip sending if the cancel flag was set,
   - if `sender.send()` fails, set the cancel flag and stop.
5. Add a local drop guard for the upgrade task so any exit path sets the cancel flag.
6. Respect client `Message::Close` by setting the flag and terminating the connection loop.
7. Preserve current non-cancellation error semantics: if the handler returns an error and the connection is still active, send `error: <message>` back as before. If the cancel flag is set, skip the send because the peer is gone or the connection is closing.
8. Abort the receiver task when the main connection loop exits so shutdown does not leave a detached task holding connection resources.
9. Document the Ping/Pong assumption: axum 0.8 wraps tungstenite, whose codec handles automatic Pong responses before yielding messages. Non-text messages other than `Close` remain ignored as today.
10. Avoid changing per-connection state semantics: messages on one WS connection remain sequential and share the same forked interpreter; different WS connections stay isolated.

## Tests

Add an integration test in `tests/server_concurrency.rs` or a new WS-focused integration test using `tokio_tungstenite`:

1. Boot a Forge server with:
   - `/ping` for readiness,
   - a `@ws("/ws")` handler that writes a temp `started` sentinel, runs a long loop with at least one statement boundary per iteration, periodically writes a `progress` sentinel, and writes a `finished` sentinel only after the loop completes.
2. Connect a WS client, send one text message, wait until `started` proves the handler is running, then close/drop the client without waiting for a response.
3. Wait for `progress` to stop changing after disconnect. Because the handler writes progress from inside the loop body, a continued-running handler keeps changing this file; a cancelled handler stops.
4. Assert `finished` does not appear. If it appears, the loop completed normally instead of being cancelled.
5. Keep the loop body cancellation-friendly by using a statement boundary inside the `repeat` body; the interpreter checks `cancelled` at each `exec_stmt`.

Note: a Forge-level positive `after_safe` sentinel is not viable because the same cancellation flag remains set after `safe { ... }` catches the first `cancelled` error; the next statement would immediately observe cancellation before writing the sentinel.

Run:

- `cargo fmt`
- focused WS integration test
- `cargo test --test server_concurrency`
- `cargo test`

## Rollback

Revert the WS branch changes and remove the new integration test. HTTP handler cancellation remains unchanged.
