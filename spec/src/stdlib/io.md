# io

Input/output operations and command-line argument handling.

## Functions

### io.prompt(text?) -> string

Displays `text` and reads a line of input from stdin. Returns the input with trailing newline removed.

```forge
let name = io.prompt("What is your name? ")
say "Hello, " + name
```

### io.print(...args) -> null

Prints arguments to stdout without a trailing newline. Arguments are joined with spaces.

```forge
io.print("Loading")
io.print(".")
io.print(".")
io.print(".\n")
// Loading...
```

### io.args() -> array

Returns the program's command line: the script path as given to `forge run`, then every argument after it. Everything after the file goes to the program, flags included, so forge's own flags (`--interp`, `--allow-read`, ...) must come before the file and never appear here. A standalone binary built with `forge build` sees its own path and arguments. A sandboxed run (`forge mcp`'s `run_forge`, `Sandbox`) sees only what its host passes (`run_forge`'s `args`), never the host process's command line. In the REPL, under `forge -e` and in `forge test` it is empty.

```forge
// forge --interp run script.fg --verbose data.txt
let args = io.args()
say args                         // ["script.fg", "--verbose", "data.txt"]
say slice(args, 1, len(args))    // ["--verbose", "data.txt"]
```

### io.args_parse() -> object

Parses the program's arguments (after the script path, up to a `--`) into an object. Flags starting with `--` become keys. If a flag is followed by a non-flag value, that value is used; otherwise the flag is set to `true`.

```forge
// forge run script.fg --port 3000 --verbose
let opts = io.args_parse()
say opts["--port"]     // "3000"
say opts["--verbose"]  // true
```

### io.args_get(flag) -> string | bool | null

Returns the value of a specific command-line flag (a string). Returns `true` if the flag exists but has no value, or `null` if the flag is not present. Arguments after a `--` are not flags.

```forge
let port = io.args_get("--port")  // "3000" or null
let verbose = io.args_get("--verbose")  // true or null
```

### io.args_has(flag) -> bool

Returns `true` if the flag is present in the program's arguments (before any `--`).

```forge
if io.args_has("--debug") {
    log.debug("Debug mode enabled")
}
```
