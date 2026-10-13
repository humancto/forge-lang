# Findings from writing eval tier v3

Writing the tier-v3 reference solutions (`eval/tasks_v3/`, 36 tasks) against Forge v0.11.0
(this branch's base, `5fc1cef`) turned up the problems below. Every repro was run with
`forge` (VM) and `forge --interp`, in editions 2026 and 2027, with the debug build the
harness uses, unless a section says otherwise.

Nothing was worked around silently. Where a task exists to exercise the broken feature,
its reference uses the natural code and the task is marked
`known_failing = "V3 finding <n>"` (validate reports it under `KNOWN FAILING`). Where a
task is about something else (mostly the performance cliffs in large-input tasks), the
reference avoids the trap and a comment in it names the finding.

Severity: **bug** (wrong result, crash, engine divergence), **perf** (natural code times
out or exhausts memory), **gap** (missing feature or behaviour a model cannot learn from
`llms.txt`).

| # | Severity | Finding | Tasks |
|---|---|---|---|
| 1 | perf | `regex.*` compiles the pattern on every call (~40 µs); a few regex calls per line make a 150k-line log take 75-110 s | access_log_rollup, ledger_replay, interval_sweep, json_orders_rollup (workaround) |
| 2 | perf | Interpreter: passing a top-level/captured array to a builtin (`slice(chars, i, j)`) copies the whole array per call: quadratic | nested_list_parser (workaround) |
| 3 | perf | VM: `obj[xs[i]] = v`, `obj[o.f] += 1`, `obj["k" + s] = v` copy the whole object per write (and grow RSS to GBs); `let k = xs[i]; obj[k] = v` is O(1) | all bulk_data tasks (workaround) |
| 4 | perf | VM: a function that writes an element of a top-level array/object copies the whole container per write; RSS grows to GBs | network_components (**known failing** on the VM), typed_route_planner |
| 5 | bug | **Fixed.** A method cannot change its receiver: `it.n += 1` / `it.items.push(x)` in an `impl` method is silently lost | (any stateful struct) |
| 6 | bug | Type checker: a parameter declared with an interface type cannot be passed to another parameter of that interface (T0012 "struct 'Metric' does not satisfy interface 'Metric'"), an error in 2027 | typed_route_planner (**known failing**) |
| 7 | bug | VM: `import { Plugin } from "api.fg"` of an interface fails (E0019 "does not export"); the interpreter imports it (**fixed**) | plugin_pipeline |
| 8 | bug | An algebraic type's name cannot be imported (`import { Shape } from "m.fg"` → E0019, both engines), so `fn f(s: Shape)` in another module is T0015 (error in 2027) (**fixed**) | stack_machine_modules |
| 9 | bug | VM: a name a module imports cannot be imported from it (re-export through a package entry); the interpreter allows it (**fixed**) | csvlite_package |
| 10 | bug | `timeout 0.5 seconds { }` truncates the limit to 0 and fails at once ("exceeded 0 second limit"); there is no millisecond unit | retry_with_timeouts (workaround) |
| 11 | bug | **Fixed.** Interpreter: a struct method named `push` is dispatched to the builtin (`E0015 push() first argument must be array`); the VM calls the method | (any struct with `push`) |
| 12 | bug | Builtin argument errors differ between engines: `join([1, 2], 5)` is `"152"` on the VM and E0015 on the interpreter; `split`/`keys`/`push` error texts differ | error_code_report (avoids them) |
| 13 | gap | Programs cannot receive command-line arguments: `forge run main.fg --top 3` is a usage error, `io.args()` is the host's whole argv (forge's own flags included) and empty under `--sandbox` | word_tool_args (**known failing**) |
| 14 | gap | `1e2` is not a number literal: it lexes as `1` followed by the name `e2` (T0006 warning, then E0003 at run time) | json_canonical |
| 15 | gap | JSON numbers lose their kind: `json.compact(1.0)` is `1`, `-0` parses as Float `-0`, integers beyond i64 silently become floats, `\f`/`\b` are written `\u000c`/`\u0008` | json_canonical (workaround) |
| 16 | gap | `llms.txt` does not document `retry`, `timeout` or `schedule` (kept in 2027); `retry` is statement-only and its exhaustion error is E0000 | retry_with_timeouts, job_queue_worker |
| 17 | gap | Many runtime errors are unclassified E0000 (`int("12x")`, `float("x")`, `json.parse` errors, invalid regex, exhausted `retry`); E0011 (invalid index) is not in `llms.txt`; `e.type` is inconsistent | error_code_report |
| 18 | bug | Type checker: `let mut lo = null` then `lo == null \|\| v < lo` warns T0014 ("Option type used in operation") although the comparison is guarded | parallel_chunk_stats (workaround) |
| 19 | gap | String literals have no Unicode escape (`"\u00e9"`, `"\u{feff}"` are E0001) and there is no `chr`/`ord`; `llms.txt` does not list the escapes that exist | csv_normalize (workaround) |
| 20 | gap | `select` returns `null` both on a timeout and when the channels are closed, and there is no `is_closed`: a consumer cannot tell a finished producer from a slow one | merge_streams (end marker) |
| 21 | perf | VM, edition 2027: annotated hot functions run ~3x slower than in 2026 (sudoku_solver 0.7 s → 2.1 s) | sudoku_solver |
| 22 | perf | VM: a two-level place update `routes[k].lat.push(x)` is ~5x slower than on the interpreter (access_log_rollup: VM 11 s, interpreter 6 s) | access_log_rollup |

Harness issues found and fixed on the way (not Forge findings): `timed = true`, which
`eval/README.md` documented, was rejected as an unknown key; `stdin_file` was read with
newline translation, so CRLF input never reached the program.

---

## Performance

### 1. Regex patterns are compiled on every call

```forge
let t0 = time.now().unix_ms
let mut c = 0
for i in range(0, 20000) {
    if regex.test("12345", "^[0-9]+$") {
        c += 1
    }
}
println("{time.now().unix_ms - t0} ms")   // ~900 ms VM, ~730 ms interpreter
```

About 40 µs per call on both engines, independent of the input: the pattern is compiled
each time. The natural access-log reference (four digit checks plus one per path segment)
took 110 s (VM) / 75 s (interpreter) for 250,000 lines; Python takes 1 s. A character
loop instead brings it to 11 s / 6 s.

Suggested fix: cache compiled patterns (a small LRU keyed by the pattern string, shared by
both engines in `stdlib/regex.rs`), as Python's `re` does.

**Fixed:** every `regex.*` function compiles through `regex_module::compiled`, a bounded LRU
of 256 patterns (keyed by the pattern text, flags included; patterns over 4 KiB and invalid
ones are not cached), thread-safe, shared by both engines. A run with a resource budget
(sandbox, MCP, `--max-memory`) gets its own cache owned by the `Budget`, so its patterns are
charged to it and freed with it. The repro takes ~55-80 ms on both engines (was ~670 /
840 ms with the same debug build); `benchmarks/vm/regex_lines.fg` (50k lines, five calls
each) went from 75 s to 0.97 s on both engines. The four workaround references
(access_log_rollup, ledger_replay, interval_sweep, json_orders_rollup) use `regex.test`
again.

### 2. Interpreter: array arguments of builtins are copied

```forge
let mut chars = []
fn first_two(i: Int) {
    return slice(chars, i, i + 2)
}
for n in [4000, 8000, 16000] {
    chars = map(range(0, n), fn(i) { return "x" })
    let t = time.now().unix_ms
    for i in range(0, 2000) {
        first_two(i)
    }
    println("n={n}: {time.now().unix_ms - t} ms")
}
```

Interpreter: 616 / 942 / 2343 ms (grows with n); VM: 13 / 11 / 29 ms. Reading `chars` to
pass it to `slice` deep-copies all n strings ("reading a variable copies it"). The
nested-list parser (`int(join(slice(chars, start, pos), ""))` per number) took 8 s on the
interpreter for a 16,000-character line, 0.35 s on the VM.

Suggested fix: let builtins that only read an array argument (`slice`, `len`, `index_of`,
`contains`, `join`, ...) borrow it (`Environment::with_value`) instead of cloning.

**Fixed:** a call of `slice`, `first`, `last`, `join`, `index_of`, `last_index_of`,
`substring`, `starts_with`, `ends_with` (and, as before, `len`, `contains`, `get`,
`has_key`) whose first argument is a variable or a field/index chain of one
(`slice(chars, i, j)`, `len(o.items)`), and the array-method forms (`xs.slice(i, j)`,
`xs.join(",")`), runs the builtin on a reference to the value under the scope lock when the
other arguments are effect-free (`places::read_only_builtin`, shared with the builtin
itself; errors take the ordinary path). The repro is 2-3 ms for every n (was 703 / 1112 /
2191 ms); `benchmarks/interp/slice_scan.fg` went from 121 s to 0.12 s. nested_list_parser
uses `join(slice(chars, start, pos), "")` again.

### 3. VM: an object key that is an index or field read copies the object

```forge
let names = map(range(0, 6000), fn(i) { return "k{i}" })
let mut o = {}
for k in names {
    o[k] = 0
}
let t0 = time.now().unix_ms
for i in range(0, 3000) {
    let k = names[i]
    o[k] += 1                       // 1 ms
}
let t1 = time.now().unix_ms
for i in range(0, 3000) {
    o[names[i]] += 1                // 1,900 ms, RSS ~900 MB
}
println("{t1 - t0} ms / {time.now().unix_ms - t1} ms")
```

VM only (the interpreter takes 2-4 ms for every form). Also slow: `o[f[1]] = 0` where `f`
is a local array (12.8 s for 20,000 lines in ledger_replay), `o[it.sku] = ...`,
`o["k" + str(i)] = 7` (80 µs each). Each write copies the 6000-key object, and the copies
are collected so late that RSS reaches 2-3 GB. Five of the six bulk_data references hit it
with their first natural draft.

Suggested fix: the in-place `SetIndexLocal` path should not depend on the key expression;
evaluate the key into a temporary register first. Separately, the GC should account for
large garbage containers so a copy-per-iteration loop cannot reach gigabytes (with
`--max-memory 1GB` the same loop stays at 1 GB, so the collector can keep up when pushed).

**Fixed:** the write side (`SetIndexLocal`) already took any key; the copy came from the
*read* `o[k]` (of `o[k] += 1`, or `get(o, k)`-style reads), which kept the in-place
`GetIndexLocal` only for literal/variable/arithmetic keys and otherwise read the object
with `GetLocal`, sharing it, so the next write copied it. Now a key that runs no code (index
and field reads, interpolation, operators: `index_runs_no_code`) is evaluated first and read
with `GetIndexLocal`; a key that may call (`o[str(i)]`) reads a `PeekLocal` snapshot of the
local taken before the key runs (as the generic read does), then shares only the element
(`GetIndex` + `Move`). The `CallPure` reads (`get`, `has_key`, `len`, `contains`, `m.get`)
and `AddLocal`/`PushLocal`/`UpdateSetLocal` accept the same wider operands. In the repro
extended with `o[it.sku] += 1`, `o[names[i]] += 1` takes 1 ms (was 1903 ms) and
`o[it.sku] += 1` 1 ms (was 1392 ms); a plain store with a computed key
(`o["k" + str(i)] = 7`) was already in place and was only slow behind the garbage of the
copying loops. `benchmarks/vm/computed_keys.fg` (300k updates of a
10k-key object) runs in 0.36 s and was killed for memory before. The GC accounting of large
garbage containers was not changed. The bulk_data references index with `f[1]`, `it.sku`,
`f[0]`, `f[2]` directly again.

### 4. VM: writing a top-level container from a function copies it

```forge
let n = int(input())
let mut parent = range(0, n)

fn set_root(x: Int, r: Int) {
    parent[x] = r
    return null
}

let t = time.now().unix_ms
for i in range(0, 20000) {
    set_root(i % n, (i * 7) % n)
}
println("{time.now().unix_ms - t} ms")
```

`echo 40000 | forge run uf.fg`: 4,077 ms and 2.4 GB RSS on the VM; 53 ms and 19 MB on the
interpreter. Every write through the captured top-level binding copies the 40,000-element
array. Union-find with path compression (network_components: 120,000 nodes) is killed by
the OOM killer at 11 GB RSS after ~16 s on the VM; with `--max-memory 1GB` (the task's
`max_memory`) the collector keeps up and it takes 25 s against the task's 15 s limit; the
interpreter needs 4 s. typed_route_planner's graph construction (`graph[a].push(edge)`
from a function) is 1.5 s for 900 roads for the same reason.

Suggested fix: give upvalue/global containers the single-owner treatment locals have
(`GcObject::unique` on the cell's value), or store the updated container back without
copying when the cell holds the only reference.

**Fixed (captured variables):** a captured local's register and its upvalue cell are now
one owner (`GetUpvalue` shares what it copies out of a cell; every local access already
went through the cell), so the top level updates a captured array in place, and a function
binds each captured mutable variable it uses as a container to a hidden local aliasing the
cell (new opcode `BindUpvalue`, bytecode format 1.7), so `parent[x] = r`, `parent[x]` and
`graph[a].push(e)` inside the function use the in-place local opcodes. Snapshots held while
a call may run (`PeekLocal` before a key with a call) share a captured local first
(`PEEK_SHARE_IF_CAPTURED`), so a callee's write through the cell copies. The repro takes
60 ms (was 3438 ms with the same debug build); `benchmarks/vm/captured_writes.fg`
(union-find, 50k nodes) went from 15.4 s to 0.25 s, and network_components passes on the
VM (no longer `known_failing`). A top-level `let` that a function references *before*
its declaration is a VM global; a global slot can own its value too (`GetGlobal` shares),
and a statement updating its container aliases it with a temporary register (`BindGlobal`),
so a function written above the data it updates no longer copies either: the repro with
`set_root` above the `let` takes 20 ms (was 3427 ms), `benchmarks/vm/global_writes.fg` 11.4 s
-> 0.16 s.

### 21. VM, edition 2027: annotated hot code is ~3x slower

sudoku_solver (two small annotated functions called ~200,000 times): VM 0.71 s in 2026,
2.14 s in 2027; interpreter 2.8 s / 3.0 s. The per-call `__types.check` instrumentation
seems to keep the functions out of the fast call path / JIT. Not a timeout today; worth
measuring in `tools/bench.sh` with `--edition 2027`.

**Fixed:** each check was an ordinary call (`__forge_call_method` on the `__types` module,
then the native `__types.check`): two native calls with argument vectors and GC scopes, a
linear scan of the builtin registry per native call, a copy of the module object's field
map, a hashed lookup of the type string and three string constants materialized per
check, about 1 µs in all. The JIT already compiled checked *pure* functions, but `fits`
reads the top-level grid, so it stayed in the VM. The VM compiler now emits a `TypeCheck`
opcode in front of every check of a common type (`Int`, `Float`, `String`, `Bool`,
`Null`, `Object`, a struct/`type` name, arrays of scalars, `?T` of those;
`src/vm/type_check.rs`): when the value certainly fits and nothing would be widened it
takes the jump over the call, otherwise the unchanged call decides (errors, Int → Float).
A statement check of a local (the argument checks) reads the local in place, so a passing
argument check is one dispatched instruction. The JIT decides `TypeCheck` from the value's
kind. The instrumentation also leaves out checks of results that always fit (`return
false`, `return "..."`, `return null` for `?T`, comparisons for `Bool`). Debug build
(opt-level 1) on the loaded 4-core host, median of 7 (min): sudoku_solver VM 1.11 s (1.03)
in 2026; 2027 2.37 s (2.19) before, 1.14 s (1.07) after. `benchmarks/typed/sudoku.fg`
(new `typed` suite of `tools/bench.sh`): 0.92 s (0.90) in 2026; 2027 2.29 s (1.99) before,
1.20 s (1.05) after. `benchmarks/typed/geometry.fg` (struct, Option and widened Float
arguments): 0.68 s (0.62) in 2026, 0.82 s (0.73) in 2027. `src/vm/type_check_tests.rs`
compares the fast path with the call for 30 values × 25 types.

### 22. VM: two-level place push is slower than the interpreter

```forge
let mut r = {}
for i in range(0, 80000) {
    let k = "k{i % 12}"
    if !has_key(r, k) {
        r[k] = { lat: [], errors: 0 }
    }
    r[k].lat.push(i)
}
```

VM 830 ms, interpreter 150 ms (one-level `g[k].push(i)`: 105 / 130 ms). Not quadratic, but
it makes the VM ~2x slower than the interpreter on access_log_rollup. Per `CLAUDE.md`,
deeper chains take the copying read-modify-write path; `slot_owned` could extend to
objects.

**Fixed:** a statement `local[k1]..[kn].push(x)` with two or more steps first runs the new
opcode `PushPlaceLocal`, which appends in place when the local owns its container and every
level is a plain array or object owned by its slot (`slot_owned` now also marks objects and
arrays at intermediate levels; a level that is not owned is replaced by a copy of itself
once); otherwise it reports "not done" and the generic place update that follows runs. The
repro takes ~190 ms on both engines (VM was 899 ms); `benchmarks/vm/deep_push.fg` (200k
pushes) went from 10.8 s to 0.31 s.

---

## Bugs

### 5. Methods cannot change their receiver, silently

```forge
struct Counter {
    n: Int
}
impl Counter {
    fn bump(it) {
        it.n += 1
        return it.n
    }
}
let mut c = Counter { n: 0 }
c.bump()
c.bump()
println(c.n)            // 0 on both engines, both editions; no warning
```

`it` is a copy, so `it.n += 1`, `it.items.push(x)` and similar updates disappear when the
method returns. Models write stacks, queues, caches and counters this way (a generic
`Stack<T>` with `push`/`pop` methods is the textbook first draft of stack_machine_modules).
`llms.txt` shows only read-only methods and never says that receivers are values.

Suggested fix: either write the receiver back when the call's receiver is a place rooted at
a mutable binding (like the nested method updates of v2 finding 1), or reject the
assignment to `it` (a T-code and a run-time error). At the least, document "methods return
a new value; write `c = c.bump()`".

**Fixed:** a method that updates its receiver declares it `mut it` (`fn bump(mut it)`), and
a plain receiver is read-only. Decisions (both engines, both editions, `llms.txt` "Structs,
methods, interfaces", spec `type-system/method-blocks.md`):

- *Opt-in and visible.* `mut` is allowed only on a method's receiver (the first parameter
  of an `impl`/`give` method, named `it` or `self`; anywhere else it is a syntax error).
  `it.n += 1`, `it.items.push(x)` or `it = ...` in a method declared `fn m(it)` is E0006
  `cannot reassign immutable variable 'it'` with a hint to declare `mut it` (T0017 from
  the checker), so the silent loss above is now an error. A method that wants to return a
  modified copy copies first (`let mut c = it`).
- *Write-back.* When a `mut it` method returns normally (explicit `return`, tail value, or
  a `?` early return), its final `it` is assigned to the place the call was made on: a
  variable, or a field/index chain rooted at one (`c.bump()`, `xs[i].bump()`,
  `o.f.bump()`; index expressions evaluated once, before the arguments). It is an ordinary
  assignment, so an immutable root is E0006 (T0017 statically), checked after the method
  returned. An error inside the method writes nothing back. Aliases are unaffected.
- *Temporaries.* `make().bump()` (any receiver that is not a place) is E0006 `cannot update
  a temporary value` (T0017 statically) rather than an allowed-but-discarded update: a
  discarded update is exactly the silent loss this finding is about, and Swift makes the
  same choice. Methods without `mut it` work on temporaries as before.
- *Everywhere a method can be found.* Struct methods, algebraic-type methods (`it = Green`
  replaces the receiver), calls through an interface-typed value (dispatch is dynamic, so
  the implementing method's `mut it` decides; interface signatures do not mention the
  receiver), embedded delegation (`outer.hit()` updates `outer.inner`), and the fast paths
  that end in a method call (`s.push(x)` on a struct, `g[k].push(x)`, `m.get(k)`).
- *Implementation.* Interpreter: `bind_params` binds a plain receiver immutably;
  `call_function_inner` hands a `mut it` method's final receiver to the dispatch that called
  it (`call_user_method`), which assigns it to the place (`assign_receiver_place`). VM: the
  compiler binds a plain receiver immutably and, in a `mut it` method, calls
  `__forge_receiver_out(it)` before each return; method calls on places go through
  `__forge_call_method_at` followed by the new `TakeReceiver` opcode, which stores the
  receiver back only after a `mut it` method (`.fgc` format 1.7). Calls on builtin modules
  (`math.sqrt(x)`) keep the plain path the JIT understands.

### 6. Interface-typed parameters cannot be passed on

```forge
interface Shape {
    fn area() -> Int
}
struct Sq {
    s: Int
}
impl Shape for Sq {
    fn area(it) -> Int {
        return it.s * it.s
    }
}
fn inner(x: Shape) -> Int {
    return x.area()
}
fn outer(x: Shape) -> Int {
    return inner(x)
}
println(outer(Sq { s: 3 }))
```

Edition 2027 (both engines): `[T0012] Error: struct 'Shape' does not satisfy interface
'Shape': missing 'area'` at `inner(x)`; nothing runs. 2026: the same warning, then `9`.
The checker treats the parameter's interface type as a struct named `Shape`.

Suggested fix: when the argument's static type is the interface itself (or an interface
that extends it), it satisfies it. Add a 2027 checker test.

**Fixed:** the satisfaction check (`check_interface_satisfaction`) looked the interface up
as a struct. A value of interface `I` now satisfies `I`, and another interface `J` when
`I` declares every method of `J` with the same parameter count and a fitting declared
return type (`check_interface_subsumes`; otherwise T0012 `interface 'Named' does not
satisfy interface 'Shape': missing 'area'`), also inside arrays and options. Checker tests
in both editions, `tests/edition2027/interface_params_test.fg` on both engines;
typed_route_planner is no longer `known_failing`.

### 7. VM: interfaces cannot be imported by name

```forge
// api.fg
interface Named {
    fn name() -> String
}
```
```forge
// main.fg
import { Named } from "api.fg"
struct Dog {
    n: String
}
impl Named for Dog {
    fn name(it) -> String {
        return it.n
    }
}
println(satisfies(Dog { n: "rex" }, Named))
```

VM (2026, 2027): `[E0019] import 'api.fg' does not export 'Named'` (hint: "only top-level
`fn`, `let`, `type` and `struct` definitions can be imported"). Interpreter: `true`.
`llms.txt` says an interface keeps its name when imported, so importing one is expected to
work. `import "api.fg"` works on both.

Suggested fix: export interface definitions from VM modules (the selective-import path in
the VM compiler/`__forge_import_module`), and add a parity fixture.

**Fixed:** the VM's import looks a requested name up among the module's prefixed globals
and then among the structs *and interfaces* it registers under their own names
(`VM::module_export`, shared by `import { ... }` and `import "m.fg" as m`). Both engines
also bind interface names on a bare `import "m.fg"`, and interfaces are namespace members.
Tests: `tests/parity/supported/import_type_and_interface.fg`,
`tests/module_exports_test.fg`; plugin_pipeline is no longer known failing.

### 8. Algebraic types cannot be imported by name

```forge
// shapes.fg
type Shape = Circle(Int) | Dot
```
```forge
// main.fg
import { Shape, Circle, Dot } from "shapes.fg"
println(Circle(2))
```

Both engines, both editions: `[E0019] import 'shapes.fg' does not export 'Shape'`.
Importing only the variants works, but then `fn area(s: Shape)` is
`[T0015] unknown type 'Shape'` (an error in 2027). The E0019 hint itself says `type`
definitions can be imported. Only the whole-module `import "shapes.fg"` lets another
module use the type in annotations.

Suggested fix: export the type name (for annotations and for `satisfies`/methods) along
with its variants; importing the type could bring its variants too.

**Fixed:** a `type` name is now a value on both engines, like a struct or interface name:
its description `{__kind__: "type", name: "Shape", variants: ["Circle", "Dot"]}` (the
object the type's metadata already was; not when a variant has the type's own name,
`semantics::type_name_is_value`). So it is exported like any top-level name:
`import { Shape } from "shapes.fg"` works, `fn area(s: Shape)` checks in the importer, and
`Shape.variants` / `m.Shape` are usable. Decision: importing the type does not bring its
variants (they are separate names, and a selective import binds exactly the names it
lists); write `import { Shape, Circle, Dot }`. A bare
`import "m.fg"` binds the type name together with the variants. The type checker already
treated type names as values and imports as type definitions. Tests:
`tests/parity/supported/import_type_and_interface.fg`, `tests/module_exports_test.fg`,
`imported_types_and_reexports_are_checked`; stack_machine_modules is no longer known failing.

### 9. VM: re-exports are not importable

```forge
// forge_modules/lib/src/core.fg
fn twice(x: Int) -> Int {
    return x * 2
}
```
```forge
// forge_modules/lib/main.fg
import { twice } from "src/core.fg"
fn quad(x: Int) -> Int {
    return twice(twice(x))
}
```
```forge
// main.fg
import { quad, twice } from "lib"
println(twice(5))
```

VM: `[E0019] import 'lib' does not export 'twice'`. Interpreter: `10`. A package whose entry
file gathers its API from internal modules works only on the interpreter. Structs behave
the same way. `llms.txt` does not say either way, so the engines should at least agree.

Suggested fix: decide (re-exports are useful for packages; the interpreter's behaviour),
implement it on the VM, and document it next to `import { a as b }`.

**Fixed:** re-exports are part of the language (the interpreter's behaviour, now
documented in `llms.txt`): a module exports every name its top level binds, including the
names its imports bind (`import { a, b as c }`, `import "x.fg" as x`, native imports) and,
on both engines, destructured `let`s. On the VM those bindings now live under the module's
private global prefix (`compiler::module_top_level_names`, plus the names of a bare
`import "x.fg"` inside a module, read once when the module is compiled) instead of leaking
into the program's globals unprefixed, so `__forge_import_module` and namespaces find
them, and `vm::module_share` carries them to other tasks with the module's other globals.
`semantics::namespace_members` lists them, so `import "lib" as lib; lib.twice(5)` works on
both engines (a re-exported struct is a struct member: `lib.Pt { x: 1 }`). The type
checker follows a selective import of a re-export to the module that defines it, so the
function keeps its signature (also under an `as` alias). A bare `import "lib"` still binds
only the module's own definitions. Tests: `tests/parity/supported/import_reexports.fg`,
`tests/module_exports_test.fg`, `module_once::a_reexporting_module_run_in_a_task_exports_its_imports`;
csvlite_package is no longer known failing.

### 10. `timeout` truncates fractional seconds

```forge
try {
    timeout 0.5 seconds {
        wait(0.1)
    }
    println("finished")
} catch e {
    println("{e.code} {e.message}")
}
```

Both engines: `E0031 timeout: operation exceeded 0 second limit` immediately (`timeout 1
seconds` with the same body finishes). `wait(0.5)` accepts fractions, `timeout` does not,
and `timeout 500 milliseconds` is a syntax error, so a sub-second time limit cannot be
written with `timeout` at all.

Suggested fix: parse the duration as a Float (`semantics::timeout_deadline` already
saturates), print it as given in the message, and accept `milliseconds`/`ms` like
`schedule` units.

**Fixed:** the limit is kept as a Float on both engines (`semantics::timeout_seconds`,
`timeout_deadline(now, f64)`, exact to the nanosecond, still saturating), and the error prints
it as a Forge number (`semantics::timeout_exceeded`: `exceeded 0.5 second limit`, still E0031).
`wait`, `timeout` and `schedule every` share one unit list (`semantics::DURATION_UNITS`):
`milliseconds`/`millisecond`/`ms`, `seconds`/`second`, `minutes`/`minute`, `hours`/`hour`
(before, `wait` and `timeout` took only `seconds`, `schedule` only the three plurals). The
words are matched after the number on the same line, not reserved, so `ms`, `second`, ... stay
ordinary names in both editions. The parser turns `wait`/`timeout` units into seconds (`500
ms` is `500 / 1000.0`, `2 minutes` is `2 * 60`); `schedule` keeps the canonical unit name and
takes a fractional interval (`semantics::schedule_interval`), which must be positive on both
engines (the interpreter used to spin on `every 0 seconds` and wrap a negative interval to
centuries; a Float interval silently became 60 s). There are no `retry` delays to convert.
retry_with_timeouts' reference now uses `timeout 150 ms { ... }`. Tests:
`tests/durations_test.fg`, `tests/edition2027/durations_test.fg` (both engines),
`semantics::tests::duration_units`, `durations_saturate_instead_of_panicking`,
`runtime::metadata::tests::schedule_units_are_normalised`.

### 11. Interpreter: a struct method named `push` calls the builtin

```forge
struct Stack {
    items: [Int]
}
impl Stack {
    fn push(it, x) {
        return Stack { items: it.items + [x] }
    }
}
let mut s = Stack { items: [] }
s = s.push(1)
println(s.items)
```

VM: `[1]`. Interpreter: `[E0015] push() first argument must be array` (both editions).
With an immutable `let s` (or a chained `s.push(1).items`) the interpreter calls the
method; with `let mut` it does not.
`CLAUDE.md` says "a struct with its own `push` gets a plain method call"; the interpreter
routes the mutating-method form to the builtin before looking at the struct's methods
(the same probably holds for `pop`, `add`, `remove`).

Suggested fix: look up user methods first in the interpreter's nested-method path, and
add the case to the parity fixtures.

**Fixed:** on a mutable variable the interpreter now treats `v.push(x)`, `v.pop()`,
`v.add(x)` and `v.remove(x)` like the VM: the arguments are evaluated, then the variable
is read; an array (`push`/`pop`) or set (`add`/`remove`) is updated in place, a frozen
value is the frozen error, and anything else (a struct's own `push`, a map's `remove`) is
an ordinary method call, with write-back when the method is declared `mut it`
(`tests/mut_receiver_test.fg`).

Two map divergences found on the way are fixed with it:

- `m = m.remove(g())` where `g` reassigns `m`: the VM read the map after the argument, the
  interpreter before. The rule both engines already applied to `push`/`pop`/`add`/`remove`
  on a mutable variable (arguments first, then the variable, so an argument's update is not
  overwritten; v2's differential fuzzer found the original bug) now holds whatever the
  variable contains, so the interpreter changed. Every other method call still evaluates its
  receiver before its arguments.
- Removing from a frozen map said `cannot remove a frozen value` (VM) and `cannot remove
  from a frozen set` (interpreter). Every frozen `push`/`pop`/`add`/`remove` error now comes
  from `semantics::frozen_method(method, kind)`: `cannot remove from a frozen map`, `cannot
  add to a frozen set`, `cannot push to a frozen array`, `cannot pop from a frozen array`
  (all E0029; the interpreter's set messages used to be unclassified E0000).

### 12. Builtin argument errors differ between engines

```forge
fn show(label: String, f) {
    try {
        println("{label}: ok {f()}")
    } catch e {
        println("{label}: {e.code} {e.message}")
    }
    return null
}
show("join", fn() { return join([1, 2], 5) })
show("split", fn() { return split(5, ",") })
show("keys", fn() { return keys(5) })
show("push", fn() { return push(5, 1) })
```

VM: `join: ok 152`, `split() requires (string, delimiter)`, `keys() requires an object or
map`, `push() requires an array`. Interpreter: `join: E0015 join() requires (array[,
separator_string])`, `split() requires (string, delimiter_string)`, `keys() requires object
or map`, `push() first argument must be array`. Also `len(json.parse("5"))` lacks the
`(got Int)` annotation on the interpreter. The codes agree, the texts and `join`'s
acceptance do not; `tests/error_codes.rs` only covers the fixtures in `tests/errors/`.

Suggested fix: raise these through shared helpers in `src/semantics/` (as `CLAUDE.md`
asks), make `join` reject a non-string separator on the VM, and add one fixture per
builtin family.

**Fixed:** an audit of ~300 bad calls (every builtin in `CLAUDE.md`'s string, collection,
functional and object lists, with Int/Null/Bool/Object/Array/String/Float arguments) found
86 differences; there are none left. The argument errors of those builtins come from one
table, `semantics::builtin_args::bad_args(name)`, on both engines (the VM maps its
`get_string_arg` failures to it), and the VM now rejects what the interpreter rejected:
`join` with a non-String separator, `starts_with`/`ends_with`/`replace` with non-Strings,
`has_key`/`get`/`pick`/`omit`/`merge` on non-objects, `entries`/`from_entries` of the
wrong kind. `get(array, i)` and dotted paths through arrays now work on the VM. The
interpreter's missing `(got ...)` annotations came from fast paths in `places.rs`
(`len`/`contains`/`push`/`pop` on a variable, and `null` parses as a variable) that raised
their own error; they now fall back to the ordinary call. The audit table is the parity
fixture `tests/parity/supported/builtin_arg_errors.fg` (engine_diff), and
`tests/builtin_arg_errors_test.fg` pins the repro's texts.

### 18. T0014 false positive on a guarded comparison

```forge
fn lowest(xs: [Int]) {
    let mut lo = null
    for v in xs {
        if lo == null || v < lo {
            lo = v
        }
    }
    return lo
}
println(lowest([3, 1, 2]))
```

`forge check`: `T0014 Option type used in operation; check with is_some() first`
(warning in both editions) at `v < lo`, although `lo == null ||` guards it. The idiom is
common for min/max scans; references must be warning-free, so parallel_chunk_stats uses a
counter instead.

Suggested fix: narrow `x` to non-null on the right of `x == null ||` (and in the `else` of
`if x == null`), or do not treat a `null`-initialised binding as an Option.

**Fixed:** the right operand of `&&` is checked with the left operand's null tests
applied, and the right operand of `||` with what the left operand being false proves, so
`lo == null || v < lo` and `x != null && x > 0` are clean. Conditions are now narrowed
by what they prove when true *and* when false: `a || b` false proves both negations, so
after `if best == null || v < best { return v }` `best` is not null; and the `else` of `if
x != null && y != null`, which used to make both Null (a false certain error on `x + 1`
there), narrows nothing.

---

## Gaps

### 13. No command-line arguments for programs

`forge run main.fg --top 3` exits with `error: unexpected argument '--top' found`.
`io.args()` returns the host's argv (`[.../forge, --interp, run, main.fg]`), so
`io.args_parse()` would report forge's own flags (`--error-format json`, `--max-time 10`
under the harness), and under `--sandbox` (no `process` capability) it returns nothing.
`llms.txt` says arguments are not passed, but `io` lists `args_parse`/`args_get`/
`args_has`, which suggests they are. A CLI tool with options and usage errors (exit 2) is
an everyday program.

Suggested fix: `forge run file.fg [--] args...` passes the trailing arguments; `io.args()`
returns only them (script name first, like `sys.argv`); allow them under `--sandbox`
(they come from the caller, not the host environment). word_tool_args is marked known
failing until then.

**Fixed:** `forge run file.fg [args...]` passes everything after the script path to the
program, flags included (clap trailing var args): forge's own flags go before the file
(`forge --interp run x.fg`, `forge run --allow-run x.fg`), and `forge run x.fg --interp`
gives `--interp` to the program. `io.args()` is exactly the program's command line, like
`sys.argv`: the script path as given, then the arguments (`["main.fg", "--top", "3"]`);
`io.args_parse`/`args_get`/`args_has` look at the arguments after the path, up to a `--`.
The arguments live in `runtime::program_args` (set once by the CLI, overridable per thread,
carried into spawned tasks by `permissions::inherit`), never in the host's argv, so they do
not depend on the `process` capability and `--sandbox` runs get them. A `Sandbox` sees only
`Sandbox::args` (default empty), whatever its capabilities; MCP `run_forge` takes an optional
`args` array (`io.args()` is `["<run_forge>", ...args]`). Binaries from `forge build --aot`
/ `--native` see their own argv (the launcher fallback passes it to `forge run`).
word_tool_args is no longer known failing (its reference reads `slice(io.args(), 1, ...)`).
Tests: `tests/program_args_cli.rs` (both engines, with and without `--sandbox`),
`run_forge_passes_program_arguments` (MCP), `host_stdin_and_argv_are_not_readable_without_process`
(security), `runtime::program_args::tests`.

### 14. Exponent literals

```forge
println(1e2)
```

`[T0006] Warning: unknown name 'e2'` and then `[E0003] undefined variable: 'e2'`: the
lexer reads `1` and the name `e2`. Models write `1e9`, `2.5e-3` freely.

Suggested fix: lex `digits[.digits](e|E)[+-]digits` as a Float literal; if exponents are
deliberately unsupported, make `1e2` a syntax error that says so.

**Fixed:** the lexer reads `digits[.digits](e|E)[+|-]digits` as one Float literal (`1e2` is
`100.0`, `1.5e-3`, `2E+10`, `1_000e1_0`); digits must follow the `e` (after an optional sign),
so `2ex` is still `2` then the name `ex`, and a literal too large for a Float (`1e999`) is a
syntax error. `forge fmt` keeps the spelling (it is one token, so `1e-3` is not spaced as a
subtraction); constants are ordinary Floats for the VM, the serializer and the JIT. Tests:
`lexer::tests::test_exponent_floats`, `tests/literals_test.fg` and
`tests/edition2027/literals_test.fg` (both engines); `llms.txt` and the spec list the forms.

### 15. JSON numbers lose their kind and precision

| input | `json.parse` | `json.compact` | Python |
|---|---|---|---|
| `1.0` | Float | `1` | `1.0` |
| `1e2` | Float | `100` | `100.0` |
| `-0` | Float `-0` | `-0` | int `0` |
| `9223372036854775808` | Float (silently) | `9223372036854776000` | exact int |
| `"\f"` | | `"\u000c"` | `"\f"` |

A float that round-trips through Forge comes back as an integer in the JSON text, so a
consumer that types by JSON syntax sees a different type. json_canonical encodes floats
itself.

Suggested fix: print whole Floats with `.0` in `json.stringify`/`compact` (str() may keep
its current form), keep `-0` an Int, and raise (or keep exactness) for integers that do
not fit i64 instead of silently converting.

**Fixed:** `json.stringify`/`compact`/`pretty` write a Float with a fraction or exponent
(`1.0`, `100.0`, `1e+21`; the digits `serde_json` uses for HTTP responses, so both paths
agree) and NaN/infinity as `null`; `json.parse` reads `-0` as the Int `0` (like `-0` in
Forge source; a same-length pre-pass in `json_module::parse_str`, so request bodies and
`fs.read_json` agree); `\b`/`\f` use the short escapes. Integers beyond i64 stay Floats:
that matches Int arithmetic overflow (a too-large literal in source is a syntax error,
which is not an option for data), and it is now documented in `llms.txt` and
`spec/src/stdlib/json.md`. Tests: `tests/json_numbers_test.fg` (both engines),
`json_module` unit tests, `tests/server_engine_parity.rs` (`2.0` and `\f` in a response).

### 16. `retry`, `timeout` and `schedule` are undocumented

These innovation blocks are kept in edition 2027 (`docs/editions/2027.md`) and work on
both engines (job_queue_worker's `schedule every 1 seconds` inside a server drains SQLite
jobs correctly on both), but `llms.txt` mentions none of them, so a model never uses them.
Details a model would need: `retry n times { }` is a statement (`let r = retry 3 times
{ ... }` is two unknown names), it re-runs the block on any error and finally raises
`retry failed after n attempts: <last error>` with code E0000; `timeout` raises E0031 (see
finding 10 for fractions).

Suggested fix: add a short section to `llms.txt` with the three blocks, their statement
form, units and errors; give exhausted retries their own code.

**Fixed:** `llms.txt` has a "Retry, timeout, schedule" section (statement form; values
leave through a variable or `return`, and `return`/`break` leave without retrying; the
100 ms, 200 ms, ... back-off; `timeout` units and E0031; `schedule` units, first run after
one interval, its own copy of the variables, errors ignored), with examples run on both
engines in both editions; `spec/src/dual-syntax/innovation.md` says the same. Exhausted
retries are E0041 (finding 17). `retry` stays statement-only: an expression form would be
a new `Expr` variant in both engines, the checker, hoisting and the formatter, while
`return` inside the block already covers it. Writing the examples found a divergence:
`forge run --interp` deferred `schedule`/`watch` start-up to the end of the top level even
without a server, so a script's schedule never ran; it is now deferred only for `@server`
programs, as on the VM (`tests/parity/supported/schedule_runs_during_top_level.fg`). Tests:
`tests/retry_timeout_test.fg` (both engines, both editions).

### 17. Unclassified runtime errors

Code E0000 (type `RuntimeError`) for `int("12x")`, `float("abc")`, any `json.parse` error,
an invalid regex and an exhausted `retry`. `null["x"]`, `5["x"]`, `"abc"[0]` are E0011,
which `llms.txt` does not list. `e.type` is `ReferenceError` for a missing key but
`RuntimeError` for an unknown field. A program that wants to tell "bad number" from "bad
JSON" from "bug" must match message prefixes.

Suggested fix: classify conversion errors (E0015 or a new code), JSON parse errors and
invalid patterns; list E0011 (and the codes error_code_report shows: E0016, E0021, E0026,
E0031) in `llms.txt`.

**Fixed:** four codes appended to `semantics::errors`, each raised through a shared helper
and with a fixture in `tests/errors/`: E0038 text that is not a number (`int("12x")`,
`float("abc")`; a value of the wrong kind such as `int([1])` stays E0015), E0039 invalid
JSON (`json.parse`, `fs.read_json`, `sh_json`), E0040 invalid regex (now one line:
`invalid regex '(': unclosed group`) and E0041 an exhausted `retry` (tried before every
other class, so the quoted last error cannot re-code it). `e.type` is now a function of
the code (`errors::error_type`), so a missing key and an unknown field are both
`ReferenceError`; new category `ValueError` for E0038–E0040. `llms.txt` lists the runtime
codes with their types (E0011, E0016, E0021, E0026, E0031 included). The E0000 fixture is
now a `toml.parse` failure. Tests: `tests/error_classes_test.fg` (both engines, both
editions), `helpers_classify_to_their_codes`, `error_type_follows_the_code`.

### 19. No Unicode escapes in string literals

`"\u00e9"` and `"\u{feff}"` are `[E0001] unknown escape: \u`; the accepted escapes are
`\n \t \r \\ \" \{ \}`, which `llms.txt` does not list. There is no `chr`/`ord` either, so
a byte-order mark or a zero-width character can only be written raw (invisible in source)
or built with `json.parse("\"\\ufeff\"")`, as csv_normalize does.

Suggested fix: support `\u{XXXX}` (and `\uXXXX`), add `chr(n)`/`ord(s)`, and list the
escapes in `llms.txt`.

**Fixed:** `"..."` literals accept `\u{XXXX}` (1 to 6 hex digits) and `\uXXXX` (exactly 4;
a UTF-16 surrogate pair `\uD83D\uDE00` is one character, as in JSON and JavaScript). A lone
surrogate, a value above `10FFFF` or a malformed escape is a syntax error that says how to
write it, and an unknown escape lists the valid ones. `chr(n)` and `ord(s)` are builtins on
both engines (registry, type checker signatures `chr(Int) -> String`, `ord(String) -> Int`;
`semantics::text` holds the rules, so invalid arguments are the same E0015 on both, with the
argument type appended centrally). On the way: `\{` and `\}` produced a brace that still
started an interpolation (`"\{x\}"` printed the value of `x`); escaped braces (also
`\u{7B}`/`\u{7D}`) are now literal (the lexer marks them with the noncharacters U+FDD0/U+FDD1,
which a literal therefore cannot contain). `llms.txt` and the spec list every escape.
csv_normalize's reference now writes the byte-order mark as `"\u{feff}"`. Tests:
`lexer::tests::test_unicode_escapes`, `test_invalid_escapes`, `semantics::text::tests`,
`tests/unicode_escapes_test.fg` and `tests/edition2027/unicode_escapes_test.fg` (both engines).

### 20. `select` cannot report a closed channel

```forge
let a = channel()
send(a, 1)
close(a)
println(select([a], 1000))   // [0, 1]
println(select([a], 1000))   // null, after waiting the full second
```

`select` skips closed channels and returns `null` on timeout and when every channel is
closed, and there is no `is_closed(ch)`; `try_receive` returns `None` for both empty and
closed. A consumer merging producers cannot tell "finished" from "slow" without an
in-band end marker (merge_streams specifies one).

Suggested fix: return `[index, null]` (or a `Closed` marker) once for a channel that is
closed and drained, or add `is_closed(ch)`; document it next to `select`.

**Fixed:** `is_closed(ch)` (registry, both engines, type checker) is true once a channel
is closed *and* drained, so "finished" can never be observed while a value is still
buffered (a channel receiver with one value of lookahead, `semantics::channel`, so asking
consumes nothing). `select`'s results are unchanged (backward compatible: `[index, value]`
or `null`), but it now returns `null` at once on both engines when every channel is
finished (the interpreter used to wait out the timeout, or forever without one, as in the
repro's second call); after a `null`, `all(chs, is_closed)` tells finished from slow.
`[index, null]` was rejected because `null` is a valid channel value. Documented in
`llms.txt` (with a merge loop) and `spec/src/concurrency/channels.md`; tests in
`tests/channel_closed_test.fg` (both engines).
