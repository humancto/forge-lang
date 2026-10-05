// Unit tests for the playground's DOM-free modules (highlighter, ANSI
// renderer, share links) and the starter code. Runs in Node:
//   node --test bindings/wasm/tests/*.mjs

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const playground = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "docs", "playground");
const { tokenize, highlight, offsetOf, rangeOf } = await import(join(playground, "highlight.js"));
const { ansiToHtml } = await import(join(playground, "ansi.js"));
const { encodeShare, decodeShare } = await import(join(playground, "share.js"));

test("tokenizer classifies Forge source", () => {
  const src = 'set x to 42 // hi\nsay "a {x} \\n" + math.sqrt(4)\n@test\nfn f() { return true }';
  const cls = (text) => {
    const at = src.indexOf(text);
    const t = tokenize(src).find((t) => t.start === at);
    return t && t.cls;
  };
  assert.equal(cls("set"), "kw2");
  assert.equal(cls("42"), "num");
  assert.equal(cls("// hi"), "comment");
  assert.equal(cls("{x}"), "interp");
  assert.equal(cls("\\n"), "esc");
  assert.equal(cls("math"), "mod");
  assert.equal(cls("@test"), "deco");
  assert.equal(cls("fn"), "kw");
  assert.equal(cls("true"), "const");
  // Tokens are ordered, non-overlapping and inside the source.
  let end = 0;
  for (const t of tokenize(src)) {
    assert.ok(t.start >= end && t.end > t.start && t.end <= src.length);
    end = t.end;
  }
});

test("highlight escapes HTML and layers marks", () => {
  const html = highlight('say "<b>"', [{ start: 0, end: 3, cls: "mark-error" }]);
  assert.ok(!html.includes("<b>"));
  assert.match(html, /class="t-kw2 mark-error">say</);
  // Unterminated constructs never throw.
  highlight('say "unterminated {x\n/* open comment');
});

test("diagnostic positions map to offsets", () => {
  const src = "let a = 1\nlet é = foo\n";
  assert.equal(offsetOf(src, 2, 1), 10);
  assert.equal(offsetOf(src, 2, 9), 18);
  assert.deepEqual(rangeOf(src, 2, 9, 2, 9), { start: 18, end: 21 }); // the word "foo"
  assert.equal(rangeOf(src, 0, 0), null);
});

test("ANSI colors become spans; other escapes are dropped", () => {
  assert.equal(ansiToHtml("\x1b[31mred\x1b[0m ok"), '<span class="a-fg-red">red</span> ok');
  assert.equal(ansiToHtml("\x1b[1;32mA\x1b[22mB"), '<span class="a-bold a-fg-green">A</span><span class="a-fg-green">B</span>');
  assert.equal(ansiToHtml("\x1b[2J\x1b[1;1Hx<y"), "x&lt;y");
});

test("share links round-trip", async () => {
  const src = 'say "héllo 👋"\n'.repeat(50);
  const fragment = await encodeShare(src, "interp");
  assert.match(fragment, /^code=[A-Za-z0-9_-]+&engine=interp$/);
  assert.ok(fragment.length < src.length, "compressed");
  assert.deepEqual(await decodeShare("#" + fragment), { source: src, engine: "interp" });
  assert.deepEqual(await decodeShare("#tour=3"), { tour: 3 });
  assert.deepEqual(await decodeShare("#code=%%%"), {});
  const plain = Buffer.from("say 1").toString("base64url");
  assert.deepEqual(await decodeShare("#src=" + plain), { source: "say 1" });
});

test("the starter program runs", async () => {
  const js = readFileSync(join(playground, "playground.js"), "utf8");
  const code = /const DEFAULT_CODE = `([\s\S]*?)`;/.exec(js)[1];
  const { load } = await import(join(playground, "forge.js"));
  const forge = await load(readFileSync(join(playground, "pkg", "forge_wasm_bg.wasm")));
  const r = forge.run(code);
  assert.equal(r.ok, true, JSON.stringify(r.error));
  assert.match(r.stdout, /^Hello, World!\n1\n2\nFizz\n4\nBuzz\n/);
  assert.match(r.stdout, /FizzBuzz\n$/);
});
