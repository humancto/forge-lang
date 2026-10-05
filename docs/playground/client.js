// Main-thread side of the playground runtime.
//
// The .wasm module is fetched and compiled once; each Web Worker
// instantiates it (a compiled WebAssembly.Module can be posted to workers).
// Two workers are used so diagnostics and formatting stay responsive while
// a program runs:
//   * the *run* worker executes programs. It is terminated and replaced when
//     the user presses Stop, when a run exceeds the wall-clock watchdog
//     (a backstop: the VM/interpreter budgets normally stop runaway code
//     deterministically first), or when the instance crashes;
//   * the *tool* worker answers check() and format().

const WASM_URL = new URL("./pkg/forge_wasm_bg.wasm", import.meta.url);
const WORKER_URL = new URL("./worker.js", import.meta.url);

/** Default wall-clock watchdog for one run, in milliseconds. */
export const DEFAULT_WATCHDOG_MS = 15000;

async function compileModule() {
  if (WebAssembly.compileStreaming) {
    try {
      return await WebAssembly.compileStreaming(fetch(WASM_URL));
    } catch (_) {
      // Wrong MIME type from a simple static server: fall back below.
    }
  }
  const response = await fetch(WASM_URL);
  if (!response.ok) throw new Error(`cannot load ${WASM_URL} (${response.status})`);
  return WebAssembly.compile(await response.arrayBuffer());
}

class WorkerHandle {
  constructor(module) {
    this.module = module;
    this.worker = null;
    this.ready = null;
    this.nextId = 1;
    this.pending = new Map();
  }

  start() {
    if (this.ready) return this.ready;
    const worker = new Worker(WORKER_URL, { type: "module" });
    this.worker = worker;
    worker.onmessage = (event) => {
      const { id, ok, value, error } = event.data;
      const entry = this.pending.get(id);
      if (!entry) return;
      this.pending.delete(id);
      if (ok) entry.resolve(value);
      else entry.reject(new Error(error));
      // A crashed instance closes its worker: forget it so the next call
      // starts a fresh one.
      if ((value && value.crashed) || (!ok && this.worker === worker)) this.reset();
    };
    worker.onerror = (event) => {
      event.preventDefault();
      this.fail(new Error(event.message || "the Forge worker failed to start"));
    };
    this.ready = this.request("init", { module: this.module });
    return this.ready;
  }

  request(op, payload) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, op, ...payload });
    });
  }

  async call(op, payload) {
    await this.start();
    return this.request(op, payload);
  }

  /** Hard-stop: terminate the worker and reject what is in flight. */
  fail(reason) {
    const pending = [...this.pending.values()];
    this.pending.clear();
    this.reset();
    for (const p of pending) p.reject(reason);
  }

  reset() {
    if (this.worker) this.worker.terminate();
    this.worker = null;
    this.ready = null;
  }
}

/** Promise-based Forge runtime for a page. */
export class ForgeClient {
  static async create(options = {}) {
    const module = await compileModule();
    const client = new ForgeClient(module, options);
    client.version = await client.tool.start();
    // Warm the run worker so the first Run is instant.
    client.runner.start().catch(() => {});
    return client;
  }

  constructor(module, { watchdogMs = DEFAULT_WATCHDOG_MS } = {}) {
    this.watchdogMs = watchdogMs;
    this.runner = new WorkerHandle(module);
    this.tool = new WorkerHandle(module);
    this.activeRun = null;
  }

  /** Run a program; resolves with forge.js's run result (never rejects). */
  run(source, options = {}) {
    this.stop("superseded by a new run");
    let settle;
    const done = new Promise((resolve) => (settle = resolve));
    const stopped = (message) =>
      settle({
        ok: false,
        stopped: true,
        stdout: "",
        stderr: "",
        output: [],
        truncated: false,
        engine: "",
        elapsed_ms: 0,
        error: { kind: "limit", message, line: 0, col: 0 },
      });
    const timer = setTimeout(() => {
      this.runner.fail(new Error("watchdog"));
      stopped(`stopped after ${Math.round(this.watchdogMs / 1000)} s (wall-clock limit)`);
    }, this.watchdogMs);
    this.activeRun = { stop: stopped, timer };
    this.runner
      .call("run", { source, options })
      .then(settle, (err) => stopped(err.message === "watchdog" ? "stopped" : err.message))
      .finally(() => {
        clearTimeout(timer);
        if (this.activeRun && this.activeRun.timer === timer) this.activeRun = null;
      });
    return done;
  }

  /** Stop the current run, if any (terminates the run worker). */
  stop(message = "the run was stopped before it finished") {
    const active = this.activeRun;
    if (!active) return false;
    this.activeRun = null;
    clearTimeout(active.timer);
    this.runner.fail(new Error("stopped"));
    active.stop(message);
    return true;
  }

  check(source) {
    return this.tool.call("check", { source });
  }

  format(source) {
    return this.tool.call("format", { source });
  }
}
