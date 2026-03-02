# Function Declaration

Function declarations introduce named, callable units of code. Forge supports both classic and natural syntax forms.

## Basic Declaration

### Classic Syntax

```forge
fn greet(name) {
    say "Hello, {name}!"
}
```

### Natural Syntax

```forge
define greet(name) {
    say "Hello, {name}!"
}
```

Both forms are semantically identical. The function is bound to the given name in the current scope.

## Parameters

Parameters are comma-separated identifiers enclosed in parentheses.

```forge
fn add(a, b) {
    a + b
}
say add(3, 4)   // 7
```

### No Parameters

Functions with no parameters use empty parentheses:

```forge
fn hello() {
    say "hello"
}
hello()
```

### Default Parameters

Parameters can have default values. Default values are used when the caller does not provide an argument for that position.

```forge
fn greet(name, greeting = "Hello") {
    say "{greeting}, {name}!"
}
greet("Alice")              // "Hello, Alice!"
greet("Bob", "Hi")          // "Hi, Bob!"
```

Default parameters must appear after all required parameters.

### Variadic Parameters

Forge does not support variadic parameters (rest parameters). To accept a variable number of arguments, use an array parameter:

```forge
fn sum_all(numbers) {
    reduce(numbers, 0, fn(acc, n) { acc + n })
}
say sum_all([1, 2, 3, 4])  // 10
```

## Return Type Annotation

An optional return type annotation may follow the parameter list:

```forge
fn square(n: int): int {
    n * n
}
```

Type annotations are checked by the type checker when enabled. They do not affect runtime behavior in the interpreter.

## Return Values

### Implicit Return

The last expression in a function body is its return value. This is the idiomatic way to return values in Forge.

```forge
fn double(x) {
    x * 2
}
say double(5)   // 10
```

### Explicit Return

The `return` keyword exits the function immediately with a value:

```forge
fn abs(x) {
    if x < 0 {
        return -x
    }
    x
}
```

A bare `return` without a value returns `null`:

```forge
fn log_if_positive(x) {
    if x <= 0 {
        return
    }
    say "positive: {x}"
}
```

See [Return, Break, Continue](./jump.md) for details.

## Function Scope

Functions create a new scope. Variables declared inside a function are not accessible outside it. Functions can access variables from their enclosing scope (closure behavior).

```forge
let multiplier = 10

fn scale(x) {
    x * multiplier      // accesses 'multiplier' from outer scope
}
say scale(5)    // 50
```

## Recursion

Functions can call themselves recursively:

```forge
fn factorial(n) {
    if n <= 1 { return 1 }
    n * factorial(n - 1)
}
say factorial(5)    // 120
```

```forge
fn fib(n) {
    if n <= 1 { return n }
    fib(n - 1) + fib(n - 2)
}
say fib(10)     // 55
```

## Async Functions

Async functions are declared with `async fn` (classic) or `forge` (natural):

```forge
async fn fetch_data(url) {
    let resp = await http.get(url)
    resp.body
}

// Natural syntax
forge fetch_data(url) {
    let resp = hold http.get(url)
    resp.body
}
```

Async functions return a future that must be awaited with `await` / `hold`. See [Async Functions](../concurrency/async.md).

## Nested Functions

Functions can be declared inside other functions:

```forge
fn outer() {
    fn inner() {
        say "inside"
    }
    inner()
}
outer()     // "inside"
```

Inner functions have access to the outer function's scope.

## Functions Are Values

Function declarations create values that can be stored in variables, passed as arguments, and returned from other functions:

```forge
fn add(a, b) { a + b }
fn sub(a, b) { a - b }

let ops = [add, sub]
say ops[0](10, 3)   // 13
say ops[1](10, 3)   // 7
```

## Parameter Type Annotations

Parameters can include optional type annotations:

```forge
fn add(a: int, b: int): int {
    a + b
}
```

These annotations are informational for the type checker and do not enforce types at runtime in the interpreter.
