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

## No Namespacing

Imported definitions are placed directly into the importing file's scope. There is no namespace or module prefix for file imports. If two imported files define the same name, the later import overwrites the earlier one; import one of them under another name with `as` to keep both.

```forge
import "a.fg"                              // defines fn helper()
import { helper as b_helper } from "b.fg"  // b.fg's helper, renamed
```

## No Export Keyword

Forge does not have an explicit `export` keyword. All top-level `fn` and `let` bindings in a file are automatically available for import by other files.

## Re-Imports

A file runs once per program. Importing it again binds its names again (for `let` bindings, their current values) without re-running its top-level code. The REPL keeps imported modules for the whole session; restart it to load a changed file. An HTTP request handler that imports a file runs it in that request's own isolated copy of the program.
