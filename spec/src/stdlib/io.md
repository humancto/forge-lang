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

Returns all command-line arguments as an array of strings, including the program name.

```forge
let args = io.args()
say args  // ["forge", "run", "script.fg", "--verbose"]
```

### io.args_parse() -> object

Parses command-line arguments into an object. Flags starting with `--` become keys. If a flag is followed by a non-flag value, that value is used; otherwise the flag is set to `true`.

```forge
// forge run script.fg --port 3000 --verbose
let opts = io.args_parse()
say opts["--port"]     // "3000"
say opts["--verbose"]  // true
```

### io.args_get(flag) -> string | bool | null

Returns the value of a specific command-line flag. Returns `true` if the flag exists but has no value, or `null` if the flag is not present.

```forge
let port = io.args_get("--port")  // "3000" or null
let verbose = io.args_get("--verbose")  // true or null
```

### io.args_has(flag) -> bool

Returns `true` if the flag is present in the command-line arguments.

```forge
if io.args_has("--debug") {
    log.debug("Debug mode enabled")
}
```
