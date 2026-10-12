# Output Functions

Functions for printing to stdout (and, with `eprint`/`eprintln`, to stderr). All output functions accept any number of arguments, which are converted to strings and joined with spaces.

## print(...args) -> null

Prints arguments to stdout **without** a trailing newline.

```forge
print("loading")
print("...")
print("\n")
// loading...
```

## println(...args) -> null

Prints arguments to stdout **with** a trailing newline.

```forge
println("Hello, world!")
println("x =", 42)
// Hello, world!
// x = 42
```

## eprint(...args) -> null

Prints arguments to stderr **without** a trailing newline.

## eprintln(...args) -> null

Prints arguments to stderr **with** a trailing newline. Use it for warnings and errors of command-line programs, so stdout carries only the program's result.

```forge
eprintln("warning: skipped line", 3)
// (on stderr) warning: skipped line 3
```

## say(...args) -> null

Alias for `println`. Prints arguments followed by a newline. This is the idiomatic Forge output function.

```forge
say "Hello!"
say "The answer is", 42
```

## yell(...args) -> null

Prints arguments in UPPERCASE followed by a newline.

```forge
yell "fire detected"
// FIRE DETECTED
```

## whisper(...args) -> null

Prints arguments in lowercase followed by a newline.

```forge
whisper "QUIET PLEASE"
// quiet please
```

## Notes

- `print` and `println` are classic-style. `say`, `yell`, and `whisper` are Forge-style.
- `print`, `println`, `say`, `yell` and `whisper` write to stdout; `eprint` and `eprintln` write the same text to stderr. `log.*` also writes to stderr, as structured events with a timestamp and level.
- `eprint` and `eprintln` exist in every edition (edition 2027 removes `say`, `yell` and `whisper`).
- Arguments of any type are auto-converted to their string representation.
