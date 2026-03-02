# term

Terminal formatting, colors, and UI widgets. Color functions return styled strings; display functions print to stderr and return `null`.

## Color Functions

Each color function wraps text in ANSI escape codes and returns the styled string.

### term.red(text) -> string

### term.green(text) -> string

### term.blue(text) -> string

### term.yellow(text) -> string

### term.cyan(text) -> string

### term.magenta(text) -> string

```forge
say term.red("Error!")
say term.green("Success!")
say term.blue("Info")
```

### term.bold(text) -> string

### term.dim(text) -> string

```forge
say term.bold("Important")
say term.dim("subtle note")
```

## Display Functions

### term.table(rows) -> null

Prints a formatted table from an array of objects. Column widths are auto-calculated. Headers come from the keys of the first object.

```forge
let data = [
    { name: "Alice", role: "Admin", active: true },
    { name: "Bob", role: "User", active: false }
]
term.table(data)
// name  | role  | active
// ------+-------+-------
// Alice | Admin | true
// Bob   | User  | false
```

### term.hr(width?, char?) -> null

Prints a horizontal rule. Default width is 40, default character is `"─"`.

```forge
term.hr()         // ────────────────────────────────────────
term.hr(20)       // ────────────────────
term.hr(20, "=")  // ====================
```

### term.banner(text) -> null

Prints text in a double-line box.

```forge
term.banner("Forge v0.3.3")
// ╔════════════════╗
// ║  Forge v0.3.3  ║
// ╚════════════════╝
```

### term.box(text) -> null

Prints text in a single-line box. Supports multi-line text.

```forge
term.box("Hello\nWorld")
// ┌───────┐
// │ Hello │
// │ World │
// └───────┘
```

### term.bar(label, value, max?) -> null

Prints a progress bar. Default `max` is 100.

```forge
term.bar("CPU", 73, 100)
//   CPU [██████████████████████░░░░░░░░] 73%
```

### term.sparkline(numbers) -> string

Returns a sparkline string from an array of numbers using Unicode block characters.

```forge
let spark = term.sparkline([1, 5, 3, 8, 2, 7, 4, 6])
say spark  // ▁▅▃█▂▇▃▆
```

### term.gradient(text) -> string

Returns text with a rainbow gradient using 256-color ANSI codes.

```forge
say term.gradient("Hello, Forge!")
```

### term.success(message) -> null

Prints a green success message with a checkmark.

```forge
term.success("Build complete")
//   ✅ Build complete
```

### term.error(message) -> null

Prints a red error message with an X mark.

```forge
term.error("Compilation failed")
//   ❌ Compilation failed
```

### term.warning(message) -> null

Prints a yellow warning message.

```forge
term.warning("Deprecated API usage")
```

### term.info(message) -> null

Prints a cyan info message.

```forge
term.info("3 files processed")
```

### term.clear() -> null

Clears the terminal screen.

### term.confirm(prompt?) -> bool

Prints a yes/no prompt and returns `true` if the user enters "y" or "yes".

```forge
if term.confirm("Delete all files?") {
    fs.remove("output/")
}
```

### term.menu(options, prompt?) -> any

Displays a numbered menu and returns the selected item.

```forge
let choice = term.menu(["New Project", "Open Project", "Quit"])
say choice
```

### term.countdown(seconds?) -> null

Displays an animated countdown. Default is 3 seconds.

```forge
term.countdown(5)
```

### term.typewriter(text, delay?) -> null

Prints text one character at a time. Default delay is 30ms per character.

```forge
term.typewriter("Loading system...", 50)
```

### term.emoji(name) -> string

Returns an emoji by name. Use `term.emojis()` to list all available names.

```forge
say term.emoji("rocket")  // 🚀
say term.emoji("check")   // ✅
say term.emoji("fire")    // 🔥
```

### term.beep() -> null

Plays the terminal bell sound.
