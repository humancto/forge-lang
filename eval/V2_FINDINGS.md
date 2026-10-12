# Findings from writing eval tier v2

Writing the 50 tier-v2 reference solutions (`eval/tasks_v2/`) against Forge v0.11.0 turned
up the problems below. None of them was worked around silently: each reference uses an
approach that works on both engines, and a comment in the reference points at the trap it
avoids. Every repro was run with `forge` (VM) and `forge --interp`, in editions 2026 and
2027, unless a section says otherwise.

Severity: **bug** (wrong result, crash, or engine divergence), **perf** (a performance
cliff that makes natural code time out), **gap** (missing feature or undocumented behaviour
that a model cannot guess from `llms.txt`).

| # | Severity | Finding | Tasks it affects |
|---|---|---|---|
| 1 | bug | `xs.push(v)` / `xs.pop()` on a nested place (`o.a.push(x)`, `g[k].push(x)`, `m[0].push(x)`) silently does nothing | data, any grouping code |
| 2 | bug (fixed) | Interpreter: `import { Struct } from "m.fg"` loses the struct's `impl` methods | money_module, any selective import of a type |
| 3 | bug (fixed) | `try_receive` returns `Ok(v)`/`null` on the VM but `Some(v)`/`None` on the interpreter | parallel |
| 4 | bug (fixed) | VM, edition 2027: a parameter declared `[fn(Int) -> Int]` rejects an array of functions at run time | typed |
| 5 | bug (fixed) | A `Float` annotation does not convert an `Int`: `fn half(x: Float) -> Float { return x / 2 }` gives `half(3) == 1` | typed_order_decode, shape_interface |
| 6 | bug (fixed) | `csv.parse` splits quoted fields that contain line breaks, trims quoted text and turns `"02134"` into `2134` | csv_quoted_fields, sales_report_api |
| 7 | bug (fixed) | `forge check` does not report an import it cannot resolve; the imported names then resolve to builtins of the same name | multifile (harness fixed) |
| 8 | bug (fixed) | The VM reports a runtime error inside a function at the caller's line (`--error-format json` too) | every task's repair round |
| 9 | bug (fixed) | The E0013 hint tells edition-2027 users to call `sus(x)`, which 2027 removes | all |
| 10 | perf | VM: `obj[key] = v` copies the whole object; building an object with n keys is O(n^2) time and memory (OOM at 20,000 keys) | data (latest_record_wins, word_freq_large, ...) |
| 11 | perf | VM: `xs.push({ ...object literal... })` is O(n^2) (50,000 pushes: 6 s, OOM inside a function) | data |
| 12 | perf | Interpreter: `get(obj, k)` and `has_key(obj, k)` copy the whole object per call (O(n) each) | data |
| 13 | perf | `set`/`map` are linear lists: `st.has`, `st.add`, `m.get`, `m.set` are O(n) per call on both engines | data |
| 14 | perf | Net effect: the textbook word count over 100,000 words takes 28 s (VM) / 30 s (interp); Python takes 0.03 s | word_freq_large |
| 15 | gap (fixed) | HTTP handlers cannot set a status code or response headers, and cannot read request headers | servers |
| 16 | gap (fixed) | A database opened at the top level of a server is not visible in handlers ("no database connection open") | todo_sqlite |
| 17 | gap (fixed) | `import { a as b }` does not exist and is misparsed ("does not export 'as'"); importing a module twice runs it twice | clashing_modules |
| 18 | gap (fixed) | Request bodies must be JSON with `Content-Type: application/json`; otherwise axum answers 400/415 in text/plain | servers |
| 19 | gap (fixed) | `llms.txt` describes import resolution incorrectly | multifile |
| 20 | gap | `regex` has no capture groups | logfmt_parse, ini_config_check |
| 21 | gap | Undocumented: `select` result shape, `exit(code)`, negative indexes, `Map` methods, `time.*` signatures, no `? :` operator, no array patterns in `match`, `typeof` names | many |
| 22 | gap | `Map<String, T>` (suggested for objects by the MCP section) does not accept an object literal; there is no typed dictionary for objects | typed |
| 23 | gap | HTTP client: a timeout and a refused connection raise the same message; `timeout` units are undocumented | http_client |
| 24 | gap | `e.message` of some caught errors contains a `\n  hint: ...` line | error_handling |
| 25 | gap | No way to write to stderr except `log.*` (structured tracing) | error_handling |
| 26 | bug (fixed) | `forge check`: a module-level variable named like a builtin (`let mut count = 0`) is typed as the builtin inside the module's functions (false T0005 in the importer) | multifile |

---

## Bugs

### 1. Method calls on a nested place are silently dropped

What I tried (log_latency_percentiles): group requests per endpoint with
`groups[endpoint].push(req)`. Every group stayed at one element; no error, no warning.

```forge
let mut g = { a: [1] }
g["a"].push(2)
g.a.push(3)
let k = "a"
g[k].push(4)
println(g)              // { "a": [1] }   expected [1, 2, 3, 4]
let mut arr = [[1], [2]]
arr[0].push(9)
println(arr)            // [[1], [2]]
let mut o = { xs: [[1], [2]] }
o.xs.pop()
println(o)              // { "xs": [[1], [2]] }
```

Both engines, both editions. Plain assignments to the same places work
(`g.a = g.a + [2]`, `o[k].n = 5`, `o.xs[0][0] = 9`), and `xs.push(v)` on a local works,
which makes the nested case look like it should work. `llms.txt` says "`xs.push(4)` —
append in place".

Suggested fix: compile `place.push(v)` / `place.pop()` (and other in-place methods) on a
field/index place like the assignment forms (`compile_store` of the updated value), or
reject them (a T-code in the checker and a runtime error), never ignore them.

Reference workaround: `group_by` (one pass, insertion-ordered) or `g[k] = g[k] + [v]`.

### 2. Interpreter: selectively imported structs lose their methods

**Fixed:** a selective import (`import { P } from "m.fg"`) brings the module's method tables, static methods, struct defaults and embedded fields with it on the interpreter too (all of them, like the VM, which registers them when the module runs). Parity fixture `tests/parity/supported/selective_import_methods.fg`.

```forge
// shapes.fg
struct P { x: Int }
impl P {
    fn double(it) { return it.x * 2 }
}
```
```forge
// main.fg
import { P } from "shapes.fg"
let p = P { x: 21 }
println(p.double())
```

VM: `42`. Interpreter: `[E0013] no method 'double' on P`. Same for an algebraic type
(`import { A } from "adt.fg"` then `A(1).show()`). `import "shapes.fg"` works on both.

Suggested fix: when a selective import brings in a struct or type name (or a variant
constructor), also bring in its method table (`impl` blocks, interface impls), as the VM
does. Add a parity fixture.

Reference workaround (money_module): `import "money.fg"`.

### 3. `try_receive` returns different types on the two engines

**Fixed:** `try_receive` returns `Some(v)` / `None` on both engines (the VM returned `Ok(v)` / null), as `docs/part2_stdlib.md` already documented; `llms.txt` states it. Parity fixture `tests/parity/supported/try_receive_option.fg`.

```forge
let c = channel(3)
send(c, 5)
println(try_receive(c))   // VM: Ok(5)      interp: Some(5)
println(try_receive(c))   // VM: null       interp: None
```

A program that matches on `Some(v)` works only on the interpreter; one that matches on
`Ok(v)` only on the VM. Also on the VM `is_some(Ok(5))` is `true`.

Suggested fix: pick one (`Option` fits "maybe a value") in `builtins_registry`, use it on
both engines, add it to the engine-diff fixtures, and document it in `llms.txt` next to
`try_send`/`select`.

### 4. VM, edition 2027: `[fn(...) -> ...]` parameters fail their run-time check

**Fixed:** the VM decided `__types.check` on its own values only for a few shapes and converted everything else to interpreter values, where a closure becomes null, so `[fn(Int) -> Int]` failed. It now decides every check on VM values (`fits_vm` in `src/vm/builtins.rs`, the rule of `semantics::types::fits`), and a violation by a function says `got function` on both engines. Tests: `tests/edition2027/annotations_test.fg` (`function_values_fit_function_annotations`, both engines).

```forge
fn apply_all(fs: [fn(Int) -> Int], x: Int) -> [Int] {
    return map(fs, fn(f) { return f(x) })
}
println(apply_all([fn(n) { return n + 1 }], 5))
```

`forge --edition 2027 check`: clean. Interpreter (2026 and 2027) and VM 2026: `[6]`.
VM 2027: `[E0017] type error: argument 'fs' of 'apply_all' must be [function], got Array`.
A single `f: fn(Int) -> Int` parameter passes on both engines; the element check of
function arrays is what differs.

Suggested fix: make the VM's `__types.check` accept VM closures as `function` elements
(probably the element type name of a VM closure differs from the interpreter's); add a
2027 parity fixture with arrays of functions.

### 5. A `Float` annotation does not make a value a Float

**Fixed:** where a declared type is enforced (edition 2027 and `--strict`), an Int checked against a declared `Float` becomes a Float (`semantics::types::widen`, mirrored by the VM's `widen_vm` and the JIT's `kind_widens`): parameters (reassigned from their check), `-> Float` results, annotated `let`s and struct fields/defaults (`__types.widen`), nested `[Float]`/`?Float`/tuples/map values/`Result`. `half(3) == 1.5` and `typeof(half(3)) == "Float"` on both engines and in JIT code. Edition 2026 without `--strict` does not instrument and does not convert (documented in docs/editions/2027.md and llms.txt: write `float(x)`). `str(3.0)` is `"3"` on both engines (unchanged), so the difference shows in arithmetic and `typeof`. Tests: `tests/edition2027/annotations_test.fg` (`float_annotations_widen_ints`), `vm::jit_tests::jit_widens_ints_checked_against_float`.

```forge
fn half(x: Float) -> Float {
    return x / 2
}
println(half(3))          // 1      (Int / Int)
println(typeof(half(3)))  // Int
let y: Float = 7
println(y / 2)            // 3
struct P { x: Float }
let p = P { x: 3 }
println(p.x / 2)          // 1
```

All engines and editions. In 2027 the argument and return checks accept the Int
(`Int` is allowed where `Float` is declared) but the value stays an Int, so arithmetic in
a function that declares Float silently becomes integer division. This bites typed JSON
decoding: a JSON price of `5` stored in a `unit_price: Float` field, then halved by a
coupon, gives `2`, not `2.5`.

Suggested fix: when an Int flows into a binding, parameter, field or return declared
`Float` (statically or by the 2027 run-time check), convert it (`5` becomes `5.0`). The
checker already treats Int as assignable to Float, so the conversion is what the program
asked for. Until then `llms.txt` should warn: "write `float(x)` when a value may be an
Int".

Reference workaround (typed_order_decode, shape_interface): `float(...)` when building the
structs, with a comment.

### 6. `csv.parse` corrupts common CSV

**Fixed:** `csv.parse`/`csv.read` are a character-level RFC 4180 parser (`src/stdlib/csv.rs`, shared by both engines): quoted fields keep commas, `""`, LF/CRLF and whitespace; an unterminated quote is an error; only unquoted fields that are exactly an integer, a plain decimal or `true`/`false` are converted (`02134`, `+1`, `1e5`, quoted numbers stay strings). Options `{ header: false }` and `{ types: false }`; `csv.stringify` output parses back to the same rows. Tests: `stdlib::csv::tests`, `tests/csv_rfc4180_test.fg` (both engines).

```forge
// t.csv:
// name,qty,note,zip
// "Smith, J",3,"said ""hi""",02134
// Bob,x,"multi
// line",00501
println(csv.parse(fs.read("t.csv")))
```

Output (both engines):

```
[{ "name": "Smith, J", "qty": 3, "note": "said \"hi\"", "zip": 2134 },
 { "name": "Bob", "qty": "x", "note": "multi", "zip": "" },
 { "name": "line,00501", "qty": "", "note": "", "zip": "" }]
```

- The parser is line-based (`input.lines()`), so a quoted field with a line break becomes
  two broken records.
- Every field is trimmed, including quoted ones (`"  Ann  "` becomes `Ann`).
- Every field that parses as a number or `true`/`false` is converted: zip codes, SKUs and
  ids lose their leading zeros (`02134` becomes `2134`), with no way to opt out.

Suggested fix: a character-level RFC 4180 parser; keep quoted fields verbatim; make type
conversion opt-in (`csv.parse(text, { types: true })`) or at least never convert quoted
fields or values with leading zeros; offer a headerless mode returning arrays. Document
the behaviour in `llms.txt`.

Reference workaround: a hand-written parser (csv_quoted_fields) and `split` for simple CSV
(sales_report_api, department_join).

### 7. `forge check` ignores imports it cannot resolve

**Fixed:** an import the checker cannot find or parse is the new T0020 (`cannot import 'does_not_exist.fg': file not found (looked for /dir/does_not_exist.fg)`, with where it looks as the help), an error from edition 2027 and under `--strict` (exit 1 from `forge check`), a warning in 2026. Its names are unknown (`Any`) instead of builtins of the same name: no false T0004 for `title()`, also after an unreadable wildcard import. Not reported when imports are skipped (`Sandbox`, MCP). Tests: `typechecker::tests::unresolved_imports_are_reported_without_cascading`, the `forge explain T0020` example test.

```forge
import { nothing_here } from "does_not_exist.fg"
println(nothing_here(1))
```

`forge --edition 2027 check t.fg` prints `0 errors, 0 warnings` (exit 0); `forge run`
fails. Worse, when the import is not found the imported names fall back to builtins: a
module function `title()` imported this way was checked as the builtin `title(s)` and
reported `T0004 fn ... expects 1 argument, got 0`.

This also hit the harness: `validate` checked `reference.fg` in place, where the modules
from `files/` are not visible. The harness now checks a copy next to `files/`
(commit "eval: type-check references next to their task files").

Suggested fix: report an unresolved import as an error (T-code) in `forge check` and the
LSP, and never fall back to a builtin for an imported name.

### 8. VM error positions inside functions point at the caller

**Fixed:** `forge run` reports a VM runtime error at the innermost frame that has a position (the line inside the callee, as the interpreter and the `Sandbox` already did), in human and JSON output. The interpreter now records the call stack an error unwinds through (`RuntimeError::trace`, only on the error path) and prints it like the VM (`at pick (line 2)` ... `at <main> (line 10)`). One difference remains: the VM names `impl` methods `<lambda>` in the trace, the interpreter `P::bad`. Test: `tests/cli_diagnostics.rs::runtime_errors_inside_functions_point_at_the_callee` (both engines, JSON and human).

```forge
fn pick(xs, i) {
    let v = xs[i]
    return v
}

println(pick([1, 2], 5))
```

`forge --error-format json run`: VM `"line":6,"col":1`; interpreter `"line":2,"col":5`. The
VM's human output adds a stack trace (`at pick (line 2)`), but the JSON diagnostic (what
the eval's repair round and editors read) names the wrong line.

Suggested fix: use the innermost frame's position for the diagnostic on the VM, like the
interpreter, and keep the trace as extra context.

### 9. The E0013 hint suggests a builtin that edition 2027 removed

**Fixed:** hints are edition-neutral instead of edition-specific: the E0013 hint is `check the spelling; print the value with println(x) (or its type with typeof(x)) to see what it contains`, valid in both editions. Also fixed: every `forge explain` example (runtime and type-checker codes) uses `println(...)` instead of `say`, E0028's example uses `let {name} = 42` instead of `unpack`, explanations of 2026-only features say so (E0021 GenZ asserts, E0022 `check`, E0023 `must`), the REPL `help` examples, the LSP completions (removed keywords and builtins are not offered in edition 2027) and the MCP server instructions (`say`). Test: `semantics::errors::tests::hints_and_examples_do_not_suggest_removed_names`.

Every `no method ...` / `no field ...` error carries the hint
`check the spelling; print the value with sus(x) to see what it contains`, also under
`--edition 2027`, where `sus` is undefined.

Suggested fix: make the hint edition-aware (`println(x)` / `typeof(x)`), or drop the
`sus` mention.

### 26. Imported modules: module-level names typed as builtins

**Fixed:** names in an imported module's function bodies resolve in that module (`ForeignModule` in `src/typechecker/infer.rs`, using `semantics::hoist::module_binders`): its own top-level bindings shadow builtins, and the importing file's functions are not visible there. Test: `typechecker::tests::imported_module_names_shadow_builtins_in_its_bodies`.

```forge
// counter.fg
let mut count = 0
fn bump() {
    count = count + 1
    return count
}
```
```forge
import { bump } from "counter.fg"
let n = bump() + 1   // T0005: operator '+' cannot be applied to fn(Any, Any) -> Int and Int
```

The checker inferred `bump`'s return type with the importer's name resolution, where
`count` is the builtin `count(xs, x)`.

## Performance

Measured with the prebuilt v0.11.0 `forge` binary on the development container; times are for
the whole loop, "n" is the number of distinct keys or elements.

| Operation (loop of n steps) | n | VM | Interpreter |
|---|---:|---:|---:|
| `ob[k] = 1` (new keys) | 4,000 | 816 ms | 2 ms |
| `ob[k] = 1` (new keys) | 12,000 | 5,519 ms | 7 ms |
| `ob[k] = [k]` (new keys) | 12,000 | 5,739 ms | 9 ms |
| `get(ob, k, 0)` (n keys) | 12,000 | 5 ms | 9,486 ms |
| `has_key(ob, k)` (n keys) | 12,000 | 6 ms | 9,550 ms |
| `m = m.set(k, 1)` | 12,000 | 1,335 ms | 11,037 ms |
| `m.get(k)` | 12,000 | 1,220 ms | 528 ms |
| `set(array)` | 12,000 | 1,176 ms | 496 ms |
| `st.has(k)` | 12,000 | 1,214 ms | 492 ms |
| `st = st.add(k)` | 12,000 | 1,329 ms | 5,392 ms |
| `from_entries(pairs)` | 12,000 | 1,427 ms | 15 ms |
| `sb = sb + k + "\n"` | 12,000 | 708 ms | 57 ms |
| `xs.push({ a: i })` | 20,000 | 1,014 ms | 60 ms |
| `xs.push([i])` / `xs.push(i)` | 20,000 | 10 ms | ~10 ms |
| `frequencies`, `group_by`, `sort`, `join`, `map` | 12,000 | <= 3 ms | <= 15 ms |

### 10. VM: assigning a key copies the object

`SetIndex` returns an updated copy (value semantics, `CLAUDE.md` "VM collections are
values"), and for objects that copy is O(size) even when the object has a single owner.
Building an object with 20,000 distinct keys did not finish: the process was killed after
40 s and ~33 s of system time (memory). The same loop is instant on the interpreter.
Arrays got the single-owner fast path (`GcObject::unique`, `AddLocal`/`PushLocal`);
objects did not.

Suggested fix: extend the `unique` single-owner in-place update to `SetIndex`/`SetField`
on objects (and maps/sets), or make object storage persistent (HAMT) so a copy is cheap.

### 11. VM: pushing object literals is quadratic

```forge
let mut recs = []
for i in range(0, n) {
    recs.push({ a: i })
}
```

5,000: 92 ms; 10,000: 265 ms; 20,000: 1,014 ms; 50,000 lines of input parsed this way took
5.1 s, and the same loop inside a function was killed (memory). `recs.push([i])`,
`recs.push(i)` and `map(range(0, n), fn(i) { return { a: i } })` are linear. Probably the
object literal clears the array's `unique` bit or the GC re-marks the growing array on
every allocation.

Suggested fix: find why an object-literal argument makes `PushLocal` copy, add a perf test
(`tools/bench.sh`) for pushing records.

### 12. Interpreter: `get` and `has_key` copy the whole object

`Environment::get` deep-clones a variable on read (`CLAUDE.md` "Reading a variable copies
it"); builtins receive their arguments that way, so `get(counts, w, 0)` on an object with
n keys costs O(n). The indexing form `counts[w]` borrows and is fast.

Suggested fix: pass the object to `get`/`has_key`/`contains`/`len`/`keys` by borrow
(`with_value`), or make `Value::Object` reference-counted (copy-on-write).

### 13. `set` and `map` are association lists

`set(xs)`, `st.has(x)`, `st.add(x)`, `m.get(k)`, `m.set(k, v)` are linear per call on both
engines (and `m.set` / `st.add` copy the collection on the interpreter). Programs that
reach for a set "for O(1) membership" get the opposite.

Suggested fix: back `Set`/`Map` by hash maps (the interpreter's `Value` would need `Hash`
or a key projection); `CLAUDE.md` already notes "hashed Set/Map indexes" as blocked on
Arc-backed values.

### 14. What a model's natural solution costs

The idiomatic word count

```forge
let words = regex.find_all(fs.read("/dev/stdin").lower(), "[a-z]+")
let mut counts = {}
for w in words {
    counts[w] = get(counts, w, 0) + 1
}
```

over 100,000 words with 5,534 distinct ones takes 28 s on the VM and 30 s on the
interpreter (findings 10 and 12 together); Python's equivalent takes 0.03 s and
`frequencies(words)` 0.07 s. The three large `data` tasks (latest_record_wins,
word_freq_large, room_bookings_merge) and chunked_word_count are written so that this
pattern times out; their references use `frequencies`, `group_by`, `sort` and `map`.
Until the fixes land, `llms.txt` should say: count with `frequencies(xs)`, group with
`group_by(xs, fn)`, and build arrays of records with `map`.

## Gaps

### 15. HTTP handlers: no status codes, no headers

`HandlerWorker::call_http` returns `(StatusCode::OK, json)` or
`(StatusCode::INTERNAL_SERVER_ERROR, {"error": message})`
(`src/runtime/server.rs`); `HandlerRequest` has path params, query params and the body,
no headers. So a Forge API cannot answer 201, 400, 404 or 409, cannot set
`Location`/`Cache-Control`, and cannot read `Authorization` (bearer auth must use a query
parameter). The server tasks in tier v2 therefore report errors in the body with status
200; a natural prompt ("respond 404 when the item does not exist") is unsolvable in Forge.

Suggested fix: let a handler return a response object, e.g.
`return response(404, { error: "not found" }, { headers: { ... } })` or a
`Response { status, body, headers }` struct, and bind `headers` (lower-case names) like
`query`. Document both in `llms.txt`.

**Fixed:** `return http.response(404, { error: "not found" })`,
`http.response(201, body, { Location: "/items/1" })`, `http.response(204)`; a String body
is sent as `text/plain` unless a `Content-Type` is given. Handlers read headers through a
`headers` parameter (lower-case names) or the whole request through `req`/`request`
(`method`, `path`, `params`, `query`, `headers`, `body`). Status and headers are
validated (200-599, no CR/LF injection). Both engines, `tests/server_engine_parity.rs`;
documented in `llms.txt`.

### 16. Top-level database connections are not visible to handlers

```forge
@server(port: 18599)
db.open("data.db")
db.execute("CREATE TABLE IF NOT EXISTS t (x INTEGER)")

@get("/rows")
fn rows() -> Json {
    return db.query("SELECT x FROM t")
}
```

`GET /rows` answers 500 `{"error": "no database connection open (call db.open() first)"}`
on both engines: the SQLite connection lives in thread-local state of the thread that ran
the top level, and each request runs on a blocking-pool thread. Opening the database in
every handler works. `llms.txt` says handlers "may read any top-level binding", which a
model reads as "open the database once at start-up".

Suggested fix: carry the connection (or its path) into request forks, or document the
per-handler `db.open` rule in the HTTP server section.

**Fixed:** the connections the top level opens (`db.open`, `pg.connect`) are shared by every
handler (`stdlib::db_share`): statements run one at a time, a transaction belongs to the
request that opened it and is rolled back when the request ends, and
`db.last_insert_rowid()` is per request. `tests/server_database.rs` hammers a counter on
both engines (plain updates and read-modify-write transactions, no lost updates).

### 17. Imports cannot be renamed; re-importing re-runs the module

```forge
import { name as aname } from "lib/a.fg"
```

`[E0019] import 'lib/a.fg' does not export 'as'`: the parser reads `as` as a second name.
Two modules that export the same name can only be used by importing one, copying its
functions into differently named variables, then importing the other
(clashing_modules shows the pattern). Importing a module a second time (`import "m.fg"`
after `import { f } from "m.fg"`) runs its top level again: side effects repeat and its
module state is reset.

Suggested fix: support `import { a as b } from "..."` (and `import "m.fg" as m` for a
namespace object); cache a module per resolved path within a run.

**Fixed:** `import { a as b } from "m.fg"` (functions and variables; renaming a struct or
type is E0019), on both engines, the checker, the LSP and `forge fmt`. A module runs once
per program and its importers share its state. Not done: `import "m.fg" as m` (namespace
objects).

### 18. Request bodies must be JSON with the JSON content type

A POST/PUT route uses axum's `Json` extractor: a request without
`Content-Type: application/json` gets `415 Expected request with Content-Type:
application/json` as text/plain, invalid JSON gets `400 Failed to parse the request body
as JSON: ...` as text/plain, and a POST without a body is a 415. The handler never runs,
so it cannot answer with its own JSON error. Not mentioned in `llms.txt`.

Suggested fix: bind the raw body (`body` as text when it is not JSON, or a separate
`raw_body` parameter) and let the handler decide; at least answer framework errors as
JSON `{"error": ...}`.

**Fixed:** bodies are decoded by content type: JSON is parsed, a form becomes an object of
Strings, anything else (or no content type) is a String; a POST without a body binds `{}`.
Invalid JSON is `400 {"error": "invalid JSON body: ..."}`, a non-UTF-8 body `415`, both as
JSON.

### 19. `llms.txt` describes import paths wrongly

`llms.txt`: "Paths resolve relative to the current working directory, then
`forge_modules/<name>/main.fg`." `package::resolve_import_from` resolves relative to the
importing file first, then the working directory, then `forge_modules/<name>/main.fg`,
`forge_modules/<name>.fg`, `forge_modules/<name>/src/main.fg` and `.forge/packages`, and
the `.fg` suffix is optional. (The behaviour is the useful one: expression_lib's
`lib/parser.fg` imports `"tokenizer.fg"` from its own directory.)

Suggested fix: correct the sentence.

**Fixed:** `llms.txt` and the spec describe the resolution order.

### 20. No regex capture groups

`regex` has `test`, `find`, `find_all`, `replace`, `split`; extracting `user=(\w+)` needs
either a hand-written scanner or `regex.replace(line, "^.*user=(\\w+).*$", "$1")` (which
only works when the pattern matches; `replace` accepts `$1` because it is Rust's
`replace_all`, also undocumented). Every log-parsing task asks for groups.

Suggested fix: `regex.match(text, pattern)` returning `null` or
`{ text, start, end, groups: [...], named: {...} }`, and `regex.match_all`.

### 21. Behaviour models need but `llms.txt` does not state

Each of these cost a wrong guess while writing references:

- `select(channels, ms)` returns `[index, value]`, or `null` on timeout **and** when the
  channels are closed (the two cannot be told apart); `receive` on a closed, drained
  channel returns `null`.
- `exit(code)` exists (and flushes output); there is no `eprintln`.
- A program ends when its top level ends, abandoning detached `spawn` tasks that are still
  running (useful: `shared_deadline` relies on it).
- Arrays accept negative indexes (`xs[-1]`, mentioned only in the E0009 hint).
- `Map` methods are `get`, `has`, `set`, `remove`, `keys`, `values`, `len`, `to_array`;
  `llms.txt` shows `set`/`get` only (`m.delete(k)` is E0013).
- `time.parse(s)` returns an object with `unix`, `iso`, `year` ... `weekday` fields and
  accepts offsets; `time.format(t_or_unix, "%Y-%m-%d")` takes strftime patterns and always
  formats in UTC; `llms.txt` lists only the names.
- There is no `cond ? a : b` (E0002 "unexpected token: Colon"); write `if`/`else`.
- `match` patterns cannot be arrays or tuples (`[Num(x), Num(y)] => ...` is
  "expected pattern"); nest matches instead.
- `typeof` returns `Int`, `Float`, `String`, `Bool`, `Null`, `Array`, `Object` (also for
  algebraic values) ..., which validation code compares against.
- `http.get` raises on connection failure but returns 4xx/5xx responses normally
  (`resp.ok` is false); `resp.json` is `null` both for an invalid body and for the JSON
  document `null`.

Suggested fix: add these to `llms.txt` (a "Concurrency details", "Maps and sets" and
"time" paragraph would cover most).

### 22. No typed dictionary for objects

`let c: Map<String, Int> = { a: 1 }` is `T0001 type mismatch: 'c' declared as
Map<String, Int> but assigned Object` (an error in 2027), and `{String: Int}` is a syntax
error. The MCP section of `llms.txt` lists "`Object`/`Map<String, T>` object" as
parameter types, which suggests the two are interchangeable. Typed programs end up with
untyped `Object` for every JSON-shaped value.

Suggested fix: let an object literal (or `Object`) be assignable to `Map<String, T>` when
its values fit, or add an `Object<T>`/`{String: T}` annotation; state the rule in
`llms.txt`.

### 23. HTTP client errors

`http.get(url, { timeout: 1 })` against a server that answers after 2 s and `http.get` of
a refused port both raise `E0016 http.get error: request failed: error sending request for
url (...)`; a program cannot distinguish a timeout from a refused connection. The unit of
`timeout` (seconds) is not documented, while `await_timeout` and `select` take
milliseconds.

Suggested fix: distinct messages (and `e.type`) for timeout, connection refused, DNS and
TLS errors; document `timeout` in seconds.

### 24. `e.message` sometimes contains the hint

```forge
try {
    let o = { a: 1 }
    println(o.b.c)
} catch e {
    println(e.message)   // "no field 'b' on object\n  hint: available fields: a"
}
```

Programs that print `e.message` print two lines. Suggested fix: keep hints out of
`message` (expose them as `e.hint`).

### 25. No stderr output

There is no `eprintln`/`io.stderr`; `log.*` writes structured tracing events (with
timestamps and targets) to stderr. CLI-style programs (grep_exit_status, ini_config_check)
can only report errors on stdout plus an exit status.

Suggested fix: `eprintln(...)` / `eprint(...)` builtins routed through `runtime::stdio::err`.
