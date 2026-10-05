// Headless smoke test of the browser build: loads the generated module
// through the playground's public JS API (docs/playground/forge.js) in
// Node, exactly as the Web Worker does, and checks programs end to end.
//
//   bindings/wasm/build.sh && node --test bindings/wasm/tests/*.mjs
//
// Tests run in order; the last one deliberately crashes the instance.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const playground = join(repo, "docs", "playground");
const { load } = await import(join(playground, "forge.js"));
const { TOUR } = await import(join(playground, "tour.js"));
const { EXAMPLES, GRAMMAR } = await import(join(playground, "generated.js"));

const forge = await load(readFileSync(join(playground, "pkg", "forge_wasm_bg.wasm")));
const ENGINES = ["auto", "vm", "interp"];

test("version matches the crate", () => {
  const cargo = readFileSync(join(repo, "Cargo.toml"), "utf8");
  const version = /^version = "([^"]+)"/m.exec(cargo)[1];
  assert.equal(forge.version(), version);
});

test("hello world on every engine", () => {
  for (const engine of ENGINES) {
    const r = forge.run('say "hello"\nyell "hi"', { engine });
    assert.equal(r.ok, true, `${engine}: ${JSON.stringify(r.error)}`);
    assert.equal(r.stdout, "hello\nHI\n");
    assert.equal(r.engine, engine === "interp" ? "interp" : "vm");
  }
});

test("clock, randomness and uuid work in wasm", () => {
  const r = forge.run(
    'say time.unix() > 1700000000\nsay len(uuid())\nsay math.random_int(5, 5)\nlet t = cook(fn() { return 1 })\nwait 0.01 seconds\nsay "done"',
  );
  assert.equal(r.ok, true, JSON.stringify(r.error));
  assert.equal(r.stdout, "true\n36\n5\ndone\n");
  assert.match(r.stderr, /COOKED/);
});

test("errors carry kind and position", () => {
  const syntax = forge.run("say 1\nlet = 2");
  assert.equal(syntax.error.kind, "syntax");
  assert.equal(syntax.error.line, 2);
  for (const engine of ENGINES) {
    const r = forge.run('say "a"\nlet x = 1 / 0', { engine });
    assert.equal(r.error.kind, "runtime");
    assert.equal(r.error.line, 2, engine);
    assert.equal(r.stdout, "a\n");
  }
});

test("infinite loops stop at the fuel limit, uncatchably", () => {
  for (const engine of ENGINES) {
    const r = forge.run("while true { }", { engine, maxInstructions: 2_000_000, maxSteps: 200_000 });
    assert.equal(r.error.kind, "limit", engine);
    assert.match(r.error.message, /fuel exhausted/);
    const caught = forge.run("while true { try { while true { } } catch e { say 1 } }", { engine, maxInstructions: 200_000, maxSteps: 20_000 });
    assert.equal(caught.error.kind, "limit", engine);
  }
});

test("channels work single-threaded; a receive that could never complete fails", () => {
  for (const engine of ENGINES) {
    const r = forge.run("let ch = channel()\nsend(ch, 5)\nsay receive(ch)", { engine });
    assert.equal(r.stdout, "5\n", engine);
  }
  for (const engine of ["vm", "interp"]) {
    const r = forge.run("let ch = channel()\nsay receive(ch)", { engine });
    assert.match(r.error.message, /would wait forever/, engine);
  }
});

test("output is capped", () => {
  const r = forge.run('repeat 1000 times { say "0123456789" }', { maxOutput: 55 });
  assert.equal(r.truncated, true);
  assert.equal(r.stdout.length, 55);
});

test("host capabilities fail clearly", () => {
  for (const src of ['http.get("https://example.com")', 'fs.read("x")', 'sh("ls")', 'db.open(":memory:")']) {
    const r = forge.run(src);
    assert.match(r.error.message, /not available in the browser playground/, src);
  }
  const spawn = forge.run("say 1\nlet h = spawn { return 1 }");
  assert.equal(spawn.error.kind, "unsupported");
  assert.equal(spawn.error.line, 2);
});

test("check and format", () => {
  assert.deepEqual(forge.check("let x = 1\nsay x"), []);
  const [d] = forge.check('fn f(a: Int) -> Int { return a }\nf("x")');
  assert.equal(d.line, 2);
  assert.match(d.code, /^T\d{4}$/);
  assert.equal(forge.format("let   x =  1\n"), "let x = 1\n");
});

test("every tour lesson runs on both engines", () => {
  assert.ok(TOUR.length >= 10 && TOUR.length <= 15, `tour has ${TOUR.length} lessons`);
  for (const lesson of TOUR) {
    for (const engine of ["vm", "interp"]) {
      const r = forge.run(lesson.code, { engine });
      assert.equal(r.ok, true, `${lesson.title} (${engine}): ${JSON.stringify(r.error)}`);
      if (lesson.expect !== undefined) {
        assert.equal(r.stdout, lesson.expect, `${lesson.title} (${engine})`);
      }
    }
  }
});

test("generated examples match examples/ and still run", () => {
  assert.ok(EXAMPLES.length > 0);
  for (const ex of EXAMPLES) {
    const onDisk = readFileSync(join(repo, "examples", ex.file), "utf8");
    assert.equal(ex.code, onDisk, `${ex.file} changed: regenerate docs/playground/generated.js`);
    const r = forge.run(ex.code);
    assert.equal(r.ok, true, `${ex.file}: ${JSON.stringify(r.error)}`);
  }
  assert.ok(GRAMMAR.keywords.includes("fn"));
  assert.ok(GRAMMAR.natural.includes("say"));
});

test("deep callback recursion fails cleanly (last: may poison the instance)", () => {
  // Recursion through `map` callbacks uses much more JS stack per Forge
  // call than plain recursion. Either the depth limit reports it, or the
  // JS stack overflows first and forge.js reports a crash; never a hang
  // or an uncaught exception.
  for (const engine of ["vm", "interp"]) {
    const src =
      "fn down(n) { if n == 0 { return 0 } return map([n], fn(x) { return down(x - 1) + 1 })[0] }\nsay down(5000)";
    const r = forge.run(src, { engine });
    assert.equal(r.ok, false, engine);
    assert.match(r.error.message, /recursion/, engine);
    if (r.crashed) {
      assert.throws(() => forge.run("say 1"), /crashed/);
      return;
    }
  }
});
