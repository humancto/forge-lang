# Type Checking

Every program is type-checked after parsing and before it runs. The checker is **gradual**: it infers types where the program determines them, checks values against the types the program *declares*, and treats everything else as `Any` — a type compatible with every other type. Unannotated code is never rejected for lacking annotations.

## Modes

| Mode | Static diagnostics | Runtime checks |
|---|---|---|
| default (`forge run app.fg`) | reported as **warnings**; the program still runs | none |
| `--strict` (`forge run --strict app.fg`) | reported as **errors**; the program does not run | annotated function arguments and results are checked on every call |

The same rules apply on every engine (`--interp`, the VM, `--jit`). Editors receive the same diagnostics through `forge lsp`, and AI agents through the `check_forge` tool of `forge mcp`.

## Types

| Type | Written | Values |
|---|---|---|
| `Int`, `Float`, `String`, `Bool`, `Null` | as shown (`int`, `str`, `boolean`, ... are accepted aliases) | primitives |
| `Any` | `Any` (also `Json`) | anything |
| `Object` | `Object` | object literals and struct instances |
| `Object<T>` | `Object<Int>`, `Object<[String]>` | objects whose values are all `T` (string keys): a typed dictionary |
| `[T]` | `[Int]`, `Array<T>` | arrays whose elements are `T` |
| `(A, B)` | `(Int, String)` | tuples |
| `Set<T>`, `Map<K, V>` | as shown | sets and `Map` values (`map()`); an object is never a `Map` |
| `?T` | `?Int`, `Option<Int>` | `T`, `null`, `None` or `Some(t)` |
| `Result<T, E>` | as shown | `Ok(t)` / `Err(e)` |
| `fn(A, B) -> R` | `fn(Int) -> Bool`, `fn()` | functions and lambdas |
| user types | `Point`, `Box<Int>`, `Shape` | structs, `type` definitions, interfaces |

`type Id = Int | String` declares a union alias; `type Shape = Circle(Float) | Rect(Float, Float)` declares an algebraic data type whose variants are constructors.

Assignability: `Any` fits everything in both directions; `Int` fits `Float` (but not the reverse); `?T` accepts `null`, `T` and `?U` when `U` fits `T`; a union accepts any of its members; function types are contravariant in parameters and covariant in results; collections compare element-wise; an `Object` and a struct instance are interchangeable. A struct satisfies an interface structurally: it must provide every interface member as a field or method, with matching parameter counts and return types.


### Typed objects

`Object<T>` is the annotation for an object used as a dictionary: string keys (as
in every object) and values of type `T`. An object literal or a parsed JSON
object fits it when every value fits `T`; internal fields (`__type__`, ...)
are not values.

```forge
let ages: Object<Int> = { ada: 36, alan: 41 }
fn total(counts: Object<Int>) -> Int {
    let mut sum = 0
    for k, v in counts {        // v: Int
        sum = sum + v
    }
    return sum
}
let ok = total(json.parse("""{"a": 1, "b": 2}"""))
let bad: Object<Int> = { a: 1, b: "x" }   // T0001: assigned Object<String>
```

The checker checks the values of an object literal against `T` (like the
elements of an array literal against `[T]`), types `o.key`, `o[key]` and
`for k, v in o` with `T`, and checks `o.key = v` / `o[key] = v`. An object
whose values it cannot see (`Object`, `json.parse(...)`) is accepted
statically; under `--strict` and from edition 2027, annotated function
arguments and results are checked at run time (`type error: argument
'counts' of 'total' must be Object<Int>, got Object`), and a declared
`Object<Float>` turns Int values into Floats. `Map<String, Int>` is the
`Map` value type and does not accept an object
(`T0001`, with a hint to write `Object<Int>`).

## Inference

The checker infers, without annotations:

* the type of every `let` from its value (`let n = 1` is `Int`, `let xs = [1, 2.5]` is `[Float]`);
* the result type of every function from its `return` statements and its final expression;
* the type arguments of generic functions at each call (`fn first<T>(xs: [T]) -> T` called with `[String]` returns `String`) and of generic structs from their fields;
* the parameters of lambdas from the function type they are passed as: in `map(names, fn(n) { n.upper() })` with `names: [String]`, `n` is a `String`;
* field types of struct values, element types of arrays and maps, the payloads of `Some`, `Ok`, `Err` and ADT variants in `match` arms;
* narrowed types after `if x != null`, `if is_some(x)`, `if is_ok(r)` and early returns.

A mutable variable without an annotation has the combined type of every value assigned to it anywhere in its scope: `let mut x = 0` later assigned `"done"` is `Any`, never a stale `Int`. Values are only ever checked against **declared** types — an inferred type never turns a later, different use into an error, because Forge arrays may hold mixed values and unannotated variables may change type.

## Diagnostics

Each diagnostic has a stable code, a precise source span, and often a help line and a quick fix (applied by editors as a code action).

| Code | Meaning | Example |
|---|---|---|
| T0001 | value does not match a declared type | `let n: Int = "a"` |
| T0002 | argument does not match the parameter type | `fn f(x: Int) {}` … `f("a")` |
| T0003 | returned value does not match the return type | `fn f() -> Int { "a" }` |
| T0004 | wrong number of arguments | `len(1, 2)` |
| T0005 | operator not defined for the operand types | `"a" - 1` |
| T0006 | unknown name (with "did you mean") | `say cuont` |
| T0007 | unknown field or method of a struct | `point.z` |
| T0008 | unknown member of a stdlib module | `math.sqr(2)` |
| T0009 | function with a return type can finish without returning | `fn f() -> Int { let y = 1 }` |
| T0010 | unreachable code | statements after `return` |
| T0011 | non-exhaustive `match` (Option, Result, Bool, ADT) | missing `None` arm |
| T0012 | a struct does not satisfy an interface | missing method |
| T0013 | call of a value that is not a function | `let c = 5` … `c()` |
| T0014 | `?T` used where `T` is required | `opt + 1` |
| T0015 | unknown type in an annotation | `let s: Strng = ""` |
| T0016 | struct literal leaves out a field without default | `Point { x: 1 }` |
| T0017 | assignment to an immutable binding | `let x = 1` … `x = 2` |
| T0018 | name read before its definition runs | `say x` then `let x = 1` |
| T0019 | result of `push(xs, v)` discarded (edition 2027 only, always an error) | `push(xs, 3)` as a statement |
| T0020 | import not found or unreadable (an error from edition 2027) | `import { f } from "missing.fg"` |
| T0021 | an import renames a struct, `type`, variant or interface (an error from edition 2027) | `import { Point as P } from "geometry.fg"` |

Diagnostics derived from run-time rules use the engines' own rules, so they agree with what would happen: operator validity comes from the shared arithmetic rules, argument counts from the shared arity rule, and module members from the stdlib registry. Code passed to `assert_throws(fn() { ... })` is expected to fail and is not reported.

Name resolution follows the run-time scoping rules: blocks, function and lambda bodies, loop variables, `match` arms and `catch` clauses each introduce a scope; a `let` is visible after its statement; a function body may refer to names defined later in an enclosing scope (the body runs when called); `import "file"` brings in that file's top-level functions, variables, structs and variants. When an imported file cannot be found, unknown-name diagnostics are suppressed for the importing file.

## Runtime enforcement (`--strict`)

Static checking cannot see values typed `Any` — data from `json.parse`, HTTP responses, unannotated parameters. Under `--strict`, every function or lambda whose parameters are annotated checks its arguments on entry, and every function with a declared return type checks each value it returns:

```forge
fn double(n: Int) -> Int { return n * 2 }
let v = json.parse("\"seven\"")   // Any
double(v)
// error: type error: argument 'n' of 'double' must be Int, got String
```

The checks are performed at run time on both engines with identical messages. They are structural and deep for collections (an `[Int]` argument is checked element by element), check struct and ADT values by their type name, accept `Int` for `Float`, and skip `Any`, generic parameters and interfaces. An `Int` accepted for a declared `Float` becomes a Float (also inside `[Float]`, `?Float`, tuples, map values and results, and for annotated `let`s and struct fields declared `Float`, which are otherwise checked statically only): `fn half(x: Float) -> Float { return x / 2 }` returns `1.5` for `half(3)`. Apart from that conversion, a correct program behaves identically with and without `--strict`. Edition 2027 applies the same enforcement by default; edition 2026 without `--strict` runs no checks and converts nothing.
