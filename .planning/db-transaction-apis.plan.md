# DB Transaction APIs Plan

## Goal

Close production-readiness F1 by making transaction APIs consistently available across `db`, `pg`, and `mysql`.

## Current State

- `db` already exposes `db.begin`, `db.commit`, and `db.rollback` as raw `BEGIN` / `COMMIT` / `ROLLBACK` on the single SQLite connection.
- `db` already has commit and rollback unit tests.
- `pg` already exposes `pg.begin`, `pg.commit`, and `pg.rollback` through `batch_execute` on the stored `Arc<Client>`.
- `pg` has no live transaction test because tests do not provision Postgres.
- `mysql` intentionally does not expose transaction helpers today. The current pool-based `mysql.query` / `mysql.execute` acquire a fresh pooled connection per call, so a naive `BEGIN` followed by later `COMMIT` could run on different physical connections and silently fail transaction semantics.

## API

- Keep existing APIs:
  - `db.begin()`, `db.commit()`, `db.rollback()`
  - `pg.begin()`, `pg.commit()`, `pg.rollback()`
- Add MySQL APIs with explicit transaction handles:
  - `let tx = mysql.begin(conn_id)`
  - `mysql.query(tx, sql, params?)`
  - `mysql.execute(tx, sql, params?)`
  - `mysql.commit(tx)`
  - `mysql.rollback(tx)`

`mysql.begin` deliberately returns an opaque transaction id instead of making
the original pooled `conn_id` transactional. This avoids changing a process-wide
pooled connection id into implicit mutable transaction state that could be
shared across HTTP request forks or unrelated interpreter sessions.

## Implementation

1. Add `mysql.begin`, `mysql.commit`, and `mysql.rollback` to `mysql::create_module()` and dispatch.
2. Add an active-transaction map:
   - key: opaque Forge MySQL transaction id (`mysql_tx_<uuid>`)
   - value: active transaction record containing the origin pool connection id and `Arc<tokio::sync::Mutex<mysql_async::Conn>>`
   - the map is process-global (`OnceLock`) like the existing MySQL pool map, so transaction handle strings survive interpreter forks and resolve to the same physical pinned connection. This is intentional and matches the existing pool-id semantics; callers must not leak tx ids across unrelated requests if they want isolation.
3. `mysql.begin(conn_id)`:
   - fail if `conn_id` is not known in the pool map
   - get one pooled connection
   - execute raw `BEGIN`
   - generate a UUID-backed transaction id
   - store that physical connection in the transaction map under the transaction id
   - return the transaction id as a string
4. `mysql.query` and `mysql.execute`:
   - treat the first string argument as either a transaction id or a pool connection id
   - first check for an active transaction connection for that id
   - if present, run on that pinned connection
   - otherwise keep current pooled one-shot behavior
5. `mysql.commit(tx_id)` / `mysql.rollback(tx_id)`:
   - execute raw `COMMIT` / `ROLLBACK` on that exact connection
   - only remove the pinned connection from the transaction map after successful completion
   - if commit/rollback fails, leave the transaction handle present so the caller can retry or rollback
   - return `Null`
   - fail if no transaction is active for the supplied id
6. `mysql.close(conn_id)`:
   - check whether any active transaction record originated from `conn_id`
   - if yes, return an error refusing to close until the caller commits or rolls back those tx handles
   - if no, keep existing pool removal behavior
7. Concurrency:
   - query/execute/commit/rollback lock the transaction connection while the SQL operation is in flight
   - this serializes operations on the same transaction handle and prevents concurrent protocol use on one MySQL connection
   - separate transaction handles use separate pooled physical connections

## Tests

- SQLite: keep existing commit/rollback tests; run `cargo test stdlib::db --lib`.
- Postgres: keep current no-connection error-path coverage; do not add a fake live test without a provisioned server. If a `FORGE_PG_TEST_URL` convention already exists, add an ignored/env-gated live transaction test; otherwise leave live coverage for integration infrastructure.
- MySQL:
  - module exposes `begin`, `commit`, `rollback`
  - wrong/missing connection id errors for begin
  - wrong/missing transaction id errors for commit/rollback
  - `mysql.query` / `mysql.execute` continue to error for unknown ids
  - if `FORGE_MYSQL_TEST_URL` is set, add an ignored/env-gated live test covering:
    - `begin(conn)` returns a tx id distinct from the connection id
    - `execute(tx, insert)` followed by `rollback(tx)` leaves row absent
    - `execute(tx, insert)` followed by `commit(tx)` leaves row present
    - concurrent transactions from the same pool id do not see each other's uncommitted writes
    - concurrent operations on the same tx handle serialize without panic/deadlock
    - `mysql.close(conn)` refuses while a tx from that pool is outstanding
- Always run:
  - `cargo fmt`
  - `cargo test stdlib::db --lib`
  - `cargo test stdlib::pg --lib`
  - `cargo test stdlib::mysql --lib`
  - `cargo test`

## Rollback

Remove the MySQL transaction map and the three MySQL dispatch arms. SQLite and Postgres remain unchanged.

## Deferred

- Moving DB handles into interpreter-scoped state would be cleaner long term, but today `db`, `pg`, and `mysql` are all stdlib modules backed by module-local process/thread state. This plan avoids adding implicit per-connection transaction state and uses opaque transaction ids to keep MySQL transaction ownership explicit.
- Live Postgres/MySQL transaction tests require provisioned services. This PR can add env-gated tests, but normal CI will still rely on unit tests unless service containers are added later.
- Forgotten MySQL transaction handles pin a connection until commit/rollback or process exit. TTL/cleanup sweeps are deferred.
- Nested transactions / SAVEPOINT support are deferred.
