# Result Type

The `Result` type represents the outcome of an operation that may succeed or fail. A `Result` is either `Ok(value)` for success or `Err(message)` for failure. Results are first-class values that can be stored in variables, passed to functions, and returned from functions.

## Syntax

```
ResultOk    = ("Ok" | "ok") "(" Expression ")"
ResultErr   = ("Err" | "err") "(" Expression ")"
```

## Constructors

Forge provides two constructors for creating Result values. Both accept case-insensitive names:

```forge
let success = Ok(42)
let failure = Err("something went wrong")

// Lowercase variants are equivalent
let success2 = ok(42)
let failure2 = err("not found")
```

`Ok` wraps any value:

```forge
Ok(42)            // Result containing Int
Ok("hello")       // Result containing String
Ok([1, 2, 3])     // Result containing Array
Ok(null)          // Result containing null (Ok with no meaningful value)
```

`Err` wraps an error value (typically a string message):

```forge
Err("file not found")
Err("invalid input: expected number")
```

If `Ok` is called with no arguments, it wraps `null`. If `Err` is called with no arguments, it wraps the string `"error"`.

## Runtime Representation

Results are distinct variants in the `Value` enum:

- `Value::ResultOk(Box<Value>)` — wraps the success value.
- `Value::ResultErr(Box<Value>)` — wraps the error value.

The `typeof` builtin returns `"Result"` for both variants:

```forge
typeof(Ok(1))     // "Result"
typeof(Err("x"))  // "Result"
```

## Display Format

Results are displayed as `Ok(value)` or `Err(value)`:

```forge
say Ok(42)              // Ok(42)
say Err("not found")    // Err(not found)
```

In JSON serialization, Results produce `{ "Ok": value }` or `{ "Err": value }`.

## Inspection Functions

Four builtin functions inspect and extract Result values:

### is_ok(result)

Returns `true` if the value is `Ok`, `false` if `Err`. Raises a runtime error if the argument is not a Result.

```forge
is_ok(Ok(42))           // true
is_ok(Err("oops"))      // false
```

### is_err(result)

Returns `true` if the value is `Err`, `false` if `Ok`. Raises a runtime error if the argument is not a Result.

```forge
is_err(Ok(42))          // false
is_err(Err("oops"))     // true
```

### unwrap(result)

Extracts the inner value from `Ok`. If the value is `Err`, raises a runtime error with the message `"unwrap() on Err: <error_value>"`.

```forge
unwrap(Ok(42))          // 42
unwrap(Err("oops"))     // runtime error: unwrap() on Err: oops
```

`unwrap` also works with Option values (`Some`/`None`):

```forge
unwrap(Some(42))        // 42
unwrap(None)            // runtime error: unwrap() called on None
```

### unwrap_or(result, default)

Extracts the inner value from `Ok`. If the value is `Err`, returns the `default` value instead. Never raises an error for valid Result inputs.

```forge
unwrap_or(Ok(42), 0)       // 42
unwrap_or(Err("oops"), 0)  // 0
```

`unwrap_or` also works with Option values:

```forge
unwrap_or(Some(42), 0)     // 42
unwrap_or(None, 0)         // 0
```

Raises a runtime error if called with the wrong number of arguments or if the first argument is not a Result or Option.

## Pattern Matching on Results

Results can be matched in `match` expressions using `Ok` and `Err` patterns:

```forge
let result = Ok(42)

match result {
    Ok(value) -> say "Got: " + str(value),
    Err(msg) -> say "Error: " + msg
}
```

## Results in Functions

Functions commonly return Results to signal success or failure:

```forge
fn divide(a, b) {
    if b == 0 {
        return Err("division by zero")
    }
    return Ok(a / b)
}

let result = divide(10, 0)
if is_err(result) {
    say "Cannot divide: " + unwrap(Err("division by zero"))
}
```

## Equality

Two `Ok` values are equal if their inner values are equal. Two `Err` values are equal if their inner values are equal. `Ok` and `Err` are never equal to each other:

```forge
Ok(42) == Ok(42)          // true
Err("x") == Err("x")     // true
Ok(42) == Err(42)         // false
Ok(1) == Ok(2)            // false
```
