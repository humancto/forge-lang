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
| 5 | bug | A method cannot change its receiver: `it.n += 1` / `it.items.push(x)` in an `impl` method is silently lost | (any stateful struct) |
| 6 | bug | Type checker: a parameter declared with an interface type cannot be passed to another parameter of that interface (T0012 "struct 'Metric' does not satisfy interface 'Metric'"), an error in 2027 | typed_route_planner (**known failing**) |
| 7 | bug | VM: `import { Plugin } from "api.fg"` of an interface fails (E0019 "does not export"); the interpreter imports it | plugin_pipeline (**known failing** on the VM) |
| 8 | bug | An algebraic type's name cannot be imported (`import { Shape } from "m.fg"` → E0019, both engines), so `fn f(s: Shape)` in another module is T0015 (error in 2027) | stack_machine_modules (**known failing**) |
| 9 | bug | VM: a name a module imports cannot be imported from it (re-export through a package entry); the interpreter allows it | csvlite_package (**known failing** on the VM) |
| 10 | bug | `timeout 0.5 seconds { }` truncates the limit to 0 and fails at once ("exceeded 0 second limit"); there is no millisecond unit | retry_with_timeouts (workaround) |
| 11 | bug | Interpreter: a struct method named `push` is dispatched to the builtin (`E0015 push() first argument must be array`); the VM calls the method | (any struct with `push`) |
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

### 14. Exponent literals

```forge
println(1e2)
```

`[T0006] Warning: unknown name 'e2'` and then `[E0003] undefined variable: 'e2'`: the
lexer reads `1` and the name `e2`. Models write `1e9`, `2.5e-3` freely.

Suggested fix: lex `digits[.digits](e|E)[+-]digits` as a Float literal; if exponents are
deliberately unsupported, make `1e2` a syntax error that says so.

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

### 17. Unclassified runtime errors

Code E0000 (type `RuntimeError`) for `int("12x")`, `float("abc")`, any `json.parse` error,
an invalid regex and an exhausted `retry`. `null["x"]`, `5["x"]`, `"abc"[0]` are E0011,
which `llms.txt` does not list. `e.type` is `ReferenceError` for a missing key but
`RuntimeError` for an unknown field. A program that wants to tell "bad number" from "bad
JSON" from "bug" must match message prefixes.

Suggested fix: classify conversion errors (E0015 or a new code), JSON parse errors and
invalid patterns; list E0011 (and the codes error_code_report shows: E0016, E0021, E0026,
E0031) in `llms.txt`.

### 19. No Unicode escapes in string literals

`"\u00e9"` and `"\u{feff}"` are `[E0001] unknown escape: \u`; the accepted escapes are
`\n \t \r \\ \" \{ \}`, which `llms.txt` does not list. There is no `chr`/`ord` either, so
a byte-order mark or a zero-width character can only be written raw (invisible in source)
or built with `json.parse("\"\\ufeff\"")`, as csv_normalize does.

Suggested fix: support `\u{XXXX}` (and `\uXXXX`), add `chr(n)`/`ord(s)`, and list the
escapes in `llms.txt`.

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
