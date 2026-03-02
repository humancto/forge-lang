# Dual Syntax Philosophy

## Principle

Every construct in Forge has two spellings: a **classic** form familiar to programmers coming from C, Rust, or JavaScript, and a **natural** form that reads closer to English. Both forms compile to the exact same AST and execute identically. There is no performance difference, no feature difference, and no hidden cost.

## Why Dual Syntax?

Programming languages force a false choice: be accessible or be powerful. Forge rejects this tradeoff.

- **Classic syntax** serves experienced developers who think in `fn`, `let`, and `else`. It is terse and precise.
- **Natural syntax** serves learners, scripters, and domain experts who prefer `define`, `set`, and `otherwise`. It reduces the cognitive barrier to writing code.

Neither form is "training wheels." Both are first-class citizens of the language.

## Rules

1. **Both forms are always available.** No mode switches, no compiler flags, no feature gates.
2. **They can be mixed freely.** You can use `let` on one line and `set` on the next. A file can use `fn` for one function and `define` for another.
3. **The AST is identical.** The parser recognizes both spellings and produces the same node. `let x = 5` and `set x to 5` produce an identical `LetDecl` AST node.
4. **Error messages normalize.** Runtime errors use the classic form regardless of which syntax the programmer used.
5. **Formatting preserves choice.** `forge fmt` does not convert between forms. Your stylistic choice is respected.

## Example: Mixed Syntax

```forge
// Classic variable, natural function
let name = "Alice"
define greet(person) {
    say "Hello, " + person
}

// Natural variable, classic function
set age to 30
fn is_adult(a) {
    return a >= 18
}

// Both work together seamlessly
if is_adult(age) {
    greet(name)
} otherwise {
    say "Too young"
}
```

## Design Guideline

When in doubt, use whichever form your team agrees on. For library code shared publicly, classic syntax is conventional. For scripts, tutorials, and learning materials, natural syntax often reads better.
