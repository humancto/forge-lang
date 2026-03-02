# Function Types

Functions are **first-class values** in Forge. They can be stored in variables, passed as arguments, returned from other functions, and placed in data structures.

## Named Functions

A named function is declared with `fn` (classic) or `define` (natural):

```forge
fn add(a, b) {
    return a + b
}

define multiply(a, b) {
    return a * b
}
```

Named functions are hoisted within their scope — they can be called before their textual definition in the source file.

## Anonymous Functions (Closures)

An anonymous function is created with the `fn` keyword in expression position:

```forge
let double = fn(x) { return x * 2 }
say double(21)  // 42
```

Anonymous functions are also called closures because they capture variables from their enclosing scope.

## Closure Semantics

Closures capture variables from the enclosing scope **by reference**. The closure retains access to the captured variables for its entire lifetime:

```forge
fn make_counter() {
    let mut count = 0
    return fn() {
        count = count + 1
        return count
    }
}

let counter = make_counter()
say counter()  // 1
say counter()  // 2
say counter()  // 3
```

Each call to `make_counter()` creates a new independent closure with its own `count` variable.

### Factory Pattern

Closures are commonly used to create specialized functions:

```forge
fn make_adder(n) {
    return fn(x) {
        return x + n
    }
}

let add5 = make_adder(5)
let add10 = make_adder(10)
say add5(3)   // 8
say add10(3)  // 13
```

## Functions as Arguments (Higher-Order Functions)

Functions can be passed as arguments to other functions:

```forge
fn apply(f, value) {
    return f(value)
}

fn square(x) { return x * x }
say apply(square, 7)  // 49
```

The built-in `map`, `filter`, `reduce`, and `sort` functions all accept function arguments:

```forge
let nums = [1, 2, 3, 4, 5]
let doubled = map(nums, fn(x) { return x * 2 })
say doubled  // [2, 4, 6, 8, 10]
```

## Functions in Data Structures

Functions can be stored in arrays and objects:

```forge
fn add(a, b) { return a + b }
fn sub(a, b) { return a - b }

let ops = [add, sub]
say ops[0](10, 3)  // 13
say ops[1](10, 3)  // 7
```

```forge
let handlers = {
    greet: fn(name) { return "Hello, {name}!" },
    farewell: fn(name) { return "Goodbye, {name}!" }
}
say handlers.greet("World")  // Hello, World!
```

## Type-Annotated Functions

Function parameters and return values may carry type annotations:

```forge
fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn format_price(amount: Float) -> String {
    return "${amount}"
}
```

Annotations are optional and serve as documentation. The optional type checker can use them to report errors before execution.

## Return Values

Functions return a value via the `return` statement. If no `return` is executed, the function returns `null`:

```forge
fn greet(name) {
    println("Hello, {name}!")
}

let result = greet("World")
say typeof(result)  // Null
```

A function may return early from any point:

```forge
fn classify(n) {
    if n < 0 { return "negative" }
    if n == 0 { return "zero" }
    return "positive"
}
```

## Async Functions

Async functions are declared with `async fn` (classic) or `forge` (natural):

```forge
// Classic
async fn fetch_data() {
    let resp = await http.get("https://api.example.com/data")
    return resp
}

// Natural
forge fetch_data() {
    let resp = hold http.get("https://api.example.com/data")
    return resp
}
```

Async functions return a future that must be awaited with `await` (classic) or `hold` (natural). See [Concurrency](../concurrency.md) for details.

## Function Equality

Functions are compared by **reference identity**, not by their code. Two function values are equal only if they refer to the same function object:

```forge
fn f() { return 1 }
let a = f
let b = f
say a == b  // true (same function object)

let c = fn() { return 1 }
say a == c  // false (different function objects)
```
