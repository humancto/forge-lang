# Statements

A **statement** is a syntactic construct that performs an action. Unlike [expressions](./expressions.md), statements do not produce values (with the exception of expression statements, where an expression is evaluated for its side effects and the result is discarded).

A Forge program is a sequence of statements executed top to bottom.

## Statement Categories

### Declarations

Declarations introduce new names into the current scope.

- **Variable declaration**: `let x = expr` / `set x to expr` -- binds a value to a name. See [Variable Declaration](./statements/variables.md).
- **Function declaration**: `fn name(params) { body }` / `define name(params) { body }` -- binds a function to a name. See [Function Declaration](./statements/functions.md).
- **Destructuring declaration**: `let {a, b} = obj` / `unpack {a, b} from obj` -- binds multiple names from a compound value. See [Variable Declaration](./statements/variables.md).

### Assignments

Assignments change the value of an existing binding.

- **Simple assignment**: `x = expr` / `change x to expr`
- **Compound assignment**: `x += expr`, `x -= expr`, `x *= expr`, `x /= expr`
- **Field assignment**: `obj.field = expr`
- **Index assignment**: `arr[i] = expr`

See [Assignment](./statements/assignment.md).

### Control Flow

Control flow statements direct the order of execution.

- **Conditional**: `if condition { body }` with optional `else`/`otherwise`/`nah` clauses. See [Control Flow](./statements/control-flow.md).
- **When guards**: `when expr { arms }` -- multi-way branch on a value. See [When Guards](./expressions/when-guards.md).
- **Match**: `match expr { arms }` -- structural pattern matching. See [Match Expressions](./expressions/match.md).

### Loops

Loop statements execute a body repeatedly.

- **For-in**: `for item in collection { body }` / `for each item in collection { body }`
- **While**: `while condition { body }`
- **Loop**: `loop { body }` -- infinite loop, exit with `break`
- **Repeat**: `repeat N times { body }` -- counted loop

See [Loops](./statements/loops.md).

### Jump Statements

Jump statements transfer control to a different point in the program.

- **return**: Exits the current function, optionally with a value.
- **break**: Exits the innermost loop.
- **continue**: Skips to the next iteration of the innermost loop.

See [Return, Break, Continue](./statements/jump.md).

### Module Statements

Module statements manage code organization across files.

- **import**: `import "file.fg"` -- executes another file and imports its definitions.

See [Import and Export](./statements/modules.md).

### Expression Statements

Any expression can appear as a statement. The expression is evaluated and its result is discarded. This is how function calls with side effects are written.

```forge
say "hello"             // function call as statement
push(items, 42)         // side-effecting call
```

## Statement Terminators

Forge does not require semicolons or other explicit statement terminators. Statements are separated by newlines. Multiple statements may appear on a single line if they are unambiguous to the parser.

```forge
let x = 10
let y = 20
say x + y
```

## Blocks

A **block** is a sequence of statements enclosed in `{` and `}`. Blocks appear as the body of functions, loops, conditionals, and other compound statements. Blocks create a new scope: variables declared inside a block are not visible outside it.

```forge
let x = "outer"
{
    let x = "inner"
    say x           // "inner"
}
say x               // "outer"
```
