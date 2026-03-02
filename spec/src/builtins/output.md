# Output Functions

Functions for printing to stdout. All output functions accept any number of arguments, which are converted to strings and joined with spaces.

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
- All five functions write to stdout (not stderr). For stderr output, use the `log` module.
- Arguments of any type are auto-converted to their string representation.
