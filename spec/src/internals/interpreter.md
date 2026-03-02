# Interpreter

The Forge interpreter is a **tree-walking interpreter** implemented in Rust. It traverses the AST directly, evaluating each node as it encounters it. This is the default and most complete execution engine.

## Architecture

```
Source (.fg) -> Lexer -> Parser -> AST -> Interpreter -> Result
                                            |
                                     Environment (scopes)
                                            |
                                     Runtime Bridge
                                  (axum, reqwest, tokio, rusqlite)
```

The interpreter lives in `src/interpreter/mod.rs` (~8,100 lines) and is the largest single file in the codebase.

## Key Components

### Environment

The interpreter maintains a stack of scopes. Each scope is an `IndexMap<String, Value>` that maps names to values. Variable resolution walks the scope stack from innermost to outermost.

- **Global scope**: Pre-populated with all 16 stdlib modules and all built-in functions.
- **Function scope**: Created on each function call, closed over by lambdas.
- **Block scope**: Created for `if`, `for`, `while`, and other block statements.

### Value Type

The `Value` enum represents all runtime values:

- `Int(i64)` -- 64-bit integer
- `Float(f64)` -- 64-bit float
- `Bool(bool)` -- boolean
- `String(String)` -- heap-allocated string
- `Array(Vec<Value>)` -- dynamic array
- `Object(IndexMap<String, Value>)` -- ordered key-value map
- `Null` -- null value
- `Function { params, body, closure }` -- named function with captured environment
- `Lambda { params, body, closure }` -- anonymous function
- `BuiltIn(String)` -- reference to a built-in function by name
- `ResultOk(Box<Value>)` / `ResultErr(Box<Value>)` -- Result type
- `Some(Box<Value>)` / `None` -- Option type
- `Channel(Arc<ChannelInner>)` -- concurrency channel
- `TaskHandle(Arc<TaskInner>)` -- async task handle

### Dispatch

Built-in function dispatch is a single large `match` statement in `call_builtin`. When a `BuiltIn("name")` value is called, the interpreter matches on the name string and executes the corresponding Rust code.

Stdlib module functions (e.g., `math.sqrt`) are dispatched through the module's `call` function. The interpreter detects dot-access on a module object and routes the call to the appropriate module.

## Features Unique to the Interpreter

The following features are **only** available in the interpreter tier:

- HTTP server (`@server`, `@get`, `@post`, `@delete`, `@ws`)
- Database access (`db.open`, `db.query`, `pg.connect`)
- AI integration (`ask`)
- Web scraping (`crawl`)
- File download (`download ... to`)
- Terminal UI widgets (`term.table`, `term.menu`, `term.confirm`)
- GenZ debug kit (`sus`, `bruh`, `bet`, `no_cap`, `ick`)
- Execution helpers (`cook`, `yolo`, `ghost`, `slay`)
- Concurrency (`channel`, `send`, `receive`, `spawn`)

## Performance Characteristics

The tree-walking approach means the interpreter re-traverses the AST on every loop iteration and function call. This makes it approximately 20x slower than Python for deep recursion benchmarks like `fib(35)`.

For most real-world scripts (file processing, HTTP handlers, database queries), interpreter overhead is negligible compared to I/O latency. The interpreter is the recommended tier for all general-purpose work.
