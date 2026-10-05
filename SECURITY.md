# Security Policy

## Supported Versions

Security fixes land on the latest minor release line. Older lines are not patched; upgrade instead.

| Version | Supported |
| ------- | --------- |
| 0.8.x   | Yes       |
| < 0.8   | No        |

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

Scope: confinement covers the `fs` module only. Other builtins that touch files (`csv.read`/`csv.write`, `db.open`, `download`, and similar) are not confined yet.

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

## Known Limitations

These are documented limitations, not vulnerabilities:

- Forge has no capability-based sandbox yet. Apart from the controls above, a Forge program has the same OS permissions as the process running it. A default-deny permission model with resource limits is on the [roadmap](ROADMAP.md).
- `unsafe` Rust is used in a few places: the C ABI entry points for native/AOT binaries (`src/lib.rs`), and the JIT's native-code calls and runtime helpers (`src/vm/jit/`, `src/vm/machine.rs`). Reports involving these are especially welcome.
- `ask` and `forge chat` send prompts to the configured LLM provider. Do not put secrets in prompts.

## Security Best Practices

- Pass user input to SQL only as query parameters.
- Do not pass untrusted input to `sh()` / `run_command()`, and only grant `--allow-run` to scripts you trust.
- Set `FORGE_FS_BASE` when running scripts that should only touch one directory.
- Leave `FORGE_HTTP_ALLOW_PRIVATE` unset in production.
- Keep secrets in environment variables (`env.get()`), never hardcoded.
