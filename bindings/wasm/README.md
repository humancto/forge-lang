# Forge in the browser (`forge-wasm`)

The Forge language core compiled to WebAssembly, and the API behind the
[Forge Playground](../../docs/playground/) (`docs/playground/`).

```
forge-lang (--no-default-features)               bindings/wasm (this crate)
  lexer, parser, type checker, formatter      ──▶   run / check / format  ──wasm-bindgen──▶  pkg/forge_wasm.js
  interpreter, bytecode VM, pure stdlib                (JSON in/out)                           │
                                                                                               ▼
                         docs/playground: forge.js (typed API) → worker.js (Web Worker) → client.js → page
```

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.114 --locked   # must match Cargo.toml
bindings/wasm/build.sh                 # -> docs/playground/pkg/
node bindings/wasm/tools/gen-playground-data.mjs   # refresh examples + grammar
node --test bindings/wasm/tests/*.mjs  # headless smoke tests (Node 18+)
cd docs && python3 -m http.server      # open http://localhost:8000/playground/
```

`cargo test` in this directory runs the Rust API's tests natively (no
browser needed). Module workers do not load from `file://`; serve `docs/`
over HTTP.

Size (release, `opt-level = 3`, Rust 1.97): **4.5 MiB raw, 1.37 MiB gzipped**.
`opt-level = "z"` gives 1.05 MiB gzipped but runs the interpreter about 2.5x
slower; `FORGE_WASM_OPT=1 build.sh` additionally runs `wasm-opt` if you have
a recent binaryen.

## JS API (`docs/playground/forge.js`)

```js
import { load } from "./forge.js";
const forge = await load();          // or load(bytes | WebAssembly.Module | URL)

forge.run('say "hi"', { engine: "auto" });
// { ok, stdout, stderr, output: [{stream, text}], truncated, engine: "vm"|"interp",
//   elapsed_ms, error: null | { kind, message, line, col, code?, trace? } }
// error.kind: "syntax" | "unsupported" | "compile" | "runtime" | "limit"

forge.check(source);   // [{ severity, message, help?, code?, line, col, end_line, end_col }]
forge.format(source);  // string, same as `forge fmt`
```

`run` options: `engine` (`"auto"` = VM with interpreter fallback, like
`forge run`; `"vm"`; `"interp"`), `maxInstructions` (VM fuel, default 100M),
`maxSteps` (interpreter fuel, default 15M), `maxMemory` (bytes, default
256 MiB), `maxOutput` (bytes, default 1 MiB).

`forge.js` runs on the calling thread. In a page use `client.js`
(`ForgeClient`): it compiles the module once, runs programs in a Web Worker,
adds a Stop button's hard stop and a 15 s wall-clock watchdog, and restarts
the worker after a crash.

## Guarantees

* **Infinite loops end deterministically.** Each run executes under the
  same resource limits as `forge run --max-fuel/--max-memory` and the
  sandbox (`src/runtime/limits.rs`): fuel is one unit per VM instruction or
  per interpreter statement/call/loop iteration, charged at the engines'
  safe points; exhaustion is fatal, so `try`/`catch`, `safe` or `retry`
  cannot catch their way past it. Memory is capped too (VM heap accounting;
  `CountingAllocator` for the interpreter).
* **Output is captured and capped** (`runtime::stdio::capture`), stdout and
  stderr separately and in order, including `sus`, `cook`, `log.*`, `term.*`.
* **No host access, clear errors.** The core is built without the `host`
  feature: the permission policy is deny-all and a denial reads
  "`… is not available in the browser playground`". OS-bound modules (`http`,
  `ws`, `db`, `pg`, `mysql`, `os`) are registered as stand-ins with the real
  member lists (checked against the real modules in CI), so programs compile
  and fail only when they call them.
* **Thread constructs are rejected up front**, with their position:
  `spawn`, `squad`, `timeout`, `schedule`, `watch`, `@server`.
* **Recursion** is limited to 1,000 calls on the VM and 450 on the
  interpreter (the browser's JS stack is ~1 MB). A JS stack overflow or Rust
  panic is reported as an error and the worker is replaced.

## What works in the browser

| Works | Not available (clear runtime error) |
| --- | --- |
| Both syntaxes, functions, closures, ADTs, `match`, enum methods, structs/interfaces, `when`, `must`, `safe`, `try`/`catch`, `check`, `retry`, `repeat`, destructuring, pipes, streams, sets/maps/tuples, `freeze`, `@test` definitions | `http.*`, `fetch`, `grab`, `download`, `crawl`, `ask` (no network) |
| Bytecode VM and interpreter; type-checker diagnostics; formatter | `fs` reads/writes, `csv.read/write`, file `import`s (no file system); `fs.join_path`/`basename`/... work |
| `math`, `json`, `regex`, `crypto`, `jwt`, `csv.parse/stringify`, `toml`, `url`, `path`, `time`, `npc`, `term`, `log`, `io.print`, `uuid` | `db`, `pg`, `mysql`, `ws`, `os`, `env` |
| String/collection/functional builtins, Result/Option, GenZ debug kit, `cook`/`slay`, `wait` (busy-waits inside the worker) | `sh`, `shell`, `run_command`, `pipe_to`, `which`, `cd`, `exit` |
| Channels within one task (`send` then `receive`) | `spawn`, `squad`, `timeout`, `schedule`, `watch`, `@server`, `import native`; a `receive` that could never complete |

`input()` / `io.prompt` see an empty stdin.

## Deploying

`.github/workflows/playground.yml` builds and tests this crate on every
change. Publishing `docs/` (with the built `pkg/`) to GitHub Pages is opt-in:
set Pages' source to "GitHub Actions" and the repository variable
`PLAYGROUND_PAGES=true`. No secrets are needed.
