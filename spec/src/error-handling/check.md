# Check

The `check` statement provides declarative validation with clear error messages. It evaluates a condition and raises a runtime error if the check fails.

## Syntax

```
CheckStmt     = "check" Expression CheckKind
CheckKind     = IsNotEmpty | Contains | Between | IsTrue
IsNotEmpty    = "is" "not" "empty"
Contains      = "contains" Expression
Between       = "is" "between" Expression "and" Expression
IsTrue        = (empty — implicit truth check)
```

## Check Kinds

Forge supports four validation kinds, each producing a specific boolean test:

### is not empty

Tests that a value is non-empty. The definition of "empty" depends on the type:

| Type        | Empty When   |
| ----------- | ------------ |
| `String`    | Length is 0  |
| `Array`     | Length is 0  |
| `Null`      | Always empty |
| Other types | Never empty  |

```forge
let name = "Alice"
check name is not empty    // passes

let empty = ""
check empty is not empty   // runtime error: check failed: "" did not pass validation
```

### contains

Tests that a string contains a substring:

```forge
let email = "user@example.com"
check email contains "@"    // passes

let bad = "not-an-email"
check bad contains "@"      // runtime error: check failed
```

The `contains` check currently operates on strings only. Both the value and the needle must be strings; other type combinations return `false`.

### is between ... and

Tests that a numeric value falls within an inclusive range:

```forge
let age = 25
check age is between 0 and 150    // passes

let temp = -10
check temp is between 0 and 100   // runtime error: check failed
```

Both `Int` and `Float` values are supported, but the value and both bounds must be the same type. Mixed-type comparisons (e.g., `Int` value with `Float` bounds) return `false`.

### Implicit Truth Check

When no check kind is specified, the value is tested for truthiness:

```forge
let valid = true
check valid    // passes

let invalid = false
check invalid  // runtime error: check failed
```

## Error Messages

When a check fails, the runtime raises a `RuntimeError` with the message:

```
check failed: <value> did not pass validation
```

Where `<value>` is the string representation of the tested value.

## Use Cases

### Input Validation

```forge
fn create_user(name, age, email) {
    check name is not empty
    check age is between 0 and 150
    check email contains "@"

    return { name: name, age: age, email: email }
}
```

### Preconditions

```forge
fn withdraw(account, amount) {
    check amount is between 1 and 10000
    check account.balance is between amount and 999999

    account.balance = account.balance - amount
}
```

### Configuration Validation

```forge
let config = load_config()
check config.host is not empty
check config.port is between 1 and 65535
```

## Comparison with Assert

| Feature       | `check`                                         | `assert`                       |
| ------------- | ----------------------------------------------- | ------------------------------ |
| Purpose       | Declarative validation                          | General assertion              |
| Syntax        | `check expr is not empty`                       | `assert(condition, "message")` |
| Error message | Auto-generated from value                       | User-provided                  |
| Kinds         | `is not empty`, `contains`, `is between`, truth | Boolean only                   |

`check` is designed for readable validation logic with automatic error descriptions. `assert` is a general-purpose assertion that requires the programmer to provide an error message.

## Nesting in Safe Blocks

Check failures are runtime errors and can be caught by `safe` blocks or `try`/`catch`:

```forge
safe {
    check "" is not empty    // fails, but error is suppressed
}
say "continues"              // prints "continues"
```

```forge
try {
    check input is not empty
} catch e {
    say "Validation error: " + e.message
}
```
