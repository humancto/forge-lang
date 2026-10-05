// Forge in WebAssembly: the typed JS API over the wasm-bindgen glue in
// ./pkg (built from bindings/wasm; see bindings/wasm/README.md).
//
//   import { load } from "./forge.js";
//   const forge = await load();                 // fetches ./pkg/forge_wasm_bg.wasm
//   const r = forge.run('say "hi"', { engine: "auto" });
//   r.stdout            // "hi\n"
//   r.error             // null | { kind, message, line, col, code?, trace? }
//   forge.check(src)    // [{ severity, message, help?, code?, line, col, end_line, end_col }]
//   forge.format(src)   // formatted source
//
// Runs synchronously on the calling thread: in a page, call it from a Web
// Worker (see worker.js / client.js) so a long program never blocks the UI.
//
// A Rust panic or a JS stack overflow aborts the WebAssembly instance (there
// is no unwinding in wasm32). `run` turns that into an error result with
// `crashed: true`; the instance is then unusable and every later call
// throws, so the owner must load a fresh one (client.js restarts its
// worker).

import init, * as raw from "./pkg/forge_wasm.js";

/**
 * Instantiate the module. `source` is anything wasm-bindgen accepts: a URL,
 * a Response, bytes or a compiled WebAssembly.Module. Defaults to the
 * .wasm file next to the glue.
 */
export async function load(source) {
  await init(source === undefined ? undefined : { module_or_path: source });
  return new Forge();
}

/** Readable message for an exception thrown out of the module. */
export function crashMessage(err) {
  const text = String((err && err.message) || err);
  if (err instanceof RangeError || /call stack/i.test(text)) {
    return "maximum recursion depth exceeded (the browser's call stack is full)";
  }
  let panic;
  try {
    panic = raw.last_panic();
  } catch (_) {
    panic = undefined;
  }
  return "internal error: " + (panic || text);
}

class Forge {
  constructor() {
    this.crashed = false;
  }

  version() {
    return this.#call(() => raw.version());
  }

  /**
   * Run a program. Options (all optional): engine ("auto" | "vm" |
   * "interp"), maxInstructions (VM fuel), maxSteps (interpreter fuel),
   * maxMemory (bytes), maxOutput (bytes).
   */
  run(source, options = {}) {
    try {
      return JSON.parse(this.#call(() => raw.run(source, JSON.stringify(options))));
    } catch (err) {
      if (!this.crashed) throw err;
      return {
        ok: false,
        crashed: true,
        stdout: "",
        stderr: "",
        output: [],
        truncated: false,
        engine: "",
        elapsed_ms: 0,
        error: { kind: "runtime", message: crashMessage(err), line: 0, col: 0 },
      };
    }
  }

  check(source) {
    return JSON.parse(this.#call(() => raw.check(source)));
  }

  format(source) {
    return this.#call(() => raw.format(source));
  }

  #call(f) {
    if (this.crashed) {
      throw new Error("the Forge WebAssembly instance crashed; load a new one");
    }
    try {
      return f();
    } catch (err) {
      // Anything thrown out of WebAssembly (a trap, a stack overflow)
      // leaves the instance in an unknown state.
      if (err instanceof WebAssembly.RuntimeError || err instanceof RangeError) {
        this.crashed = true;
      }
      throw err;
    }
  }
}
