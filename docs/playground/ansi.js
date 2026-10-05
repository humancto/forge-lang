// ANSI SGR (color/style) escape sequences -> HTML spans, for program
// output (`term.red(...)`, the GenZ debug kit, `log.*`). Other escape
// sequences (cursor movement, clear screen) are dropped.

import { escapeHtml } from "./highlight.js";

const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

export function ansiToHtml(text) {
  let html = "";
  const state = { bold: false, dim: false, italic: false, underline: false, fg: null, bg: null };
  // ESC [ params final-byte
  const re = /\x1b\[([0-9;?]*)([A-Za-z])/g;
  let last = 0;
  let m;
  const emit = (chunk) => {
    if (!chunk) return;
    const cls = [];
    if (state.bold) cls.push("a-bold");
    if (state.dim) cls.push("a-dim");
    if (state.italic) cls.push("a-italic");
    if (state.underline) cls.push("a-underline");
    if (state.fg) cls.push("a-fg-" + state.fg);
    if (state.bg) cls.push("a-bg-" + state.bg);
    const safe = escapeHtml(chunk);
    html += cls.length ? `<span class="${cls.join(" ")}">${safe}</span>` : safe;
  };
  while ((m = re.exec(text))) {
    emit(text.slice(last, m.index));
    last = re.lastIndex;
    if (m[2] !== "m") continue;
    const codes = m[1] === "" ? [0] : m[1].split(";").map(Number);
    for (let k = 0; k < codes.length; k++) {
      const c = codes[k];
      if (c === 0) Object.assign(state, { bold: false, dim: false, italic: false, underline: false, fg: null, bg: null });
      else if (c === 1) state.bold = true;
      else if (c === 2) state.dim = true;
      else if (c === 3) state.italic = true;
      else if (c === 4) state.underline = true;
      else if (c === 22) state.bold = state.dim = false;
      else if (c === 23) state.italic = false;
      else if (c === 24) state.underline = false;
      else if (c >= 30 && c <= 37) state.fg = NAMES[c - 30];
      else if (c >= 90 && c <= 97) state.fg = "bright-" + NAMES[c - 90];
      else if (c === 39) state.fg = null;
      else if (c >= 40 && c <= 47) state.bg = NAMES[c - 40];
      else if (c >= 100 && c <= 107) state.bg = "bright-" + NAMES[c - 100];
      else if (c === 49) state.bg = null;
      else if (c === 38 || c === 48) k += codes[k + 1] === 5 ? 2 : 4; // 256/true color: skip
    }
  }
  emit(text.slice(last).replace(/\x1b/g, ""));
  return html;
}
