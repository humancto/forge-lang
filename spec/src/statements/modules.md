# Import and Export

The `import` statement loads definitions from external Forge source files or references built-in modules.

## Importing Files

To import all top-level definitions from another Forge source file:

```forge
import "utils.fg"
```

The import path is a string literal specifying the file to load. It is resolved in this order:

1. relative to the directory of the importing file;
2. relative to the current working directory;
3. as an installed package: `forge_modules/{path}/main.fg`, `forge_modules/{path}.fg`, `forge_modules/{path}/src/main.fg`.

The `.fg` extension is optional. If no file is found, the import fails with error `E0019`.

## Import Semantics

The first time a program imports a file:

1. The source file is read, parsed and (from edition 2027) type-checked.
2. Its top-level statements run once, in a context of their own.
3. The module is remembered for the rest of the program, by its canonical path.

Each `import` statement then binds names from the module: every top-level definition (`fn`, `let`, `struct`, `type` and its variants, `impl` methods) for `import "file.fg"`, or the listed names for a selective import. Importing a file that already ran — from the same file, from another module, or with `import "file.fg"` after `import { f } from "file.fg"` — binds names again but does not run the module again, so every importer shares the module's state:

```forge
// counter.fg
let mut count = 0
fn bump() {
    count = count + 1
    return count
}
```

```forge
// main.fg
import { bump } from "counter.fg"
import { report } from "stats.fg"   // stats.fg also imports counter.fg
say bump()                          // 1
say report()                        // sees the same counter
```

A `let` binding is copied when it is imported: the importer gets its value at the time of the import. Functions read the module's current state.

```forge
// utils.fg
fn double(x) { x * 2 }
fn triple(x) { x * 3 }
let PI = 3.14159
```

```forge
// main.fg
import "utils.fg"
say double(5)       // 10
say triple(5)       // 15
say PI              // 3.14159
```

## Selective Imports

To import specific names from a file, list them after the path:

```forge
import { double, PI } from "utils.fg"
say double(5)       // 10
say PI              // 3.14159
// triple is NOT imported
```

Only the listed names are bound in the current scope. A name the module does not define is error `E0019`.

### Renaming with `as`

`name as alias` binds an imported function or variable under another name, for example when two modules export the same name:

```forge
import { name as user_name } from "user.fg"
import { name as team_name } from "team.fg"
import { sqrt as root } from "math"
say user_name()
say root(16)        // 4.0
```

Only functions and variables can be renamed. Structs, `type` definitions, their variants and interfaces keep their name — their values, methods and `match` patterns refer to it — so `import { Point as P } from "geometry.fg"` is error `E0019`. `as` is not available in native imports (`import native "lib" as name` names the whole library instead).

## Namespace Imports

`import "path" as name` binds one name to the module's namespace instead of
bringing its definitions into scope:

```forge
// geo.fg
let TAU = 6.28
fn area(s) { ... }
struct Point { x: Int, y: Int = 0 }
impl Point { fn origin() { return Point { x: 0 } } }
type Shape = Circle(Float) | Square(Float) | Dot
```

```forge
// main.fg
import "geo.fg" as geo
println(geo.TAU)                         // 6.28
println(geo.area(geo.Circle(2.0)))       // functions and variant constructors
let p = geo.Point { x: 1 }               // a struct of the module
let o = geo.Point.origin()               // its static methods
let kind = match geo.Square(1.0) {
    geo.Circle(r) => "circle",           // variants in patterns
    geo.Square(w) => "square",
    geo.Dot => "dot",
}
```

The namespace is a read-only object. Its members are every name the
module's top level binds, in definition order, plus `__module__` (the path as
written): functions, `let` variables (destructured names too), `type` names
(the type's description, e.g. `geo.Shape.variants` is
`["Circle", "Square", "Dot"]`; not when a variant has the type's name),
variants, interfaces, structs, and the names the module's own imports bind
(re-exports). Methods and type tables come with the import as for every
other form. The module is the same single run as every other import of the file,
so `geo.f()` and an `f` imported with `import { f } from "geo.fg"` share the
module's state. A variable member is the variable's value when the import
ran, like `import { TAU }`; read state that changes through a function.

* `geo.nope` (or `geo.nope()`) is error `E0013`,
  `module 'geo.fg' has no member 'nope'`, with a suggestion when a member
  has a close name. The type checker reports it statically (`T0008`) and
  checks `geo.f(x)` against `f`'s signature, as for a direct import.
* `geo.x = 1` is error `E0006` (the namespace binding is immutable); a
  namespace has no builtin methods (`geo.len()` is `E0013` too).
* `geo.Name { ... }` requires `Name` to be a struct of the module
  (`'geo.Name' is not a struct`, `E0013`, otherwise).
* `geo.Name(...)` (or `geo.Name`) in a pattern matches only values built by
  the member `geo.Name`: that variant *of that type*. If another module's
  type also has a `Name` variant, its values do not match. An unknown
  member is `E0013` (`T0008` statically) when the arm is tried; a member
  that is not a variant is `'geo.Name' is not a variant` (`E0013`). A bare
  `Name(...)` pattern still matches any value whose variant is `Name`.
* The namespace is a value: it can be passed to functions and stored;
  `typeof(geo)` is `Object`.
* `import "math" as m` is error `E0019`: built-in modules keep their own
  name.

## Built-in Modules

Forge's standard library modules (`math`, `fs`, `io`, `crypto`, `db`, `pg`, `env`, `json`, `regex`, `log`, `term`, `http`, `csv`, `exec`, `time`) are automatically available without import. Attempting to `import` a built-in module name produces an error with guidance:

```forge
import "math"
// Error: 'math' is a built-in module — it's already available. Use it directly: math.function()
```

Built-in modules are accessed via dot notation:

```forge
say math.sqrt(16)       // 4.0
say math.pi             // 3.141592653589793
let data = fs.read("file.txt")
```

## Module-Level Execution

When a file is imported, all of its top-level statements execute. This includes not just declarations but also expression statements and side effects:

```forge
// setup.fg
say "setting up..."         // prints during import
let config = { debug: true }

fn get_config() { config }
```

```forge
// main.fg
import "setup.fg"           // prints "setting up..."
let c = get_config()
say c.debug                 // true
```

## Circular Imports

A module that imports itself, directly or through other modules, is error `E0019` with the chain:

```
circular import: a.fg -> b.fg -> a.fg
```

Move the shared code into a third module to break the cycle.

## Name Clashes

`import "file.fg"` and `import { ... } from "file.fg"` place definitions directly into the importing file's scope. If two imported files define the same name, the later import overwrites the earlier one; import one of them under another name with `as`, or import a module as a namespace, to keep both.

```forge
import "a.fg"                              // defines fn helper()
import { helper as b_helper } from "b.fg"  // b.fg's helper, renamed
import "c.fg" as c                         // c.helper()
```

## No Export Keyword

Forge does not have an explicit `export` keyword. All top-level `fn` and `let` bindings in a file are automatically available for import by other files.

## Re-Imports

A file runs once per program. Importing it again binds its names again (for `let` bindings, their current values) without re-running its top-level code. The REPL keeps imported modules for the whole session; restart it to load a changed file. An HTTP request handler that imports a file runs it in that request's own isolated copy of the program.
