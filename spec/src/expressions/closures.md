# Closures and Lambdas

A **closure** (also called a **lambda**) is an anonymous function expression that captures variables from its enclosing scope. Closures are first-class values: they can be stored in variables, passed as arguments, and returned from functions.

## Syntax

```
fn(parameters) { body }
```

The keyword `fn` introduces a closure. Parameters are comma-separated identifiers enclosed in parentheses. The body is a block of statements.

```forge
let double = fn(x) { x * 2 }
say double(5)       // 10
```

## No-Parameter Closures

Closures with no parameters use empty parentheses:

```forge
let greet = fn() { "hello" }
say greet()         // "hello"
```

## Implicit Return

The last expression in a closure body is its return value. An explicit `return` statement is also permitted but rarely needed.

```forge
let add = fn(a, b) { a + b }       // implicit return
say add(3, 4)                       // 7

let abs = fn(x) {
    if x < 0 {
        return -x                   // explicit return
    }
    x
}
```

## Capture Semantics

Closures capture variables from their enclosing scope **by reference**. Changes to captured variables are visible inside the closure, and mutations inside the closure affect the outer scope.

```forge
let mut count = 0
let increment = fn() {
    count = count + 1
    count
}
say increment()     // 1
say increment()     // 2
say count           // 2
```

### Capture at Definition Time

The closure captures a reference to the variable's binding, not its current value. The variable is resolved at the time the closure is called, not when it is defined.

```forge
let mut x = 10
let get_x = fn() { x }
say get_x()         // 10

x = 20
say get_x()         // 20
```

## Higher-Order Functions

Closures enable higher-order programming patterns. A higher-order function either takes a function as an argument or returns one.

### Closures as Arguments

Many built-in functions accept closures:

```forge
let nums = [1, 2, 3, 4, 5]

let evens = filter(nums, fn(x) { x % 2 == 0 })
say evens       // [2, 4]

let doubled = map(nums, fn(x) { x * 2 })
say doubled     // [2, 4, 6, 8, 10]

let total = reduce(nums, 0, fn(acc, x) { acc + x })
say total       // 15
```

### Returning Closures

Functions can return closures, creating function factories:

```forge
fn make_adder(n) {
    fn(x) { x + n }
}

let add5 = make_adder(5)
say add5(10)    // 15
say add5(20)    // 25
```

## Closures in Method Syntax

Closures integrate naturally with method-style calls:

```forge
let names = ["Charlie", "Alice", "Bob"]
let sorted = names.sort(fn(a, b) { a < b })
say sorted      // ["Alice", "Bob", "Charlie"]
```

## Multi-Statement Bodies

Closure bodies can contain multiple statements. The last expression is the return value.

```forge
let process = fn(items) {
    let filtered = filter(items, fn(x) { x > 0 })
    let doubled = map(filtered, fn(x) { x * 2 })
    doubled
}
say process([-1, 2, -3, 4])    // [4, 8]
```

## Closures vs Named Functions

Closures and named functions (`fn name(...) { }` / `define name(...) { }`) differ in two ways:

1. **Naming**: Named functions are bound to a name in the current scope. Closures are anonymous and must be assigned to a variable explicitly.
2. **Hoisting**: Named functions are available throughout their defining scope. Closures are only available after the variable assignment that holds them.

Both named functions and closures capture their environment identically.

## Recursive Closures

A closure can call itself recursively if it is assigned to a variable that is in scope at the time of the call:

```forge
let factorial = fn(n) {
    if n <= 1 { 1 }
    else { n * factorial(n - 1) }
}
say factorial(5)    // 120
```

## Type Annotations

Closures do not currently support parameter or return type annotations. The types of parameters and the return value are inferred at runtime.
