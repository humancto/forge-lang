// The Forge playground page: editor, Run, output, problems, share links,
// examples and the guided tour. The language itself runs in Web Workers
// (client.js -> worker.js -> forge.js -> WebAssembly).

import { ForgeClient } from "./client.js";
import { Editor } from "./editor.js";
import { ansiToHtml } from "./ansi.js";
import { escapeHtml } from "./highlight.js";
import { encodeShare, decodeShare } from "./share.js";
import { EXAMPLES } from "./generated.js";
import { TOUR } from "./tour.js";

const $ = (sel) => document.querySelector(sel);
const DRAFT_KEY = "forge-playground:draft";
const THEME_KEY = "forge-playground:theme";
const TOUR_KEY = "forge-playground:tour";

const storage = {
  get(key) {
    try {
      return localStorage.getItem(key);
    } catch (_) {
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (_) {
      /* private mode: drafts are a convenience only */
    }
  },
};

const DEFAULT_CODE = `// Welcome to the Forge playground!
// Press Run (Ctrl/⌘ + Enter). Take the tour for a guided start.

set name to "World"
say "Hello, {name}!"

define fizzbuzz(n) {
    return when n % 15 {
        == 0 -> "FizzBuzz",
        else -> when n % 3 {
            == 0 -> "Fizz",
            else -> when n % 5 { == 0 -> "Buzz", else -> str(n) }
        }
    }
}

for i in range(1, 16) {
    say fizzbuzz(i)
}
`;

// ---------------------------------------------------------------- state

let client = null;
let running = false;
let checkTimer = 0;
let checkSeq = 0;
let tourIndex = -1; // -1 = tour closed
let lastRunMarks = [];

const editor = new Editor($("#editor"), {
  label: "Forge source code",
  onChange: (code) => {
    lastRunMarks = [];
    storage.set(DRAFT_KEY, code);
    scheduleCheck();
  },
});

// ---------------------------------------------------------------- theme

function applyTheme(theme) {
  if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
  const dark =
    theme === "dark" || (theme !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  $("#theme").setAttribute("aria-pressed", String(dark));
  $("#theme").title = dark ? "Switch to light theme" : "Switch to dark theme";
}
$("#theme").addEventListener("click", () => {
  const dark = $("#theme").getAttribute("aria-pressed") === "true";
  const next = dark ? "light" : "dark";
  storage.set(THEME_KEY, next);
  applyTheme(next);
});
applyTheme(storage.get(THEME_KEY));

// --------------------------------------------------------------- status

function setStatus(text, busy = false) {
  $("#status").textContent = text;
  $("#status").classList.toggle("busy", busy);
}

let toastTimer = 0;
function toast(text) {
  const el = $("#toast");
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (el.hidden = true), 2500);
}

// ------------------------------------------------------------ problems

function renderProblems(diags) {
  const list = $("#problems-list");
  const errors = diags.filter((d) => d.severity === "error").length;
  const warnings = diags.length - errors;
  $("#problems-count").textContent = diags.length ? `${errors} error(s), ${warnings} warning(s)` : "none";
  if (!diags.length) {
    list.innerHTML = `<li class="empty">No problems found.</li>`;
    return;
  }
  list.innerHTML = diags
    .map(
      (d, i) => `<li><button type="button" class="problem ${d.severity}" data-i="${i}">
        <span class="sev">${d.severity}</span>
        <span class="loc">${d.line}:${d.col}</span>
        <span class="msg">${escapeHtml(d.message)}${d.help ? `<span class="help">help: ${escapeHtml(d.help)}</span>` : ""}</span>
        ${d.code ? `<span class="code">${escapeHtml(d.code)}</span>` : ""}
      </button></li>`,
    )
    .join("");
  list.querySelectorAll("button").forEach((b) =>
    b.addEventListener("click", () => {
      const d = diags[Number(b.dataset.i)];
      editor.goTo(d.line, d.col);
    }),
  );
}

function scheduleCheck() {
  clearTimeout(checkTimer);
  checkTimer = setTimeout(runCheck, 350);
}

async function runCheck() {
  if (!client) return;
  const seq = ++checkSeq;
  const code = editor.value;
  try {
    const raw = await client.check(code);
    if (seq !== checkSeq || code !== editor.value) return;
    const diags = raw.map((d) => ({
      severity: d.severity,
      message: d.message,
      help: d.help,
      code: d.code,
      line: d.line,
      col: d.col,
      endLine: d.end_line,
      endCol: d.end_col,
    }));
    renderProblems(diags);
    editor.setMarks([...diags, ...lastRunMarks]);
  } catch (err) {
    console.warn("check failed:", err);
  }
}

// --------------------------------------------------------------- output

function renderOutput(result) {
  const out = $("#output");
  let html = result.output
    .map((c) => `<span class="${c.stream}">${ansiToHtml(c.text)}</span>`)
    .join("");
  if (result.truncated) html += `<span class="note">… output truncated</span>\n`;
  const e = result.error;
  if (e) {
    const where = e.line ? `line ${e.line}${e.col ? ":" + e.col : ""}` : "";
    const label = { syntax: "Syntax error", unsupported: "Not available here", compile: "Compile error", limit: "Stopped", runtime: "Error" }[e.kind] || "Error";
    html += `<div class="run-error kind-${e.kind}" role="alert">
      <strong>${label}</strong>${where ? ` <button type="button" class="goto" data-line="${e.line}" data-col="${e.col || 1}">${where}</button>` : ""}
      <pre>${escapeHtml(e.message)}</pre></div>`;
  }
  if (!html) html = `<span class="note">(no output)</span>`;
  out.innerHTML = html;
  out.querySelectorAll(".goto").forEach((b) =>
    b.addEventListener("click", () => editor.goTo(Number(b.dataset.line), Number(b.dataset.col))),
  );
  out.scrollTop = 0;
}

// ------------------------------------------------------------------ run

async function run() {
  if (!client) return;
  if (running) {
    client.stop();
    return;
  }
  running = true;
  $("#run").textContent = "Stop";
  $("#run").setAttribute("aria-label", "Stop the running program");
  setStatus("Running…", true);
  $("#output").setAttribute("aria-busy", "true");
  const engine = $("#engine").value;
  try {
    const result = await client.run(editor.value, { engine });
    renderOutput(result);
    lastRunMarks = [];
    if (result.error && result.error.line) {
      lastRunMarks = [{ severity: "error", message: result.error.message, line: result.error.line, col: result.error.col || 1 }];
      editor.setMarks(lastRunMarks);
      runCheck();
    }
    const engineName = result.engine === "interp" ? "interpreter" : result.engine === "vm" ? "VM" : "";
    const time = result.elapsed_ms ? ` in ${result.elapsed_ms < 10 ? result.elapsed_ms.toFixed(1) : Math.round(result.elapsed_ms)} ms` : "";
    setStatus(result.ok ? `Finished${time}${engineName ? " on the " + engineName : ""}` : result.stopped ? "Stopped" : `Failed${time}`);
    if (tourIndex >= 0) markLessonRun(result.ok);
  } catch (err) {
    renderOutput({ output: [], error: { kind: "runtime", message: String(err.message || err) } });
    setStatus("Failed");
  } finally {
    running = false;
    $("#run").textContent = "Run";
    $("#run").setAttribute("aria-label", "Run the program (Ctrl or Command + Enter)");
    $("#output").setAttribute("aria-busy", "false");
  }
}

$("#run").addEventListener("click", run);
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
    e.preventDefault();
    run();
  } else if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "F" || e.key === "f")) {
    e.preventDefault();
    format();
  }
});

// --------------------------------------------------------------- format

async function format() {
  if (!client) return;
  const before = editor.value;
  const after = await client.format(before);
  if (after !== before && editor.value === before) {
    editor.input.focus();
    editor.input.select();
    // Keep undo: replace through the editing pipeline.
    if (!(document.execCommand && document.execCommand("insertText", false, after))) editor.value = after;
    editor.input.setSelectionRange(0, 0);
    toast("Formatted");
  } else {
    toast("Already formatted");
  }
}
$("#format").addEventListener("click", format);

// ---------------------------------------------------------------- share

$("#share").addEventListener("click", async () => {
  const fragment = await encodeShare(editor.value, $("#engine").value);
  const url = `${location.origin}${location.pathname}#${fragment}`;
  history.replaceState(null, "", "#" + fragment);
  try {
    await navigator.clipboard.writeText(url);
    toast("Link copied to the clipboard");
  } catch (_) {
    prompt("Copy this link:", url);
  }
});

// ------------------------------------------------------------- examples

const examples = $("#examples");
for (const [i, ex] of EXAMPLES.entries()) {
  const opt = document.createElement("option");
  opt.value = String(i);
  opt.textContent = ex.title.length > 48 ? ex.file : ex.title;
  examples.appendChild(opt);
}
examples.addEventListener("change", () => {
  const ex = EXAMPLES[Number(examples.value)];
  if (ex) {
    closeTour();
    loadCode(ex.code);
    setStatus(`Loaded examples/${ex.file}`);
  }
  examples.value = "";
});

function loadCode(code) {
  editor.value = code;
  storage.set(DRAFT_KEY, code);
  lastRunMarks = [];
  $("#output").innerHTML = `<span class="note">Press Run to see the output.</span>`;
  if (client) setStatus("Ready");
  scheduleCheck();
}

// ----------------------------------------------------------------- tour

function openLesson(index) {
  tourIndex = Math.max(0, Math.min(TOUR.length - 1, index));
  const lesson = TOUR[tourIndex];
  document.body.classList.add("touring");
  $("#tour").hidden = false;
  $("#tour-toggle").setAttribute("aria-expanded", "true");
  $("#tour-progress").textContent = `Lesson ${tourIndex + 1} of ${TOUR.length}`;
  $("#tour-title").textContent = lesson.title;
  $("#tour-body").innerHTML = lesson.body;
  $("#tour-prev").disabled = tourIndex === 0;
  $("#tour-next").textContent = tourIndex === TOUR.length - 1 ? "Finish" : "Next →";
  $("#tour-done").hidden = true;
  $("#tour-bar").style.width = `${((tourIndex + 1) / TOUR.length) * 100}%`;
  storage.set(TOUR_KEY, String(tourIndex));
  history.replaceState(null, "", `#tour=${tourIndex + 1}`);
  loadCode(lesson.code);
  $("#tour-title").focus();
}

function markLessonRun(ok) {
  $("#tour-done").hidden = !ok;
}

function closeTour() {
  if (tourIndex < 0) return;
  tourIndex = -1;
  document.body.classList.remove("touring");
  $("#tour").hidden = true;
  $("#tour-toggle").setAttribute("aria-expanded", "false");
  if (location.hash.startsWith("#tour")) history.replaceState(null, "", location.pathname);
}

$("#tour-toggle").addEventListener("click", () => {
  if (tourIndex >= 0) closeTour();
  else openLesson(Number(storage.get(TOUR_KEY) || 0));
});
$("#tour-close").addEventListener("click", closeTour);
$("#tour-prev").addEventListener("click", () => openLesson(tourIndex - 1));
$("#tour-next").addEventListener("click", () => {
  if (tourIndex === TOUR.length - 1) {
    closeTour();
    storage.set(TOUR_KEY, "0");
    toast("Tour complete — happy forging!");
  } else {
    openLesson(tourIndex + 1);
  }
});
$("#tour-reset").addEventListener("click", () => loadCode(TOUR[tourIndex].code));

// ----------------------------------------------------------------- boot

async function boot() {
  const shared = await decodeShare(location.hash);
  if (shared.engine) $("#engine").value = shared.engine;
  if (shared.tour) openLesson(shared.tour - 1);
  else if (shared.source !== undefined) loadCode(shared.source);
  else loadCode(storage.get(DRAFT_KEY) || DEFAULT_CODE);

  setStatus("Loading Forge…", true);
  try {
    client = await ForgeClient.create();
  } catch (err) {
    setStatus("Could not load Forge");
    $("#output").innerHTML = `<div class="run-error" role="alert"><strong>Could not load the Forge WebAssembly module.</strong>
      <pre>${escapeHtml(String(err.message || err))}

If you are running the playground locally, build it first:
  bindings/wasm/build.sh
and serve docs/ over HTTP (module workers do not load from file://).</pre></div>`;
    return;
  }
  $("#version").textContent = `Forge ${client.version}`;
  $("#run").disabled = false;
  $("#format").disabled = false;
  setStatus("Ready");
  runCheck();
}

boot();
