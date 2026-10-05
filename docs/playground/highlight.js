// Syntax highlighting for the playground editor.
//
// A small single-pass tokenizer for Forge. Keyword, builtin, module, type
// and constant lists come from the VS Code TextMate grammar (via
// generated.js), so the playground highlights the same words editors do.

import { GRAMMAR } from "./generated.js";

const CLASS_OF_WORD = new Map();
for (const [list, cls] of [
  [GRAMMAR.builtins, "fn"],
  [GRAMMAR.types, "type"],
  [GRAMMAR.constants, "const"],
  [GRAMMAR.natural, "kw2"],
  [GRAMMAR.keywords, "kw"],
]) {
  for (const w of list) CLASS_OF_WORD.set(w, cls);
}
// Extra natural-syntax words the grammar does not list yet.
for (const w of ["otherwise", "nah", "else", "None", "Some", "Ok", "Err", "it"]) {
  if (!CLASS_OF_WORD.has(w)) CLASS_OF_WORD.set(w, w[0] === w[0].toUpperCase() ? "type" : "kw");
}
const MODULES = new Set(GRAMMAR.modules);

const OPERATORS = ["|>", "=>", "->", "==", "!=", "<=", ">=", "&&", "||", "...", ">>", "+=", "-=", "*=", "/="];

/**
 * Split `src` into tokens: `{ start, end, cls }` with `cls` one of
 * comment, str, esc, interp, num, kw, kw2, fn, type, const, mod, deco,
 * op, call or "" (plain). Offsets are UTF-16 indices into `src`.
 */
export function tokenize(src) {
  const out = [];
  const n = src.length;
  let i = 0;
  const push = (start, end, cls) => {
    if (end > start) out.push({ start, end, cls });
  };
  const isIdStart = (c) => /[A-Za-z_]/.test(c);
  const isId = (c) => /[A-Za-z0-9_]/.test(c);

  function string(quote) {
    // `quote` is `"` or `"""`. Handles escapes and {interpolation}.
    const begin = i;
    let segStart = i;
    i += quote.length;
    while (i < n) {
      if (quote === '"' && src[i] === "\n") break;
      if (src.startsWith(quote, i)) {
        i += quote.length;
        break;
      }
      if (src[i] === "\\" && i + 1 < n) {
        push(segStart, i, "str");
        push(i, i + 2, "esc");
        i += 2;
        segStart = i;
        continue;
      }
      if (src[i] === "{") {
        push(segStart, i, "str");
        let depth = 0;
        const s = i;
        while (i < n && src[i] !== "\n") {
          if (src[i] === "{") depth++;
          else if (src[i] === "}" && --depth === 0) {
            i++;
            break;
          }
          i++;
        }
        push(s, i, "interp");
        segStart = i;
        continue;
      }
      i++;
    }
    push(segStart, i, "str");
    return begin;
  }

  while (i < n) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") {
      const end = src.indexOf("\n", i);
      push(i, end < 0 ? n : end, "comment");
      i = end < 0 ? n : end;
    } else if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      const stop = end < 0 ? n : end + 2;
      push(i, stop, "comment");
      i = stop;
    } else if (src.startsWith('"""', i)) {
      string('"""');
    } else if (c === '"') {
      string('"');
    } else if (/[0-9]/.test(c)) {
      const m = /^(0x[0-9a-fA-F]+|0b[01]+|0o[0-7]+|\d[\d_]*(\.\d+)?)/.exec(src.slice(i, i + 64));
      const len = m ? m[0].length : 1;
      push(i, i + len, "num");
      i += len;
    } else if (c === "@" && isIdStart(src[i + 1] || "")) {
      let j = i + 1;
      while (j < n && isId(src[j])) j++;
      push(i, j, "deco");
      i = j;
    } else if (isIdStart(c)) {
      let j = i;
      while (j < n && isId(src[j])) j++;
      const word = src.slice(i, j);
      let cls = CLASS_OF_WORD.get(word) || "";
      if (MODULES.has(word) && src[j] === ".") cls = "mod";
      else if (!cls && src[j] === "(") cls = "call";
      push(i, j, cls);
      i = j;
    } else {
      const op = OPERATORS.find((o) => src.startsWith(o, i));
      if (op) {
        push(i, i + op.length, "op");
        i += op.length;
      } else {
        push(i, i + 1, /[=<>!+\-*/%?]/.test(c) ? "op" : "");
        i++;
      }
    }
  }
  return out;
}

const ESCAPES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
export const escapeHtml = (s) => s.replace(/[&<>"]/g, (c) => ESCAPES[c]);

/**
 * Highlighted HTML for `src`. `marks` are `{ start, end, cls }` ranges
 * (diagnostics) layered over the tokens as extra classes.
 */
export function highlight(src, marks = []) {
  const tokens = tokenize(src);
  // Cut points: token boundaries plus mark boundaries.
  const cuts = new Set([0, src.length]);
  for (const t of tokens) {
    cuts.add(t.start);
    cuts.add(t.end);
  }
  for (const m of marks) {
    cuts.add(m.start);
    cuts.add(m.end);
  }
  const points = [...cuts].filter((p) => p >= 0 && p <= src.length).sort((a, b) => a - b);
  let html = "";
  let t = 0;
  for (let k = 0; k + 1 < points.length; k++) {
    const a = points[k];
    const b = points[k + 1];
    while (t < tokens.length && tokens[t].end <= a) t++;
    const tok = tokens[t] && tokens[t].start <= a && tokens[t].end >= b ? tokens[t].cls : "";
    const classes = [];
    if (tok) classes.push("t-" + tok);
    for (const m of marks) if (m.start <= a && m.end >= b) classes.push(m.cls);
    const text = escapeHtml(src.slice(a, b));
    html += classes.length ? `<span class="${classes.join(" ")}">${text}</span>` : text;
  }
  // A trailing newline needs a character after it to occupy a line.
  return html + "\n";
}

/** UTF-16 offset of 1-based `line`/`col` (col counted in code points). */
export function offsetOf(src, line, col) {
  let offset = 0;
  for (let l = 1; l < line; l++) {
    const nl = src.indexOf("\n", offset);
    if (nl < 0) return src.length;
    offset = nl + 1;
  }
  let c = 1;
  while (c < col && offset < src.length && src[offset] !== "\n") {
    const code = src.codePointAt(offset);
    offset += code > 0xffff ? 2 : 1;
    c++;
  }
  return offset;
}

/** Range to underline for a diagnostic: its span, or the word at its start. */
export function rangeOf(src, line, col, endLine, endCol) {
  if (!line) return null;
  const start = offsetOf(src, line, Math.max(col, 1));
  let end = endLine ? offsetOf(src, endLine, Math.max(endCol, 1)) : start;
  if (end <= start) {
    end = start;
    while (end < src.length && /[A-Za-z0-9_]/.test(src[end])) end++;
    if (end === start) {
      // Not on a word: underline to the end of the line (at least 1 char).
      const nl = src.indexOf("\n", start);
      end = nl < 0 ? src.length : nl;
      if (end === start) {
        return { start: Math.max(0, start - 1), end: start };
      }
    }
  }
  return { start, end };
}
