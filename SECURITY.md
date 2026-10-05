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

### Native plugins are full trust (`--allow-ffi`)

> **Loading a native plugin (`import native "libfoo"`) runs arbitrary machine code inside the Forge process.** Nothing Forge enforces — no capability, `--sandbox`, `FORGE_FS_BASE`, the SSRF guard, `--max-time` or the MCP output capture — applies to code inside the library. It can read and write any file, open sockets, spawn processes, and corrupt or crash Forge. Granting `ffi` is equivalent to granting everything.

- `forge run` and `forge test` refuse to load native code unless `--allow-ffi` (any library) or `--allow-ffi=PATHS` (only libraries at or under those paths) is given, or `allow-ffi` is set in `forge.toml` `[permissions]`. It is opt-in like `--allow-run`, because a script could otherwise write a library to disk and load it to escape every other restriction.
- The REPL and `forge -e` allow it (a person is typing), unless `--sandbox` is given.
- `--sandbox`, `forge mcp` and the embedding `Sandbox` deny it unless explicitly granted (`--allow-ffi=PATHS` / `Sandbox::allow_ffi`). Never grant `ffi` to code you would not run as a native binary yourself.
- The check runs on the resolved, canonical library path **before** the library is opened (opening already runs its initializers), so `..` and symlinks cannot widen a scoped grant. There is no library search path: Forge only loads the file the program names, relative to the importing file or the working directory.
- Prefer path-scoped grants to a directory only you can write to (`--allow-ffi=./plugins`). A writable plugin directory lets anyone who can write there run code as you.
- Plugins are never unloaded, and they must be thread-safe. A plugin that violates the ABI (`crates/forge-plugin/include/forge_plugin.h`) can cause undefined behaviour; Forge validates the descriptor and every returned value it can check (tags, UTF-8, NULL pointers, nesting depth), but cannot protect against a library that writes through bad pointers.

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
| `ffi` | `import native` — loading a native plugin. **Full trust**: see "Native plugins are full trust" | `--allow-ffi[=PATHS]` |

### CLI

Defaults: `forge run` allows everything except `run` and `ffi`; `-e` and the REPL also allow `run` and `ffi`. `--sandbox` switches to default-deny, and each `--allow-*` flag grants one capability back:

```bash
forge run --sandbox agent.fg                                   # nothing allowed
forge run --sandbox --allow-read=./data --allow-write=./out agent.fg
forge run --sandbox --allow-net=api.example.com,*.cdn.example.com agent.fg
forge run --allow-read=./data app.fg      # scoped flag restricts just that capability
forge run --max-time 30 job.fg            # wall-clock limit, exit code 124
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
allow-ffi = ["./plugins"] # native plugins: full trust
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
- Lower-level: `forge_lang::Capabilities` (policy builder), `forge_lang::permissions::{set_global, scope, require}`.

### MCP server (`forge mcp`)

`forge mcp` exposes the embedding sandbox to AI agents over the Model Context Protocol (stdio). Its policy is built like the CLI's but **always starts from deny-all**, including `process`; only the `--allow-*` flags and `forge.toml` `[permissions]` grant capabilities (`sandbox = false` is ignored, and `run` needs an explicit `--allow-run` / `allow-run = true`). `--max-time` (default 30s) bounds each call rather than the server process.

- Every `run_forge` call gets a fresh interpreter on its own thread; nothing persists between calls. A timed-out script is cancelled cooperatively and, if stuck in a native call, detached; at most 8 calls run at once.
- Script output is captured (including `spawn`ed tasks, `timeout` blocks, imports and `io.print`), capped at 64 KiB in the response, and a script that prints more than 1 MiB is stopped.
- On Unix the protocol stream is moved to private close-on-exec descriptors before any script runs; fd 0 becomes `/dev/null` and fd 1 is redirected to stderr. A script (or a granted subprocess) can therefore neither read protocol input nor inject protocol output. On other platforms only the sandbox capture applies.
- Untrusted source cannot crash the server with deep nesting: the parser rejects nesting beyond a fixed depth.
- The server trusts its client: anyone who can write to its stdin can run code with the granted capabilities. Grant the narrowest scopes (`--allow-net=api.example.com`, `--allow-read=./data`).

### Not covered yet (future work)

- **Memory limit.** There is no heap cap yet; call depth is bounded by `--max-depth` / `FORGE_MAX_DEPTH`.
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
- Only pass `--allow-ffi` for plugins you built or trust as much as Forge itself; scope it to their directory.
- Run scripts you did not write with `--sandbox` and only the `--allow-*` grants they need; embed untrusted code with `forge_lang::Sandbox`.
- Set `FORGE_FS_BASE` when running scripts that should only touch one directory.
- Leave `FORGE_HTTP_ALLOW_PRIVATE` unset in production.
- Keep secrets in environment variables (`env.get()`), never hardcoded.
