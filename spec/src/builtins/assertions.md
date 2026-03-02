# Assertion Functions

Functions for testing and validation. Assertion failures produce runtime errors with descriptive messages.

## assert(condition, message?) -> null

Asserts that `condition` is truthy. Throws a runtime error if false.

```forge
assert(1 + 1 == 2)
assert(len("hello") > 0, "string should not be empty")
```

## assert_eq(actual, expected, message?) -> null

Asserts that `actual` equals `expected`. Shows both values on failure.

```forge
assert_eq(1 + 1, 2)
assert_eq(sort([3, 1, 2]), [1, 2, 3])
assert_eq(user.name, "Alice", "user name mismatch")
```

## assert_ne(actual, expected, message?) -> null

Asserts that `actual` does not equal `expected`.

```forge
assert_ne(1, 2)
assert_ne(user.role, "admin", "user should not be admin")
```

## assert_throws(fn, message?) -> null

Asserts that calling `fn` produces a runtime error. Useful for testing error handling.

```forge
assert_throws(fn() { int("not a number") })
assert_throws(fn() { bruh "expected crash" })
```

## satisfies(value, interface) -> bool

Checks whether `value` structurally satisfies an `interface` (Go-style structural typing). Returns `true` if the value has all methods specified by the interface, either through the environment or through `give`/`impl` blocks.

```forge
power Printable {
    fn to_string() -> string
}

thing User {
    name: string
}

give User {
    fn to_string() {
        return self.name
    }
}

let u = User { name: "Alice" }
assert(satisfies(u, Printable))
```

See the [Type System](../types/interfaces.md) chapter for more details on interface satisfaction.
