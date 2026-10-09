// A dependency-free code editor: a real <textarea> (native editing, undo,
// IME, screen readers, mobile keyboards) laid exactly over a highlighted
// <pre>. The textarea grows to fit its content, so the surrounding scroller
// moves both layers together and no scroll syncing is needed.
//
// Keys: Tab / Shift+Tab indent and outdent (press Esc first to move focus
// out with Tab instead), Enter keeps the indentation (and adds a level
// after "{"), Ctrl/⌘+Enter runs (handled by the page).

import { highlight, rangeOf } from "./highlight.js";

const INDENT = "    ";

export class Editor {
  constructor(root, { onChange, label } = {}) {
    this.onChange = onChange || (() => {});
    this.marks = [];
    this.tabEscapes = false;

    root.classList.add("editor");
    root.innerHTML = `
      <div class="editor-gutter" aria-hidden="true"></div>
      <div class="editor-code">
        <pre class="editor-highlight" aria-hidden="true"></pre>
        <textarea class="editor-input" spellcheck="false" autocapitalize="off"
          autocomplete="off" autocorrect="off" wrap="off"></textarea>
      </div>`;
    this.root = root;
    this.gutter = root.querySelector(".editor-gutter");
    this.pre = root.querySelector(".editor-highlight");
    this.input = root.querySelector(".editor-input");
    this.input.setAttribute("aria-label", label || "Code editor");
    this.input.setAttribute("aria-describedby", "editor-help");

    this.input.addEventListener("input", () => {
      this.marks = [];
      this.render();
      this.onChange(this.value);
    });
    this.input.addEventListener("keydown", (e) => this.#onKey(e));
    // Escape releases Tab for focus navigation until the next edit/click.
    this.input.addEventListener("blur", () => (this.tabEscapes = false));
    this.input.addEventListener("mousedown", () => (this.tabEscapes = false));
    new ResizeObserver(() => this.#fit()).observe(root);
  }

  get value() {
    return this.input.value;
  }

  /** Replace the whole text (resets undo history). */
  set value(text) {
    this.input.value = text;
    this.marks = [];
    this.render();
  }

  focus() {
    this.input.focus();
  }

  /**
   * Underline ranges: `[{ line, col, endLine, endCol, severity, message }]`
   * (1-based). Cleared on the next edit.
   */
  setMarks(diagnostics) {
    const src = this.value;
    this.marks = [];
    for (const d of diagnostics) {
      const r = rangeOf(src, d.line, d.col, d.endLine, d.endCol);
      if (r) this.marks.push({ ...r, cls: "mark-" + d.severity, line: d.line, message: d.message });
    }
    this.render();
  }

  /** Move the caret to the start of `line` (1-based) and focus it. */
  goTo(line, col = 1) {
    const lines = this.value.split("\n");
    let offset = 0;
    for (let l = 1; l < line && l <= lines.length; l++) offset += lines[l - 1].length + 1;
    offset += Math.max(0, col - 1);
    this.input.focus();
    this.input.setSelectionRange(offset, offset);
    const lineHeight = parseFloat(getComputedStyle(this.input).lineHeight) || 20;
    const scroller = this.root;
    const top = (line - 1) * lineHeight;
    if (top < scroller.scrollTop || top > scroller.scrollTop + scroller.clientHeight - lineHeight * 2) {
      scroller.scrollTop = Math.max(0, top - scroller.clientHeight / 3);
    }
  }

  render() {
    const src = this.value;
    this.pre.innerHTML = highlight(src, this.marks);
    const count = src.split("\n").length;
    const flagged = new Map();
    for (const m of this.marks) {
      const prev = flagged.get(m.line);
      if (!prev || m.cls === "mark-error") flagged.set(m.line, m);
    }
    let gutter = "";
    for (let l = 1; l <= count; l++) {
      const m = flagged.get(l);
      gutter += m
        ? `<div class="${m.cls}" title="${m.message.replace(/"/g, "&quot;")}">${l}</div>`
        : `<div>${l}</div>`;
    }
    this.gutter.innerHTML = gutter;
    this.#fit();
  }

  #fit() {
    const t = this.input;
    t.style.height = "0px";
    t.style.width = "0px";
    t.style.height = Math.max(t.scrollHeight, this.pre.scrollHeight) + "px";
    t.style.width = Math.max(t.scrollWidth, this.pre.scrollWidth) + "px";
  }

  /** Insert text at the selection, keeping the native undo stack. */
  #insert(text) {
    this.input.focus();
    const ok = document.execCommand && document.execCommand("insertText", false, text);
    if (!ok) {
      this.input.setRangeText(text, this.input.selectionStart, this.input.selectionEnd, "end");
      this.input.dispatchEvent(new Event("input"));
    }
  }

  #onKey(e) {
    const t = this.input;
    if (e.key === "Escape") {
      this.tabEscapes = true;
      return;
    }
    if (e.key === "Tab" && !this.tabEscapes && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      const { selectionStart: s, selectionEnd: end, value } = t;
      const multi = value.slice(s, end).includes("\n");
      if (!multi && !e.shiftKey) {
        this.#insert(INDENT);
        return;
      }
      // Indent or outdent every selected line.
      const lineStart = value.lastIndexOf("\n", s - 1) + 1;
      const block = value.slice(lineStart, end);
      const lines = block.split("\n");
      const changed = lines.map((l) =>
        e.shiftKey ? l.replace(/^( {1,4}|\t)/, "") : INDENT + l,
      );
      t.setSelectionRange(lineStart, end);
      this.#insert(changed.join("\n"));
      t.setSelectionRange(lineStart, lineStart + changed.join("\n").length);
      return;
    }
    if (e.key === "Enter" && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) {
      const { selectionStart: s, value } = t;
      const lineStart = value.lastIndexOf("\n", s - 1) + 1;
      const line = value.slice(lineStart, s);
      let indent = /^\s*/.exec(line)[0];
      if (/[{[(]\s*$/.test(line)) indent += INDENT;
      e.preventDefault();
      this.#insert("\n" + indent);
    }
  }
}
