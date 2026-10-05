//! Stable runtime error codes (`E0000` ...).
//!
//! Both engines raise runtime errors as plain message strings, built by the
//! shared helpers in [`crate::semantics`] (and a few stdlib modules). This
//! module is the **one table** that gives every error class a stable code,
//! a short title, a default one-line hint and a long explanation (shown by
//! `forge explain E0009`). The code of an error is derived from its message
//! by [`classify`], so the interpreter and the VM report the same code
//! whenever they report the same message — and the parity tests
//! (`tests/error_codes.rs`) check that they do.
//!
//! Rules:
//!
//! * Codes are part of Forge's public surface (CLI output, `--error-format
//!   json`, `forge check`, the MCP server, `catch e { e.code }`). A code is
//!   never reused for a different problem and never renumbered; add new
//!   codes at the end of [`RUNTIME_ERRORS`].
//! * Every code has a fixture under `tests/errors/` that triggers it on both
//!   engines (enforced by `tests/error_codes.rs`), except codes the runtime
//!   cannot be made to raise deterministically from a script; those are
//!   listed in `UNREACHABLE_FROM_FIXTURES` there with a reason.
//! * Error *text* is produced by the helpers next to each rule (for
//!   example [`crate::semantics::index_out_of_bounds`]); the matcher here
//!   keys on the stable prefix those helpers emit. A unit test below
//!   asserts that each helper's output classifies to its code, so a wording
//!   change that would silently re-code an error fails the build.
//!
//! The message convention shared by every helper: the first line is the
//! headline; an optional line starting with `  hint: ` carries the hint.

/// One runtime error class.
#[derive(Debug)]
// `title` and `explanation` are read by `forge explain` (the binary only).
#[allow(dead_code)]
pub struct ErrorCode {
    /// Stable identifier, `E` + four digits.
    pub code: &'static str,
    /// Short lowercase title (`"index out of bounds"`).
    pub title: &'static str,
    /// One-line hint shown when the message itself carries none.
    pub hint: &'static str,
    /// Long explanation for `forge explain`: what happened, an example
    /// and the fix.
    pub explanation: &'static str,
    /// Whether a message (its headline, without the hint) belongs to this
    /// class. Checked in table order; the first match wins.
    matches: fn(&str) -> bool,
}

/// Prefix of the hint line in error messages (`"...\n  hint: ..."`).
pub const HINT_PREFIX: &str = "  hint: ";

/// The generic code for messages no other entry recognises (errors raised
/// with custom text by stdlib code that has no dedicated class).
pub const GENERIC: &ErrorCode = &RUNTIME_ERRORS[0];

fn starts_with_any(h: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| h.starts_with(p))
}

/// `name()` / `module.name()` at the start of a headline: the error comes
/// from a builtin. Returns the rest after `() `.
fn builtin_prefix(h: &str) -> Option<&str> {
    let open = h.find("()")?;
    let name = &h[..open];
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    Some(&h[open + 2..])
}

fn is_builtin_argument_error(h: &str) -> bool {
    let Some(rest) = builtin_prefix(h) else {
        return h.starts_with("expected ") && h.ends_with(" argument");
    };
    [
        "require", "expects", "takes", "must", "argument", " arg ", "needs", "accepts", "count",
        "size",
    ]
    .iter()
    .any(|w| rest.contains(w))
}

fn is_stdlib_failure(h: &str) -> bool {
    if builtin_prefix(h).is_some() {
        return true;
    }
    // `fs.read error: ...`, `http.get error: ...`, `download failed: ...`
    let head = h.split(':').next().unwrap_or("");
    !head.contains(' ') && head.contains('.') && h.contains(" error")
        || starts_with_any(
            h,
            &[
                "download failed",
                "download error",
                "crawl error",
                "write error",
                "ask error",
                "cwd error",
            ],
        )
        || h.split_once(' ')
            .is_some_and(|(first, rest)| first.contains('.') && rest.starts_with("error:"))
}

/// The table, in code order. [`classify`] tries the entries in this order
/// (first match wins), except the broad builtin classes in
/// `FALLBACK_CODES`, which are tried last.
pub static RUNTIME_ERRORS: &[ErrorCode] = &[
    ErrorCode {
        code: "E0000",
        title: "runtime error",
        hint: "see the message above; run with --interp for a second opinion if it looks like an engine bug",
        explanation: "\
A runtime error that has no more specific code. Most errors raised by the
standard library with custom text (for example a failed `json.parse`) land
here, as do errors created by your own code with a custom message.

Example:

    let data = json.parse(\"{not json\")

Fix: read the message — it names the operation that failed. Wrap code that
can fail in `try { ... } catch e { ... }` (the caught object has `message`,
`type` and `code` fields) or use a Result-returning API with `?`.",
        matches: |_| false,
    },
    ErrorCode {
        code: "E0001",
        title: "invalid token",
        hint: "check for an unterminated string, a stray character or a malformed number",
        explanation: "\
The lexer found text that is not a Forge token: an unterminated string, a
character that has no meaning in Forge, or a malformed number literal.

Example:

    say \"hello

Fix: close the string (`say \"hello\"`), remove the stray character, or fix
the literal. The caret in the snippet points at where scanning stopped.",
        matches: |_| false,
    },
    ErrorCode {
        code: "E0002",
        title: "syntax error",
        hint: "check the line above for a missing `}`, `)` or `,`",
        explanation: "\
The parser could not make sense of the token sequence: a missing closing
brace or parenthesis, a keyword in the wrong place, or an incomplete
expression.

Example:

    fn add(a, b) {
        return a +
    }

Fix: complete the expression (`return a + b`). When the reported line looks
fine, the real problem is usually an unclosed `{`, `(` or `[` earlier.",
        matches: |_| false,
    },
    ErrorCode {
        code: "E0003",
        title: "undefined name",
        hint: "define the name before using it, or check its spelling",
        explanation: "\
A variable or function name was read, but nothing with that name is defined
in the current scope, an enclosing scope, or the globals (builtins, stdlib
modules, top-level definitions). When a defined name is close in spelling,
the message suggests it.

Example:

    let count = 1
    say coutn          // undefined variable: 'coutn' — did you mean 'count'?

Fix: correct the spelling, or define the name before the line that uses it.
Names defined inside a block or function are not visible outside it. The
type checker reports the same problem before the program runs (T0006).",
        matches: |h| h.starts_with("undefined variable") || h.starts_with("undefined: "),
    },
    ErrorCode {
        code: "E0004",
        title: "value is not callable",
        hint: "only functions and lambdas can be called; check that the name is not shadowed by a variable",
        explanation: "\
A call `f(...)` was made on a value that is not a function: a number,
string, object, null, ... This usually means a variable shadows a function
of the same name, or a field holds data instead of a function.

Example:

    let total = 5
    total()            // cannot call a value of type Int

Fix: call the function you meant, or rename the variable that shadows it.
The type checker warns about this before running when it can (T0013).",
        matches: |h| {
            h.starts_with("cannot call a value")
                || h == "cannot call non-function"
                || h == "null function"
        },
    },
    ErrorCode {
        code: "E0005",
        title: "wrong number of arguments",
        hint: "match the call to the function's parameter list",
        explanation: "\
A function, lambda or method was called with fewer arguments than it
requires or more than it accepts. Parameters with default values may be
left out. (Callbacks passed to builtins such as `map` and `filter` are
exempt: missing parameters become null and extra arguments are ignored.)

Example:

    fn add(a, b) { return a + b }
    add(1)             // fn add expects 2 arguments, got 1

Fix: pass every required argument, or give the parameter a default:
`fn add(a, b = 0) { ... }`. Builtins report the same problem as
`len() expects 1 argument, got 2`.",
        matches: |h| {
            (h.starts_with("fn ") || h.starts_with("method ")) && h.contains(" expects ")
                || builtin_prefix(h).is_some_and(|rest| {
                    let rest = rest.trim_start();
                    rest.starts_with("expects ") && rest.contains("argument") && rest.contains(", got ")
                })
        },
    },
    ErrorCode {
        code: "E0006",
        title: "assignment to an immutable variable",
        hint: "declare the variable with `let mut` (or `set mut`) to allow reassignment",
        explanation: "\
A variable declared without `mut` was reassigned. Forge bindings are
immutable by default so that values do not change behind your back.

Example:

    let x = 1
    x = 2              // cannot reassign immutable variable 'x'

Fix: declare it mutable: `let mut x = 1` (natural syntax: `set mut x to 1`),
or bind a new name instead of reassigning. Reported statically as T0017.",
        matches: |h| h.starts_with("cannot reassign immutable variable"),
    },
    ErrorCode {
        code: "E0007",
        title: "operator applied to unsupported types",
        hint: "convert the operands first, e.g. int(\"3\"), str(n) or float(n)",
        explanation: "\
An arithmetic, comparison or unary operator was applied to values it does
not support: `null + 1`, `\"a\" * 3`, `true < false`, `[1] - [2]`, ...
Numbers mix freely (an Int and a Float give a Float) and `+` with a string
on either side concatenates, but no other operator converts implicitly.

Example:

    let n = \"3\"
    say n * 2          // invalid operator

Fix: convert explicitly — `int(n) * 2`, `str(count) + \" items\"` — or check
for null before doing arithmetic. Reported statically as T0005.",
        matches: |h| {
            starts_with_any(
                h,
                &[
                    "cannot apply ",
                    "invalid operator",
                    "cannot perform arithmetic on null",
                    "cannot negate",
                    "tuples only support",
                    "sets only support",
                    "maps only support",
                    "invalid comparison",
                    "`?` expects",
                ],
            )
        },
    },
    ErrorCode {
        code: "E0008",
        title: "division by zero",
        hint: "check that the divisor is not zero before dividing",
        explanation: "\
An integer was divided (`/`) or reduced (`%`) by zero. Integer division by
zero has no result, so Forge stops instead of producing a wrong number.
(Float division follows IEEE 754 and yields infinity or NaN.)

Example:

    let per_item = total / count      // count is 0

Fix: guard the division — `if count != 0 { total / count } else { 0 }` —
or divide floats when infinity is an acceptable answer.",
        matches: |h| h.starts_with("division by zero") || h.starts_with("modulo by zero"),
    },
    ErrorCode {
        code: "E0009",
        title: "index out of bounds",
        hint: "check the index against len() before indexing",
        explanation: "\
An array, tuple or string was indexed past its end. Valid indices for a
sequence of length n are 0 to n-1, and -n to -1 counting from the end.
The message gives the index and the length.

Example:

    let a = [1, 2, 3]
    say a[3]           // index out of bounds: index 3 on array of length 3

Fix: index with `len(a) - 1` (or `-1`) for the last element, check
`if i < len(a)` first, or iterate with `for x in a` instead of indices.",
        matches: |h| h.starts_with("index out of bounds"),
    },
    ErrorCode {
        code: "E0010",
        title: "missing key",
        hint: "check with has_key(obj, key) or use get(obj, key, default)",
        explanation: "\
An object or map was indexed with a key it does not contain
(`obj[\"key\"]`). Indexing is strict so that typos do not silently yield
null.

Example:

    let m = {a: 1}
    say m[\"b\"]          // key 'b' not found

Fix: use `get(m, \"b\", 0)` for a default, test with `has_key(m, \"b\")`, or
fix the key's spelling.",
        matches: |h| h.starts_with("key '") && h.contains("' not found"),
    },
    ErrorCode {
        code: "E0011",
        title: "value cannot be indexed",
        hint: "index arrays and strings with integers, objects and maps with keys",
        explanation: "\
`container[index]` (or `container[index] = value`) was used with a
container/index combination that does not support it: indexing a number,
indexing an array with a string, assigning into a tuple or a set, ...

Example:

    let n = 42
    say n[0]           // invalid index operation: cannot index Int with Int

Fix: index only arrays, tuples and strings (with integers) or objects and
maps (with keys). Sets are unordered — use `.has(x)` or iterate.",
        matches: |h| {
            starts_with_any(
                h,
                &[
                    "invalid index operation",
                    "cannot index",
                    "cannot mutate a tuple",
                    "can only assign to variable indices",
                    "can only assign to variable fields",
                    "invalid assignment target",
                ],
            )
        },
    },
    ErrorCode {
        code: "E0012",
        title: "field access on null",
        hint: "the value is null here; check it with `if x != null` before reading its fields",
        explanation: "\
A field or method was read from `null`. The value usually came from a
function that returned nothing (a missing `return`), a lookup that found
nothing, or a variable that was never given a value.

Example:

    fn find_user(id) { if id == 1 { return {name: \"Ada\"} } }
    let user = find_user(2)
    say user.name      // cannot access field 'name' on Null

Fix: handle the missing case — `if user != null { say user.name }` — or
make the function return an Option/Result so callers must handle it.",
        matches: |h| h.starts_with("cannot access field") && h.ends_with(" on Null"),
    },
    ErrorCode {
        code: "E0013",
        title: "unknown field or method",
        hint: "check the spelling; print the value with sus(x) to see what it contains",
        explanation: "\
A field or method that the value does not have was accessed. For objects
the message suggests a field with a similar name when there is one.

Example:

    let user = {name: \"Ada\"}
    say user.nmae      // no field 'nmae' on object — did you mean 'name'?

Fix: correct the name. To read a field that may be absent, use
`get(user, \"nickname\", \"none\")` or `has_key(user, \"nickname\")`.",
        matches: |h| {
            starts_with_any(h, &["no field '", "no method '", "unknown method"])
                || (h.starts_with("cannot call '") && h.contains("' on "))
        },
    },
    ErrorCode {
        code: "E0014",
        title: "field access on a non-object",
        hint: "fields can only be read from objects and struct values",
        explanation: "\
A field was read from or written to a value that has no fields: a number,
a boolean, a function, ...

Example:

    let n = 5
    say n.value        // cannot access field 'value' on Int

Fix: access fields only on objects and struct instances; check what the
variable holds (`say typeof(n)`).",
        matches: |h| h.starts_with("cannot access field") || h.starts_with("cannot set field"),
    },
    ErrorCode {
        code: "E0015",
        title: "invalid argument to a builtin",
        hint: "check the argument types the builtin expects (see `forge doc` or llms.txt)",
        explanation: "\
A builtin function or stdlib member was called with an argument of the
wrong type or an out-of-range value. The message names the builtin, what it
expects and (in parentheses) the types it got.

Example:

    say len(5)         // len() requires string, array, tuple, set, map, or object (got Int)

Fix: pass the expected type, converting first when needed (`str(5)`,
`int(\"5\")`, `[x]`).",
        matches: is_builtin_argument_error,
    },
    ErrorCode {
        code: "E0016",
        title: "standard library operation failed",
        hint: "the operation itself failed (file, network, parse, ...); see the message for the cause",
        explanation: "\
A builtin or stdlib member received valid arguments but the operation
failed: a file that does not exist, a network error, malformed JSON, ...

Example:

    let text = fs.read(\"missing.txt\")     // fs.read error: No such file or directory

Fix: handle the failure — check first (`fs.exists(path)`), or wrap the call
in `try { ... } catch e { ... }`.",
        matches: is_stdlib_failure,
    },
    ErrorCode {
        code: "E0017",
        title: "value does not match its type annotation",
        hint: "pass a value of the annotated type, or loosen the annotation",
        explanation: "\
Under `forge run --strict`, annotated parameters and return types are also
checked while the program runs. A value that does not fit its annotation
stops the program at the call or return.

Example:

    fn double(x: Int) -> Int { return x * 2 }
    double(\"2\")        // type error: argument 'x' of 'double' must be Int, got String

Fix: convert the value (`double(int(\"2\"))`) or change the annotation.",
        matches: |h| h.starts_with("type error: "),
    },
    ErrorCode {
        code: "E0018",
        title: "maximum recursion depth exceeded",
        hint: "check for infinite recursion, or restructure the algorithm to use a loop",
        explanation: "\
A chain of function calls went deeper than the recursion limit (10000 by
default) or the native stack. This is almost always unbounded recursion: a
missing or unreachable base case.

Example:

    fn countdown(n) { return countdown(n - 1) }   // no base case
    countdown(10)

Fix: add the base case (`if n == 0 { return 0 }`), or turn the recursion
into a loop. A legitimately deep algorithm can raise the limit with
`FORGE_MAX_DEPTH` or `--max-depth`.",
        matches: |h| h.starts_with("maximum recursion depth exceeded"),
    },
    ErrorCode {
        code: "E0019",
        title: "import failed",
        hint: "check the import path (relative to the importing file) and the exported names",
        explanation: "\
An `import` could not be completed: the file was not found, it does not
export the requested name, it failed to parse, or modules import each
other in a cycle.

Example:

    import { helper } from \"utils\"     // cannot import 'utils': file not found

Fix: check the path — it is resolved relative to the importing file, and
`utils`, `utils.fg` and `forge_modules/utils/main.fg` are tried. Only
top-level `fn`, `let`, `type` and `struct` definitions can be imported.
Break import cycles by moving shared code into a third module.",
        matches: |h| {
            starts_with_any(h, &["cannot import", "circular import"])
                || (h.starts_with("import '")
                    && (h.contains("does not export")
                        || h.contains("parse error")
                        || h.contains("lex error")))
        },
    },
    ErrorCode {
        code: "E0020",
        title: "permission denied",
        hint: "grant the capability with the --allow-* flag named in the message or in forge.toml [permissions]",
        explanation: "\
The program used a capability (files, network, environment, subprocesses,
databases, AI, native plugins) that the current policy does not grant.
`forge run` grants most capabilities by default but not subprocesses;
sandboxes (`forge mcp`, `[permissions] sandbox = true`, embedders) deny
everything not granted.

Example:

    sh(\"ls\")           // permission denied: run — run with --allow-run

Fix: pass the flag named in the message (`--allow-run`, `--allow-read=./data`,
`--allow-net=api.example.com`, ...) or add it to `[permissions]` in
forge.toml. Grant the narrowest scope that works.",
        matches: |h| h.starts_with("permission denied"),
    },
    ErrorCode {
        code: "E0021",
        title: "assertion failed",
        hint: "the asserted condition was false; the message shows the values involved",
        explanation: "\
An `assert`, `assert_eq`, `assert_ne`, `assert_throws` (or the GenZ kit's
`bet`, `no_cap`, `ick`) check failed. In `forge test` this fails the test.

Example:

    assert_eq(add(2, 2), 5)    // assertion failed: expected `5`, got `4`

Fix: the code under test or the expectation is wrong — the message shows
both values.",
        matches: |h| {
            starts_with_any(
                h,
                &["assertion failed", "LOST THE BET", "CAP DETECTED", "ICK:"],
            )
        },
    },
    ErrorCode {
        code: "E0022",
        title: "check failed",
        hint: "the value did not satisfy the `check` rule",
        explanation: "\
A declarative `check` statement rejected a value.

Example:

    let name = \"\"
    check name is not empty    // check failed: \"\" did not pass validation

Fix: validate input before it reaches the check, or handle the failure with
`try { ... } catch e { ... }`.",
        matches: |h| h.starts_with("check failed"),
    },
    ErrorCode {
        code: "E0023",
        title: "unwrap of a missing value",
        hint: "handle the Err/None case with unwrap_or(x, default), match, or `?`",
        explanation: "\
`unwrap(x)`, `unwrap_err(x)` or `must expr` was used on a value that does not
hold what was asked for: `unwrap(None)`, `unwrap(Err(...))`, `must null`.

Example:

    let port = unwrap(env.get(\"PORT\"))    // unwrap() called on None

Fix: provide a fallback — `unwrap_or(env.get(\"PORT\"), \"8080\")` — or
`match` on the value and handle both cases.",
        matches: |h| {
            starts_with_any(
                h,
                &[
                    "unwrap() called on",
                    "unwrap_err() called on",
                    "must failed",
                    "null reference",
                ],
            )
        },
    },
    ErrorCode {
        code: "E0024",
        title: "unhandled error value",
        hint: "handle the Err with match / unwrap_or, or let a calling function propagate it with `?`",
        explanation: "\
An `Err(...)` (or another value) was propagated with `?` all the way out of
the program: no caller handled it.

Example:

    fn load() { return Err(\"no config\") }
    let cfg = load()?          // unhandled error: no config

Fix: handle the error where it can be handled — `match load() { Ok(c) =>
..., Err(e) => ... }` — or provide a default with `unwrap_or`.",
        matches: |h| starts_with_any(h, &["unhandled error", "unhandled propagated value"]),
    },
    ErrorCode {
        code: "E0025",
        title: "explicit panic",
        hint: "the program stopped itself with bruh(); see the message for why",
        explanation: "\
The program called `bruh(message)` to stop immediately.

Example:

    if config == null { bruh(\"config missing\") }     // BRUH: config missing

Fix: this is intentional — address the condition the message describes.",
        matches: |h| h.starts_with("BRUH"),
    },
    ErrorCode {
        code: "E0026",
        title: "non-exhaustive match",
        hint: "add a `_ => ...` arm to handle every other value",
        explanation: "\
A `match` statement ran with a value that none of its arms matched.

Example:

    match status { \"ok\" => say \"fine\" }      // status is \"error\"

Fix: add the missing arms, or a wildcard arm: `_ => say \"unexpected\"`.
The type checker reports missing ADT variants statically (T0011).",
        matches: |h| h.starts_with("non-exhaustive match"),
    },
    ErrorCode {
        code: "E0027",
        title: "control flow outside its construct",
        hint: "use break/continue only inside loops and return only inside functions",
        explanation: "\
`break` or `continue` ran outside any loop (for example inside a function
called from a loop), or `return` ran outside a function.

Example:

    fn stop() { break }
    for i in range(3) { stop() }      // break outside of loop

Fix: return a value from the function and `break` in the loop itself:
`if should_stop() { break }`.",
        matches: |h| {
            starts_with_any(
                h,
                &[
                    "break outside of loop",
                    "continue outside of loop",
                    "return outside of a function",
                ],
            )
        },
    },
    ErrorCode {
        code: "E0028",
        title: "destructuring failed",
        hint: "destructure arrays with [..], objects with {..} and tuples with (..)",
        explanation: "\
A destructuring pattern did not fit the value: `unpack {a} from` a
non-object, `unpack [x] from` a non-array, `let (a, b) =` a non-tuple.

Example:

    unpack {name} from 42      // cannot destructure non-object

Fix: destructure the right kind of value, or check its type first.",
        matches: |h| h.starts_with("cannot destructure"),
    },
    ErrorCode {
        code: "E0029",
        title: "mutation of a frozen value",
        hint: "frozen values are read-only; copy the value instead of mutating it",
        explanation: "\
A value created with `freeze` was modified.

Example:

    let config = freeze({debug: false})
    config.debug = true        // cannot mutate a frozen value

Fix: build a modified copy (`merge(config, {debug: true})`) instead.",
        matches: |h| {
            starts_with_any(
                h,
                &[
                    "cannot mutate a frozen",
                    "cannot modify frozen",
                    "cannot mutate frozen",
                ],
            )
        },
    },
    ErrorCode {
        code: "E0030",
        title: "channel closed",
        hint: "a channel cannot be used after close(); check the sender/receiver lifecycle",
        explanation: "\
A value was sent on, or received from, a channel that has been closed (and
drained).

Example:

    let ch = channel()
    close(ch)
    send(ch, 1)                // channel closed

Fix: close a channel only after the last send, and stop receiving once
`receive` reports the end (or iterate with `for x in ch`).",
        matches: |h| h.starts_with("channel closed") || h.contains("closed channel"),
    },
    ErrorCode {
        code: "E0031",
        title: "cancelled or out of time",
        hint: "the work exceeded a time limit or was cancelled; raise the limit or do less work per call",
        explanation: "\
The running code was stopped from outside: a `timeout N seconds { }` block
expired, `--max-time` or a sandbox limit was reached, or a task/request was
cancelled (for example because an HTTP client disconnected).

Example:

    timeout 1 seconds { wait(5) }    // timeout: operation exceeded 1 second limit

Fix: raise the limit, make the work faster, or split it into smaller steps.",
        matches: |h| {
            starts_with_any(
                h,
                &["timeout", "cancelled", "task cancelled", "execution exceeded"],
            ) || h.contains("exceeded the limit")
                || h.contains("limit exceeded")
        },
    },
    ErrorCode {
        code: "E0032",
        title: "unsupported feature",
        hint: "this construct is recognised but not implemented yet; see the message for an alternative",
        explanation: "\
The program used a construct that the language reserves but does not
implement yet, such as `yield` / `emit` (generators).

Example:

    fn numbers() { yield 1 }   // yield/emit is not supported yet

Fix: follow the hint in the message — for generators, collect values into
an array and return it, or use `.stream()` for lazy iteration.",
        matches: |h| h.contains("is not supported yet") || h.starts_with("unsupported"),
    },
    ErrorCode {
        code: "E0033",
        title: "stream misuse",
        hint: "streams are single-use; create a new stream with .stream() for each pass",
        explanation: "\
A lazy stream was advanced while it was already being advanced. Streams
guard against this internally (a stream's state is borrowed while it
produces a value); ordinary programs cannot normally reach it, so seeing it
usually means a stream is shared between concurrent tasks.

Example:

    let s = [1, 2, 3].stream()
    // advancing `s` from two tasks at the same time

Fix: give each task its own stream (call `.stream()` per task), or collect
it into an array first (`let xs = s.collect()`) and share the array.",
        matches: |h| h.starts_with("stream already in use") || h.starts_with("stream "),
    },
];

/// The code for a runtime error message.
pub fn classify(message: &str) -> &'static ErrorCode {
    let headline = headline(message);
    // `task error: <inner>` / `squad task error: <inner>` wrap the error of
    // a spawned task: classify the inner error.
    let inner = ["squad task error: ", "task error: "]
        .iter()
        .find_map(|p| headline.strip_prefix(p))
        .unwrap_or(headline);
    // The broad builtin classes (anything starting with `name()`) are
    // tried after every specific class, so `unwrap() called on None` is
    // E0023, not "invalid argument to a builtin".
    let is_fallback = |e: &&ErrorCode| FALLBACK_CODES.contains(&e.code);
    RUNTIME_ERRORS
        .iter()
        .skip(1)
        .filter(|e| !is_fallback(e))
        .chain(RUNTIME_ERRORS.iter().filter(is_fallback))
        .find(|e| (e.matches)(inner))
        .unwrap_or(GENERIC)
}

/// Classes matched only after all others (see [`classify`]).
const FALLBACK_CODES: &[&str] = &["E0015", "E0016"];

/// Look up a runtime code (`"E0009"`, case-insensitive).
pub fn lookup(code: &str) -> Option<&'static ErrorCode> {
    RUNTIME_ERRORS
        .iter()
        .find(|e| e.code.eq_ignore_ascii_case(code))
}

/// First line of a message.
pub fn headline(message: &str) -> &str {
    message.lines().next().unwrap_or(message)
}

/// The hint carried by the message itself, if any.
pub fn message_hint(message: &str) -> Option<&str> {
    message
        .lines()
        .find_map(|l| l.strip_prefix(HINT_PREFIX))
        .map(str::trim)
}

/// The hint to show for a message: its own, else its code's default.
pub fn hint_for(message: &str) -> &str {
    message_hint(message).unwrap_or(classify(message).hint)
}

/// The legacy `type` field of a caught error object (`catch e { e.type }`).
/// Kept for compatibility; new code should prefer `e.code`. Only the
/// headline is inspected, so words in a hint (a field named `type` in a
/// did-you-mean) cannot change it.
pub fn legacy_error_type(message: &str) -> &'static str {
    let message = headline(message);
    if message.contains("type") || message.contains("Type") {
        "TypeError"
    } else if message.contains("division by zero") || message.contains("modulo by zero") {
        "ArithmeticError"
    } else if message.contains("assertion") {
        "AssertionError"
    } else if message.contains("index") || message.contains("out of bounds") {
        "IndexError"
    } else if message.contains("not found") || message.contains("undefined") {
        "ReferenceError"
    } else if message.contains("immutable") || message.contains("cannot reassign") {
        "TypeError"
    } else {
        "RuntimeError"
    }
}

/// How many leading arguments [`annotate_builtin_error`] describes.
pub const MAX_ANNOTATED_ARGS: usize = 4;

/// Append the argument types to a builtin's argument error so the user
/// sees what was actually passed: `len() requires ... (got Int)`.
///
/// Applied by both engines' builtin dispatch to errors that [`classify`]
/// as E0015, start with `name()` and do not already mention what they got.
/// `arg_types` are user-facing type names (see [`user_type_name`]).
pub fn annotate_builtin_error(name: &str, message: &str, arg_types: &[&str]) -> Option<String> {
    let head = headline(message);
    if arg_types.is_empty()
        || !head.starts_with(&format!("{}()", name))
        || head.contains("got ")
        || classify(message).code != "E0015"
    {
        return None;
    }
    let got = format!("{} (got {})", head, arg_types.join(", "));
    Some(match message.split_once('\n') {
        Some((_, rest)) => format!("{}\n{}", got, rest),
        None => got,
    })
}

/// Normalise an engine's type name to the user-facing name used in error
/// messages, so both engines describe the same value the same way.
pub fn user_type_name(engine_name: &str) -> &'static str {
    match engine_name {
        "Int" => "Int",
        "Float" => "Float",
        "String" => "String",
        "Bool" => "Bool",
        "Null" => "Null",
        "Array" => "Array",
        "Tuple" => "Tuple",
        "Set" => "Set",
        "Map" => "Map",
        "Object" => "Object",
        "Result" => "Result",
        "Option" => "Option",
        "Stream" => "Stream",
        "Channel" => "Channel",
        "TaskHandle" => "TaskHandle",
        "Function" | "Lambda" | "BuiltIn" | "Closure" => "Function",
        _ => "Value",
    }
}

/// "Did you mean ...?" for a name that is not defined. `groups` lists the
/// visible names innermost scope first (the globals last); within a group
/// order does not matter. The closest name wins; ties go to the inner
/// group, then to the alphabetically first name — so the answer depends
/// only on which names are visible, not on hash-map iteration order, and
/// both engines agree.
pub fn suggest_name<'a, G, I>(name: &str, groups: G) -> Option<String>
where
    G: IntoIterator<Item = I>,
    I: IntoIterator<Item = &'a str>,
{
    let mut ordered: Vec<&str> = Vec::new();
    for group in groups {
        let mut names: Vec<&str> = group.into_iter().collect();
        names.sort_unstable();
        ordered.extend(names);
    }
    crate::typechecker::suggest::closest(name, ordered).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_dense_and_documented() {
        let mut seen = HashSet::new();
        for (i, e) in RUNTIME_ERRORS.iter().enumerate() {
            assert!(seen.insert(e.code), "duplicate {}", e.code);
            assert_eq!(e.code, format!("E{:04}", i), "codes are numbered densely");
            assert!(!e.title.is_empty() && !e.hint.is_empty(), "{}", e.code);
            assert!(
                e.explanation.contains("Example") && e.explanation.contains("Fix"),
                "{} explanation needs an example and a fix",
                e.code
            );
            assert!(!e.hint.contains('\n'), "{} hint must be one line", e.code);
        }
    }

    /// Every shared message helper classifies to its code. A wording change
    /// that would re-code an error fails here.
    #[test]
    fn helpers_classify_to_their_codes() {
        use crate::semantics as s;
        let cases: Vec<(String, &str)> = vec![
            (s::undefined_variable("x", None), "E0003"),
            (s::undefined_variable("x", Some("y")), "E0003"),
            (s::not_callable("Int"), "E0004"),
            (s::check_call_arity("f", 2, 2, 1).unwrap_err(), "E0005"),
            (s::check_method_arity("m", 2, 2, 0).unwrap_err(), "E0005"),
            (
                crate::builtins_registry::check_arity("len", 2).unwrap_err(),
                "E0005",
            ),
            (s::immutable_reassign("x"), "E0006"),
            (
                s::binary(s::BinaryOp::Sub, s::Operand::Str("a"), s::Operand::Int(1)).unwrap_err(),
                "E0007",
            ),
            (
                s::binary(s::BinaryOp::Add, s::Operand::Null, s::Operand::Int(1)).unwrap_err(),
                "E0007",
            ),
            (s::DIVISION_BY_ZERO.to_string(), "E0008"),
            (s::MODULO_BY_ZERO.to_string(), "E0008"),
            (s::index_out_of_bounds(3, "array", 3), "E0009"),
            (s::missing_key("k"), "E0010"),
            (s::invalid_index("Int", "Int"), "E0011"),
            (s::invalid_index("Set", "Int"), "E0011"),
            (s::invalid_index_assign("Tuple"), "E0011"),
            (s::invalid_index_assign("Set"), "E0011"),
            (s::field_access("name", "Null"), "E0012"),
            (s::no_field("nmae", ["name"]), "E0013"),
            (s::field_access("x", "Int"), "E0014"),
            (
                "len() requires string, array, tuple, set, map, or object".to_string(),
                "E0015",
            ),
            ("fs.read error: No such file".to_string(), "E0016"),
            (
                "type error: argument 'x' of 'f' must be Int, got String".to_string(),
                "E0017",
            ),
            (
                crate::runtime::recursion::depth_exceeded_message(5),
                "E0018",
            ),
            (s::import_not_found("utils"), "E0019"),
            (s::import_missing_name("utils", "x"), "E0019"),
            ("circular import: a.fg -> b.fg -> a.fg".to_string(), "E0019"),
            (
                "permission denied: run — run with --allow-run".to_string(),
                "E0020",
            ),
            (
                "assertion failed: expected `5`, got `4`".to_string(),
                "E0021",
            ),
            (s::check_failed("\"\""), "E0022"),
            ("unwrap() called on None".to_string(), "E0023"),
            ("must failed: bad".to_string(), "E0023"),
            (s::unhandled_error("bad"), "E0024"),
            (s::TRY_REQUIRES_RESULT.to_string(), "E0007"),
            ("BRUH: stop".to_string(), "E0025"),
            (s::NON_EXHAUSTIVE_MATCH.to_string(), "E0026"),
            (s::RETURN_OUTSIDE_FUNCTION.to_string(), "E0027"),
            ("break outside of loop".to_string(), "E0027"),
            ("cannot destructure non-object".to_string(), "E0028"),
            ("cannot mutate a frozen value".to_string(), "E0029"),
            ("channel closed".to_string(), "E0030"),
            (
                "timeout: operation exceeded 1 second limit".to_string(),
                "E0031",
            ),
            ("cancelled".to_string(), "E0031"),
            (s::YIELD_UNSUPPORTED.to_string(), "E0032"),
            (
                "stream already in use (re-entrant advance)".to_string(),
                "E0033",
            ),
            (
                "task error: index out of bounds: index 1 on array of length 0".to_string(),
                "E0009",
            ),
            ("something odd happened".to_string(), "E0000"),
        ];
        for (message, code) in cases {
            assert_eq!(classify(&message).code, code, "{:?}", message);
        }
    }

    #[test]
    fn hints_come_from_the_message_or_the_table() {
        assert_eq!(
            hint_for(s_div()),
            "check that the divisor is not zero before dividing"
        );
        assert_eq!(hint_for("key 'b' not found"), lookup("E0010").unwrap().hint);
        assert_eq!(lookup("e0010").map(|e| e.code), Some("E0010"));
        assert!(lookup("E9999").is_none());
    }

    #[test]
    fn legacy_types_look_at_the_headline_only() {
        use crate::semantics as s;
        assert_eq!(legacy_error_type(s::MODULO_BY_ZERO), "ArithmeticError");
        assert_eq!(
            legacy_error_type(&s::index_out_of_bounds(1, "array", 0)),
            "IndexError"
        );
        assert_eq!(
            legacy_error_type(&s::undefined_variable("x", None)),
            "ReferenceError"
        );
        assert_eq!(
            legacy_error_type(&s::no_field("x", ["type"])),
            "RuntimeError"
        );
        assert_eq!(legacy_error_type(&s::immutable_reassign("x")), "TypeError");
    }

    fn s_div() -> &'static str {
        crate::semantics::DIVISION_BY_ZERO
    }

    #[test]
    fn builtin_errors_name_the_argument_types() {
        assert_eq!(
            annotate_builtin_error("len", "len() requires a string", &["Int"]).as_deref(),
            Some("len() requires a string (got Int)")
        );
        assert_eq!(
            annotate_builtin_error(
                "push",
                "push() first argument must be array\n  hint: h",
                &["Int", "Int"]
            )
            .as_deref(),
            Some("push() first argument must be array (got Int, Int)\n  hint: h")
        );
        // Not this builtin's own error (a callback failed inside map()).
        assert_eq!(
            annotate_builtin_error("map", "len() requires a string", &["Array"]),
            None
        );
        // Already says what it got.
        assert_eq!(
            annotate_builtin_error("len", "len() expects 1 argument, got 2", &["Int", "Int"]),
            None
        );
        // Not an argument error.
        assert_eq!(
            annotate_builtin_error("fs.read", "fs.read error: gone", &["String"]),
            None
        );
    }

    #[test]
    fn suggestions_prefer_inner_scopes_and_are_order_independent() {
        let locals = ["total"];
        let globals = ["toml", "println"];
        assert_eq!(
            suggest_name("totl", [locals.to_vec(), globals.to_vec()]).as_deref(),
            Some("total")
        );
        assert_eq!(
            suggest_name("totl", [vec![], vec!["toml", "tota"]]).as_deref(),
            Some("toml")
        );
        assert_eq!(
            suggest_name("totl", [vec![], vec!["tota", "toml"]]).as_deref(),
            Some("toml")
        );
        assert_eq!(suggest_name("zzzz", [globals.to_vec()]), None);
    }
}
