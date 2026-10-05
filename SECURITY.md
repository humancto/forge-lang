# Security Policy

## Supported Versions

Security fixes land on the latest minor release line. Older lines are not patched; upgrade instead.

| Version | Supported |
| ------- | --------- |
| 0.9.x   | Yes       |
| < 0.9   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in Forge, **please do not open a public issue.**

Instead, report it privately:

1. Go to [Security Advisories](https://github.com/humancto/forge-lang/security/advisories/new)
2. Or email: **security@forge-lang.dev**

Include:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We will acknowledge your report within 48 hours and provide a timeline for a fix.

## Security Controls

Forge is a young language. These are the controls that exist today and their exact scope.

### Shell execution is opt-in (`--allow-run`)

`sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok`, `run_command`, and `pipe_to` return a permission error under `forge run` unless the `--allow-run` flag is passed:

```bash
forge --allow-run run deploy.fg
```

The REPL and `forge -e` enable shell execution automatically, since a person is typing the code. `forge build --native --allow-run` bakes the permission into a standalone binary; without it the binary denies shell execution.

### HTTP client SSRF guard (on by default)

The HTTP client (`http.*`, `fetch`, `download`, `crawl`) refuses requests whose host is, or resolves to, a private, loopback, or link-local address. The resolved address is pinned for the connection, and every redirect target is re-checked. To call local services on purpose (for example in development), set:

```bash
FORGE_HTTP_ALLOW_PRIVATE=1 forge run client.fg
```

### Filesystem confinement (`FORGE_FS_BASE`, opt-in)

When `FORGE_FS_BASE` is set to a directory, every `fs.*` operation is confined to paths under it, on both the VM and the interpreter. Paths are canonicalized with symlinks resolved, so `..` traversal and symlinks pointing outside the base are both rejected. When the variable is unset or empty, `fs.*` can reach any path the process can.

Scope: confinement covers the `fs` module, `csv.read`/`csv.write`, `toml.read`, `env.load`, SQLite `db.open` files and `http.download` destinations. For new code prefer the capability flags below (`--sandbox --allow-read=DIR --allow-write=DIR`), which apply the same symlink-aware resolution and can be combined with `FORGE_FS_BASE`; both checks must pass.

### Parameterized SQL queries

`db` (SQLite), `pg` (PostgreSQL) and `mysql` accept a params array. Always pass untrusted input as a parameter, never by string concatenation:

```forge
db.query("SELECT * FROM users WHERE name = ?", [name])
```

Note: the interpreter binds these parameters correctly. Parameter binding on the default bytecode VM is still being brought to parity, so verify with `--interp` if a query that takes parameters returns unexpected `null` columns. See the roadmap's VM parity work.

### HTTP server defaults

- `@server` binds to `127.0.0.1` unless you pass `host:` explicitly.
- CORS is restrictive (same-origin) by default. Opt into permissive CORS with `@server(port: 8080, cors: "permissive")`.
- At most 512 requests run at once by default; further requests get `503` with `Retry-After`.
- Each request runs in a forked interpreter, so one request cannot mutate another request's state.
- Inbound `X-Request-Id` headers are capped at 64 characters.

## Sandboxing and permissions

Forge has a capability-based permission model shared by the VM and the interpreter. Every privileged operation asks one central check against the active policy and fails with the same message shape:

```text
permission denied: fs.write (/etc/passwd) — run with --allow-write or grant it in the host policy
```

| Capability | Covers | CLI flag |
| --- | --- | --- |
| `fs.read` | `fs.read`/`list`/`exists`/`size`/`lines`/`read_json`/`is_dir`/`is_file`, `csv.read`, `toml.read`, `env.load`, `import` (see below) | `--allow-read[=PATHS]` |
| `fs.write` | `fs.write`/`append`/`remove`/`mkdir`/`copy`/`rename`/`write_json`, `csv.write`, `db.open` files, `http.download` destination | `--allow-write[=PATHS]` |
| `net` | `http.*`, `fetch`, `download`, `crawl`, `ws.connect`, binding an `@server` port | `--allow-net[=HOSTS]` |
| `env` | `env.*` | `--allow-env` |
| `db` | `db.*`, `pg.*`, `mysql.*` | `--allow-db` |
| `run` | `sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok`, `run_command`, `pipe_to` | `--allow-run` |
| `ai` | `ask` | `--allow-ai` |
| `process` | `exit()`, `cd()` (mutate the host process) | always granted by the CLI; host policy only |

### CLI

Defaults are unchanged: `forge run` allows everything except `run`; `-e` and the REPL also allow `run`. `--sandbox` switches to default-deny, and each `--allow-*` flag grants one capability back:

```bash
forge run --sandbox agent.fg                                   # nothing allowed
forge run --sandbox --allow-read=./data --allow-write=./out agent.fg
forge run --sandbox --allow-net=api.example.com,*.cdn.example.com agent.fg
forge run --allow-read=./data app.fg      # scoped flag restricts just that capability
forge run --max-time 30 job.fg            # wall-clock limit, exit code 124
forge run --max-fuel 50000000 job.fg      # deterministic step budget
forge run --max-memory 256MB job.fg       # memory limit
```

- Path scopes are resolved like the OS resolves them: made absolute, every existing component canonicalized (symlinks followed, `..` applied to the real parent), the not-yet-existing tail normalized. A symlink or `..` that leads outside a granted directory is denied; a dangling symlink is always denied. Grants are resolved once, at startup.
- `fs.exists`, `fs.is_dir` and `fs.is_file` return `false` instead of failing when the path is outside the grant, so they cannot probe for files.
- Host scopes match `host`, `*.domain` (subdomains and the domain itself) or `host:port`, case-insensitively. When `net` is scoped, every redirect target must also be on the allowlist. The SSRF guard still applies on top: granting `127.0.0.1` does not bypass it (use `FORGE_HTTP_ALLOW_PRIVATE=1`).
- `import` of modules under the entry script's directory (or the test directory, or `forge_modules/`) is always allowed; any other module path needs `fs.read`.
- The flags work before or after the subcommand (`forge --sandbox run x.fg` or `forge run --sandbox x.fg`) and apply to `forge test` as well.

The same policy can live in `forge.toml`; CLI flags override it per capability. A malformed `[permissions]` table (including an unknown key) is an error, never silently ignored:

```toml
[permissions]
sandbox = true
allow-read = ["./data"]   # or true
allow-write = ["./out"]
allow-net = ["api.example.com"]
allow-env = true
max-time = 30
```

### Embedding (Rust host API)

Hosts that run untrusted Forge code (AI agents, automation) use `forge_lang::Sandbox`, which starts from **deny-all** — including `process`, so a script cannot call `exit()` on the host:

```rust
use forge_lang::{Capability, Sandbox};
use std::time::Duration;

let out = Sandbox::new()
    .allow_read(["./data"])
    .allow_net(["api.example.com"])
    .allow(Capability::Ai)
    .max_time(Duration::from_secs(5))
    .run_source(script)?;          // Result<Output { stdout }, SandboxError>
```

- The policy is installed on the sandbox's worker thread, not process-wide, so concurrent sandboxes and the host itself are independent. Every thread the engines fork (`spawn`, `squad`, `timeout`, `schedule`, `watch`) inherits the policy of the thread that forked it (`permissions::spawn`).
- `say`/`println`/`print` output is captured in `Output::stdout`. Errors come back as `SandboxError::{Syntax, PermissionDenied, Runtime, Timeout}`.
- `max_time` returns control to the host by the deadline: the program is cancelled cooperatively, and if it does not stop within a short grace period its worker thread is detached.
- Resource limits (`max_fuel`, `max_memory`, or a full `forge_lang::Limits` via `.limits(...)`) come back as `SandboxError::{FuelExhausted, MemoryLimit, ResourceLimit}`; see "Resource limits" below. `max_memory` needs `forge_lang::CountingAllocator` as the host's `#[global_allocator]` (the run fails with a clear error otherwise).
- Lower-level: `forge_lang::Capabilities` (policy builder), `forge_lang::permissions::{set_global, scope, require}`.

### Resource limits

Wall-clock time is not enough for multi-tenant use: it depends on machine load and does not bound memory. Every run can also carry a budget (`src/runtime/limits.rs`), set with CLI flags, `forge_lang::Sandbox`, or `forge mcp` (which applies defaults):

| Limit | Meaning | CLI / API | Error |
| --- | --- | --- | --- |
| Fuel | Execution steps. VM: one per bytecode instruction; interpreter: one per statement, call and loop iteration. Deterministic: a single-threaded program runs out at exactly the same step on every run and every machine (per engine — the engines count different units). | `--max-fuel N`, `Sandbox::max_fuel` | `fuel exhausted: ...` (`FuelExhausted`) |
| Memory | VM: estimated live bytes of the GC heap (strings, arrays, objects, maps, sets, closures, boxed ints); crossing the limit forces a collection and the run fails only if the live heap is still too big. Interpreter: bytes held by the run's threads, measured by `CountingAllocator` and polled at every step. | `--max-memory 256MB`, `Sandbox::max_memory` | `memory limit exceeded: ...` (`MemoryLimit`) |
| Value size | Longest string and largest collection a program may build, checked *before* allocating (`repeat_str("x", 1e12)`, `range(1e12)`, `pad_start`, doubling `s = s + s`). With a memory limit they default to what the limit could hold. | `Limits::{max_string_bytes, max_collection_len}` | `resource limit exceeded: ...` (`ResourceLimit`) |
| Handles | Concurrently open files (fs calls, SQLite connections), sockets (HTTP requests, WebSockets, PostgreSQL/MySQL connections), subprocesses and tasks (`spawn`, `timeout` blocks). | `Limits::{max_open_files, max_sockets, max_processes, max_tasks}` | `resource limit exceeded: too many ...` |
| Imports | Module files loaded per run. | `Limits::max_imports` | `resource limit exceeded: more than N imports` |

- **Fuel and memory are fatal.** `try`/`catch`, `safe` and `retry` do not catch them, and the budget remembers the trip: if a builtin swallows the error (`assert_throws`, a failed task), every later step fails too and the host still reports the limit. The other limits are ordinary runtime errors a program may catch.
- **The host survives.** A tripped run unwinds normally; a sandbox's memory is released with its worker thread, and the next run starts from a fresh budget. Budgets are per run and inherited by every thread the run forks, never shared between sandboxes.
- **Overhead.** With no limits set, engines pay one decrement and branch per safe point they already had (VM: per instruction, folded into the existing `timeout` poll; interpreter: per statement/call/iteration), and the counting allocator pays one thread-local load per allocation. See CHANGELOG for measured numbers.
- **JIT.** Native code has no fuel counter, so while a fuel limit is active the VM does not enter JIT code (hot functions keep running in the VM). The JIT never allocates on the GC heap, so memory accounting is unaffected.
- **Approximations.** VM object sizes are estimates; the interpreter's meter also counts transient copies and may undercount memory allocated before the run and freed during it. Either engine can overshoot the memory limit by what a single step allocates (bounded by the value-size caps), and the VM by up to 1/8 of the limit when the live heap sits just under it.

### MCP server (`forge mcp`)

`forge mcp` exposes the embedding sandbox to AI agents over the Model Context Protocol (stdio). Its policy is built like the CLI's but **always starts from deny-all**, including `process`; only the `--allow-*` flags and `forge.toml` `[permissions]` grant capabilities (`sandbox = false` is ignored, and `run` needs an explicit `--allow-run` / `allow-run = true`). `--max-time` (default 30s) bounds each call rather than the server process.

- Every `run_forge` call gets a fresh interpreter on its own thread; nothing persists between calls. A timed-out script is cancelled cooperatively and, if stuck in a native call, detached; at most 8 calls run at once.
- Every call also gets a fresh resource budget: by default 200,000,000 steps of fuel, 256 MiB of memory, 32 open files, 16 sockets, 4 subprocesses, 16 tasks and 256 imports (`forge_lang::mcp::default_limits`). `--max-fuel` / `--max-memory` change the server's limits; an agent's `max_fuel` argument can only lower the fuel. Errors come back as `fuel_exhausted`, `memory_limit` or `resource_limit`.
- Script output is captured (including `spawn`ed tasks, `timeout` blocks, imports and `io.print`), capped at 64 KiB in the response, and a script that prints more than 1 MiB is stopped.
- On Unix the protocol stream is moved to private close-on-exec descriptors before any script runs; fd 0 becomes `/dev/null` and fd 1 is redirected to stderr. A script (or a granted subprocess) can therefore neither read protocol input nor inject protocol output. On other platforms only the sandbox capture applies.
- Untrusted source cannot crash the server with deep nesting: the parser rejects nesting beyond a fixed depth.
- The server trusts its client: anyone who can write to its stdin can run code with the granted capabilities. Grant the narrowest scopes (`--allow-net=api.example.com`, `--allow-read=./data`).

### Not covered yet (future work)

- **Memory accounting is approximate** (see "Resource limits"); call depth is bounded separately by `--max-depth` / `FORGE_MAX_DEPTH`. Builtins that run long without calling back into Forge code (sorting a huge array, a slow regex) are not charged fuel per element, and `replace`/`join` do not check the string cap before building their result.
- Fuel limits disable the JIT tier for the run.
- `--max-time` in the CLI ends the process from a watchdog thread; the embedding API cancels cooperatively. Neither interrupts a single blocking native call (a long HTTP request is bounded by its own timeout).
- HTTP server handlers run on tokio's blocking pool and use the process-wide policy (the CLI's). The embedding `Sandbox` does not start servers, `schedule` or `watch` blocks.
- `db` grants SQLite's own file access (`ATTACH DATABASE`), and `pg`/`mysql` open network connections under `db`, not `net`.
- `which()`, `input()`/`io.prompt`, `os.*` and `time` are not gated. stdin is shared with the host.
- Path checks and the subsequent open are not atomic: a process outside the sandbox that swaps a directory for a symlink between the two can win the race.
- The VM's `ws` module is not registered yet; on the interpreter `ws.connect` is gated by `net`.

## Known Limitations

These are documented limitations, not vulnerabilities:

- Without `--sandbox` (or a host policy), a Forge program keeps its historical defaults: it has the same OS permissions as the process running it, except for subprocesses. See "Sandboxing and permissions" for what the sandbox does and does not cover yet.
- `unsafe` Rust is used in a few places: the C ABI entry points for native/AOT binaries (`src/lib.rs`), and the JIT's native-code calls and runtime helpers (`src/vm/jit/`, `src/vm/machine.rs`). Reports involving these are especially welcome.
- `ask` and `forge chat` send prompts to the configured LLM provider. Do not put secrets in prompts.

## Security Best Practices

- Pass user input to SQL only as query parameters.
- Do not pass untrusted input to `sh()` / `run_command()`, and only grant `--allow-run` to scripts you trust.
- Run scripts you did not write with `--sandbox` and only the `--allow-*` grants they need; embed untrusted code with `forge_lang::Sandbox`.
- Set `FORGE_FS_BASE` when running scripts that should only touch one directory.
- Leave `FORGE_HTTP_ALLOW_PRIVATE` unset in production.
- Keep secrets in environment variables (`env.get()`), never hardcoded.
