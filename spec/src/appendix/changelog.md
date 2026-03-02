# Changelog

## v0.3.3

**Type system -- natural syntax**

- Added `thing` keyword as alias for `struct`
- Added `power` keyword as alias for `interface`
- Added `give` keyword as alias for `impl`
- Added `give X the power Y` syntax for interface implementation
- Added `craft` keyword for struct construction
- Added `has` contextual parsing inside struct/thing bodies (not a reserved keyword)
- Interface satisfaction checking (Go-style structural typing)

## v0.3.2

**Landing page and book**

- Added mdBook-based language specification
- Created project landing page
- Documentation improvements across all modules

## v0.3.1

**Bug fixes and stabilization**

- Fixed production gaps identified by architecture audit
- Comprehensive integration test suite (179 tests)
- Stability improvements for interpreter, VM, and JIT tiers

## v0.3.0

**Standard library expansion**

- Expanded stdlib to 16 modules with 230+ functions
- Added `time` module (25 functions: now, unix, parse, format, diff, add, sub, zone, etc.)
- Added `npc` module (16 fake data generators)
- Added `csv` module (parse, stringify, read, write with auto type inference)
- Added `term` module (25+ functions: colors, table, sparkline, bar, banner, box, gradient, menu, etc.)
- Added `exec` module (run_command with stdout/stderr/status)
- Expanded `http` module with download and crawl
- Expanded `fs` module to 20 functions
- Native `Option<T>` values (`Some`, `None`, `is_some`, `is_none`)
- Tokio `spawn` with language-level task handles
- Language-level channels (`channel`, `send`, `receive`, `try_send`, `try_receive`)
- GenZ debug kit (`sus`, `bruh`, `bet`, `no_cap`, `ick`)
- Execution helpers (`cook`, `yolo`, `ghost`, `slay`)
- 30 interactive tutorials via `forge learn`
- Shell integration (`sh`, `shell`, `sh_lines`, `sh_json`, `sh_ok`, `pipe_to`)
- Dual syntax (natural language keywords alongside classic syntax)
- Innovation keywords: `when`, `must`, `safe`, `check`, `retry`, `timeout`, `schedule`, `watch`, `ask`, `download`, `crawl`, `repeat`, `wait`, `grab`, `emit`, `hold`
- Pipe operator (`|>`) and query-style pipe chains
- `where` filter syntax
- `freeze` for immutable values
- Decorator system (`@server`, `@get`, `@post`, `@put`, `@delete`, `@ws`)
- HTTP server powered by axum + tokio
- WebSocket support
- PostgreSQL support via `pg` module
- Compound assignment operators (`+=`, `-=`, `*=`, `/=`)
- Spread operator (`...`) in arrays and objects
- String interpolation (`"hello, {name}"`)
- Raw strings (`"""no interpolation"""`)
- Destructuring (`let {a, b} = obj` / `unpack {a, b} from obj`)
- Default parameter values
- 12 example programs
- 13 CLI commands (run, repl, version, fmt, test, new, build, install, lsp, learn, chat, help, -e)

## v0.2.0

**VM and JIT tiers**

- Register-based bytecode VM (`--vm` flag)
- 32-bit instruction encoding (ABC, ABx, AsBx formats)
- Mark-sweep garbage collector for VM heap
- Green thread scheduler in VM
- Cranelift JIT compiler (`--jit` flag)
- JIT achieves ~11x faster than Python on `fib(30)`
- Bytecode compiler from AST to VM instructions
- VM supports core language features (variables, functions, closures, control flow, arrays, objects)
- JIT supports integer/float arithmetic, function calls, recursion, basic control flow

## v0.1.0

**Initial release**

- Tree-walking interpreter in Rust
- Core language: variables, functions, closures, if/else, for, while, match
- Basic type system: Int, Float, Bool, String, Array, Object, Null
- Result type (`Ok`, `Err`) with `?` operator
- `math`, `fs`, `io`, `crypto`, `db`, `env`, `json`, `regex`, `log` modules
- Built-in HTTP client via `reqwest`
- SQLite database access via `rusqlite`
- REPL mode
- Formatter (`forge fmt`)
- Test runner (`forge test`)
- Project scaffolding (`forge new`)
