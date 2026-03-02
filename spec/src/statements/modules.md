# Import and Export

The `import` statement loads definitions from external Forge source files or references built-in modules.

## Importing Files

To import all top-level definitions from another Forge source file:

```forge
import "utils.fg"
```

The import path is a string literal specifying the file to load. The interpreter resolves the path relative to the current working directory and checks the following locations in order:

1. `{path}` -- the exact path as given
2. `{path}.fg` -- with the `.fg` extension appended
3. `forge_modules/{path}/main.fg` -- in the `forge_modules` directory

If no file is found at any of these locations, a runtime error is produced.

## Import Semantics

When a file is imported, the following steps occur:

1. The source file is read and parsed.
2. A **new interpreter instance** is created.
3. The imported file is executed in the new interpreter.
4. Top-level definitions (`fn` and `let` bindings) are copied into the importing file's scope.

Importantly, each import creates a fresh interpreter. Side effects in the imported file (such as printing) occur during import. The imported file does not share mutable state with the importing file.

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

Only the listed names are copied into the current scope.

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

Forge does not detect or prevent circular imports. A circular import chain will cause infinite recursion and a stack overflow. It is the programmer's responsibility to avoid circular dependencies.

## No Namespacing

Imported definitions are placed directly into the importing file's scope. There is no namespace or module prefix for file imports. If two imported files define the same name, the later import overwrites the earlier one.

```forge
import "a.fg"       // defines fn helper()
import "b.fg"       // also defines fn helper() -- overwrites a.fg's version
```

## No Export Keyword

Forge does not have an explicit `export` keyword. All top-level `fn` and `let` bindings in a file are automatically available for import by other files.

## Re-Imports

Importing the same file multiple times re-executes it each time. There is no import caching or module singleton behavior for file imports.
