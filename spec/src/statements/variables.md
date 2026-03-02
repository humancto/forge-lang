# Variable Declaration

Variable declarations introduce new bindings in the current scope. Forge supports both classic and natural syntax forms, with optional mutability and destructuring.

## Immutable Variables

By default, variables are immutable. An immutable binding cannot be reassigned after initialization.

### Classic Syntax

```forge
let name = "Alice"
let age = 30
let items = [1, 2, 3]
```

### Natural Syntax

```forge
set name to "Alice"
set age to 30
set items to [1, 2, 3]
```

Both forms are semantically identical. The variable is bound to the result of evaluating the right-hand expression.

## Mutable Variables

To allow a variable to be reassigned after its initial declaration, use the `mut` keyword.

### Classic Syntax

```forge
let mut count = 0
count = count + 1       // allowed
```

### Natural Syntax

```forge
set mut count to 0
change count to count + 1   // allowed
```

Attempting to reassign an immutable variable produces a runtime error:

```forge
let x = 10
x = 20          // runtime error: cannot reassign immutable variable 'x'
```

## Type Annotations

Variable declarations may include an optional type annotation after the variable name:

```forge
let name: string = "Alice"
let age: int = 30
let ratio: float = 0.5
```

Type annotations are checked by the type checker when enabled. They do not affect runtime behavior in the interpreter.

## Initializer Expressions

The right-hand side of a variable declaration is any valid expression:

```forge
let sum = 1 + 2 + 3
let greeting = "Hello, {name}!"
let data = fs.read("config.json")
let result = compute(x, y)
```

Every variable declaration requires an initializer. There is no uninitialized variable syntax.

## Destructuring

Forge supports destructuring assignment for objects and arrays.

### Object Destructuring

#### Classic Syntax

```forge
let person = { name: "Alice", age: 30, city: "NYC" }
let { name, age } = person
say name    // "Alice"
say age     // 30
```

#### Natural Syntax

```forge
let person = { name: "Alice", age: 30, city: "NYC" }
unpack { name, age } from person
say name    // "Alice"
say age     // 30
```

Object destructuring extracts the named fields from an object and binds them to variables with the same names.

### Array Destructuring

```forge
let coords = [10, 20, 30]
let [x, y, z] = coords
say x   // 10
say y   // 20
say z   // 30
```

#### Rest Pattern

Array destructuring supports a rest pattern to capture remaining elements:

```forge
let items = [1, 2, 3, 4, 5]
let [first, ...rest] = items
say first   // 1
say rest    // [2, 3, 4, 5]
```

## Scope

Variables are scoped to the block in which they are declared. A variable declared inside an `if` body, loop body, or function body is not accessible outside that block.

```forge
if true {
    let x = 42
    say x       // 42
}
// x is not accessible here
```

Inner scopes can shadow variables from outer scopes:

```forge
let x = "outer"
{
    let x = "inner"
    say x           // "inner"
}
say x               // "outer"
```

## Multiple Declarations

Each `let`/`set` statement declares a single binding (or a destructuring pattern). To declare multiple variables, use separate statements:

```forge
let x = 1
let y = 2
let z = 3
```
