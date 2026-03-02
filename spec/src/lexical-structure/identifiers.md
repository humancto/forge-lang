# Identifiers

An identifier is a name that refers to a variable, function, type, field, or module. Identifiers are the primary mechanism for binding values to names in Forge programs.

## Syntax

> _Identifier_ → _IdentStart_ _IdentContinue_\*
>
> _IdentStart_ → `a`-`z` | `A`-`Z` | `_`
>
> _IdentContinue_ → _IdentStart_ | `0`-`9`

An identifier begins with an ASCII letter or underscore, followed by zero or more ASCII letters, digits, or underscores. Identifiers have no maximum length.

```forge
x
name
_private
camelCase
snake_case
PascalCase
item2
MAX_RETRIES
__internal
```

## Case Sensitivity

Identifiers are **case-sensitive**. The names `name`, `Name`, and `NAME` refer to three distinct bindings.

```forge
let name = "alice"
let Name = "Bob"
let NAME = "CHARLIE"
say name   // alice
say Name   // Bob
say NAME   // CHARLIE
```

## Reserved Words

If an identifier matches a keyword string (see [Keywords](./keywords.md)), it is lexed as that keyword token rather than as an `Ident` token. Keywords cannot be used as identifiers.

```forge
// Error: 'let' is a keyword, not a valid variable name
let let = 5  // parse error
```

## Naming Conventions

Forge does not enforce naming conventions, but the following are idiomatic:

| Element         | Convention    | Example              |
| --------------- | ------------- | -------------------- |
| Variables       | `snake_case`  | `user_name`          |
| Functions       | `snake_case`  | `get_user`           |
| Types (structs) | `PascalCase`  | `HttpRequest`        |
| Interfaces      | `PascalCase`  | `Describable`        |
| Constants       | `UPPER_SNAKE` | `MAX_RETRIES`        |
| Modules         | `snake_case`  | `math`, `fs`, `json` |

## The `it` Identifier

The identifier `it` has special meaning inside method blocks defined with `give` (or `impl`). When used as the first parameter of a method, `it` refers to the receiver instance — the object on which the method was called.

```forge
thing Person {
    name: String,
    age: Int
}

give Person {
    define greet(it) {
        return "Hi, I'm " + it.name
    }
}

set p to craft Person { name: "Alice", age: 30 }
say p.greet()  // Hi, I'm Alice
```

When `p.greet()` is called, the value of `p` is automatically bound to `it` inside the method body. The caller does not pass `it` explicitly.

If the first parameter of a method is _not_ named `it`, the method is treated as a **static method** — it is called on the type itself rather than on an instance:

```forge
give Person {
    define infant(name) {
        return craft Person { name: name, age: 0 }
    }
}

set baby to Person.infant("Bob")
```

Outside of method blocks, `it` has no special meaning and may be used as an ordinary identifier, though this is discouraged for clarity.

## Underscore

A lone underscore (`_`) is a valid identifier. By convention, it is used as a placeholder for values that are intentionally ignored:

```forge
match result {
    Ok(_) => say "success"
    Err(msg) => say "error: {msg}"
}
```

```forge
for _, value in enumerate(items) {
    say value
}
```

## Shadowing

A new `let` or `set` declaration may reuse an identifier that is already in scope. The new binding _shadows_ the previous one within the inner scope:

```forge
let x = 10
say x        // 10

if true {
    let x = 20
    say x    // 20 (shadows outer x)
}

say x        // 10 (outer x is unchanged)
```

Shadowing creates a new binding; it does not mutate the original variable.
