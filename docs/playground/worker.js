// Runs Forge (WebAssembly) off the main thread for client.js.
//
// Protocol (all messages carry the request `id`):
//   in:  { id, op: "init", module }            module: compiled WebAssembly.Module
//        { id, op: "run", source, options }
//        { id, op: "check", source }
//        { id, op: "format", source }
//   out: { id, ok: true, value }   or   { id, ok: false, error }
// After a crash (`value.crashed`), the worker closes itself; the client
// starts a new one.

import { load } from "./forge.js";

let forge = null;

self.onmessage = async (event) => {
  const { id, op } = event.data;
  try {
    if (op === "init") {
      forge = await load(event.data.module);
      self.postMessage({ id, ok: true, value: forge.version() });
      return;
    }
    if (!forge) throw new Error("worker used before init");
    let value;
    if (op === "run") value = forge.run(event.data.source, event.data.options || {});
    else if (op === "check") value = forge.check(event.data.source);
    else if (op === "format") value = forge.format(event.data.source);
    else throw new Error("unknown op " + op);
    self.postMessage({ id, ok: true, value });
    if (forge.crashed) self.close();
  } catch (err) {
    self.postMessage({ id, ok: false, error: String((err && err.message) || err) });
    if (forge && forge.crashed) self.close();
  }
};
