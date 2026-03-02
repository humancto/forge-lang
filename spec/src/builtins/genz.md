# GenZ Debug Kit

A set of debugging and assertion functions with personality. These are fully functional tools with expressive error messages -- not just jokes.

## sus(value) -> value

Inspects a value and prints its type and content to stderr, then returns the value unchanged. Works like Rust's `dbg!` macro -- you can wrap any expression without changing program behavior.

```forge
let x = sus(42)
// stderr: 🔍 SUS CHECK: 42 (Int)
// x is still 42

let result = sus(http.get("https://api.example.com"))
// Prints the response object, then returns it
```

## bruh(message?) -> never

Panics with a runtime error. Equivalent to `panic!` in Rust. Default message: "something ain't right".

```forge
bruh "database connection lost"
// Error: BRUH: database connection lost

bruh
// Error: BRUH: something ain't right
```

## bet(condition, message?) -> bool

Asserts that `condition` is truthy. Returns `true` on success, errors on failure. Equivalent to `assert`.

```forge
bet(user.age >= 18, "user must be an adult")
// On failure: Error: LOST THE BET: user must be an adult

bet(1 + 1 == 2)  // passes, returns true
```

## no_cap(a, b) -> bool

Asserts that `a` equals `b`. Returns `true` on success, errors on failure. Equivalent to `assert_eq`.

```forge
no_cap(1 + 1, 2)  // passes
no_cap("hello", "world")
// Error: CAP DETECTED: hello ≠ world
```

## ick(condition, message?) -> bool

Asserts that `condition` is **false**. Returns `true` when the condition is false, errors when true. The inverse of `bet`.

```forge
ick(user.banned, "user should not be banned")
// On failure: Error: ICK: user should not be banned

ick(false)  // passes, returns true
```
