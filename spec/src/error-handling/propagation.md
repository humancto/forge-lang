# Error Propagation

The `?` operator provides concise syntax for propagating errors up the call stack. When applied to a `Result` value, it unwraps `Ok` values and short-circuits on `Err` values by returning the error from the enclosing function.

## Syntax

```
TryExpr = Expression "?"
```

The `?` operator is a postfix unary operator applied to an expression that evaluates to a `Result`.

## Semantics

The `?` operator evaluates its operand and inspects the result:

- If the value is `Ok(v)`, the expression evaluates to `v` (the inner value is unwrapped).
- If the value is `Err(e)`, the enclosing function immediately returns `Err(e)`.
- If the value is neither `Ok` nor `Err`, a runtime error is raised: `` `?` expects Result value (Ok(...) or Err(...)) ``.

```forge
fn parse_number(s) {
    if s == "" {
        return Err("empty string")
    }
    return Ok(int(s))
}

fn double_parsed(s) {
    let n = parse_number(s)?    // unwraps Ok or returns Err
    return Ok(n * 2)
}

double_parsed("5")     // Ok(10)
double_parsed("")      // Err("empty string")
```

## Propagation Mechanism

When `?` encounters an `Err` value, it raises a `RuntimeError` with the `propagated` field set to the `Err` value. The runtime distinguishes propagated errors from ordinary runtime errors. When a propagated error reaches a function boundary, the function returns the propagated `Err` value rather than crashing the program.

The implementation:

1. `Expr::Try(expr)` evaluates `expr`.
2. If the result is `ResultOk(value)`, returns `value`.
3. If the result is `ResultErr(err)`, calls `RuntimeError::propagate(ResultErr(err))`, which creates a `RuntimeError` whose `propagated` field carries the original `Err` value.
4. The calling function's error handler detects the propagated value and converts it back into a return value.

## Chaining

The `?` operator can be chained across multiple function calls:

```forge
fn read_config() {
    let text = read_file("config.json")?
    let parsed = json.parse(text)?
    return Ok(parsed)
}
```

Each `?` either unwraps the `Ok` value for the next step or short-circuits the entire function with the first `Err` encountered.

## Requirements

The `?` operator requires that its operand evaluates to a `Result` value. Applying `?` to a non-Result value (such as an `Int`, `String`, or `null`) raises a runtime error:

```forge
let x = 42?    // runtime error: `?` expects Result value (Ok(...) or Err(...))
```

## Usage Patterns

### Propagate and Transform

```forge
fn load_user(id) {
    let data = fetch_user_data(id)?
    let user = parse_user(data)?
    return Ok(user)
}
```

### Propagate with Fallback

Combine `?` with `unwrap_or` for partial error handling:

```forge
fn get_config_or_default() {
    let config = read_config()?            // propagate file errors
    let timeout = unwrap_or(config.timeout, 30)  // fallback for missing field
    return Ok(timeout)
}
```

### Top-Level Handling

At the top level, propagated errors become runtime errors since there is no enclosing function to return from:

```forge
// If read_config returns Err, this crashes the program
let config = read_config()?
```

To handle errors at the top level, use `is_ok`/`is_err` checks, `unwrap_or`, or `try`/`catch` blocks instead of `?`.
