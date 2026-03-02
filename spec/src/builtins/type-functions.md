# Type Functions

Functions for type conversion and inspection.

## str(value) -> string

Converts any value to its string representation.

```forge
str(42)        // "42"
str(3.14)      // "3.14"
str(true)      // "true"
str(null)      // "null"
str([1, 2])    // "[1, 2]"
```

## int(value) -> int

Converts a value to an integer. Accepts integers, floats (truncated), and numeric strings.

```forge
int(3.14)     // 3
int("42")     // 42
int(100)      // 100
```

Errors if the string is not a valid integer.

## float(value) -> float

Converts a value to a float. Accepts integers, floats, and numeric strings.

```forge
float(42)       // 42.0
float("3.14")   // 3.14
float(1)        // 1.0
```

Errors if the string is not a valid number.

## type(value) -> string

Returns the type name of `value` as a string.

```forge
type(42)          // "Int"
type(3.14)        // "Float"
type("hello")     // "String"
type(true)        // "Bool"
type(null)        // "Null"
type([1, 2])      // "Array"
type({a: 1})      // "Object"
type(fn(x) { x }) // "Function"
```

## typeof(value) -> string

Alias for `type`. Returns the type name of `value`.

```forge
typeof("hello")  // "String"
typeof(42)       // "Int"
```
