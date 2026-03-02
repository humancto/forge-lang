# Collection Types

Forge has two built-in collection types: `Array` and `Object`. Both are mutable, heap-allocated, and compared by reference for identity but by value for equality.

## Array

An array is an **ordered, heterogeneous, 0-indexed** sequence of values. Elements may be of any type, including other arrays and objects.

### Creation

Arrays are created with square bracket literals:

```forge
let empty = []
let nums = [1, 2, 3]
let mixed = [1, "two", true, null]
let nested = [[1, 2], [3, 4]]
```

### Indexing

Elements are accessed by zero-based integer index using bracket notation:

```forge
let fruits = ["apple", "banana", "cherry"]
say fruits[0]   // apple
say fruits[1]   // banana
say fruits[2]   // cherry
```

Negative indices count from the end of the array:

```forge
say fruits[-1]  // cherry
```

Accessing an out-of-bounds index produces a runtime error.

### Mutation

Arrays are mutable. Elements can be replaced by index assignment:

```forge
let mut items = [1, 2, 3]
items[0] = 10
say items  // [10, 2, 3]
```

The `push()` built-in appends an element:

```forge
let mut items = [1, 2]
push(items, 3)
say items  // [1, 2, 3]
```

The `pop()` built-in removes and returns the last element:

```forge
let mut items = [1, 2, 3]
let last = pop(items)
say last   // 3
say items  // [1, 2]
```

### Length

The `len()` built-in returns the number of elements:

```forge
say len([1, 2, 3])  // 3
say len([])          // 0
```

### Spread

The spread operator `...` expands an array within another array literal:

```forge
let a = [1, 2, 3]
let b = [...a, 4, 5]
say b  // [1, 2, 3, 4, 5]
```

### Iteration

Arrays are iterable with `for`/`in`:

```forge
for item in [10, 20, 30] {
    say item
}
```

With `enumerate()` for index-value pairs:

```forge
for i, item in enumerate(["a", "b", "c"]) {
    say "{i}: {item}"
}
```

### Functional Operations

Arrays support `map`, `filter`, `reduce`, `sort`, `reverse`, `find`, `flat_map`, `any`, `all`, and other functional built-ins:

```forge
let nums = [1, 2, 3, 4, 5]
let doubled = map(nums, fn(x) { return x * 2 })
let evens = filter(nums, fn(x) { return x % 2 == 0 })
let sum = reduce(nums, 0, fn(acc, x) { return acc + x })
```

### Truthiness

An empty array `[]` is falsy. All non-empty arrays are truthy.

## Object

An object is an **insertion-ordered map** from string keys to arbitrary values. Objects are Forge's general-purpose key-value data structure.

### Creation

Objects are created with curly brace literals:

```forge
let empty = {}
let user = { name: "Alice", age: 30 }
let config = {
    host: "localhost",
    port: 8080,
    debug: false,
}
```

Keys are written as bare identifiers in the literal syntax. At runtime, they are strings.

### Field Access

Fields are accessed with dot notation or bracket notation:

```forge
let user = { name: "Alice", age: 30 }
say user.name       // Alice
say user["age"]     // 30
```

Dot notation requires the field name to be a valid identifier. Bracket notation accepts any string expression, making it suitable for dynamic keys:

```forge
let key = "name"
say user[key]  // Alice
```

### Mutation

Objects are mutable. Fields can be added, updated, or accessed dynamically:

```forge
let mut obj = { x: 1 }
obj.y = 2
obj.x = 10
say obj  // { x: 10, y: 2 }
```

### Key Operations

| Function          | Description                             |
| ----------------- | --------------------------------------- |
| `keys(obj)`       | Returns array of keys                   |
| `values(obj)`     | Returns array of values                 |
| `entries(obj)`    | Returns array of `[key, value]` pairs   |
| `has_key(obj, k)` | Returns true if key exists              |
| `len(obj)`        | Returns number of key-value pairs       |
| `merge(a, b)`     | Returns new object merging `b` into `a` |
| `pick(obj, ks)`   | Returns object with only specified keys |
| `omit(obj, ks)`   | Returns object without specified keys   |

### Spread

The spread operator expands an object within another object literal:

```forge
let base = { x: 1, y: 2 }
let ext = { ...base, z: 3 }
say ext  // { x: 1, y: 2, z: 3 }
```

When keys conflict, later values overwrite earlier ones:

```forge
let a = { x: 1, y: 2 }
let b = { ...a, x: 10 }
say b  // { x: 10, y: 2 }
```

### Iteration

Objects are iterable. A `for`/`in` loop over an object yields `[key, value]` pairs in insertion order:

```forge
let user = { name: "Alice", age: 30 }
for key, value in user {
    say "{key}: {value}"
}
```

### Truthiness

An empty object `{}` is truthy (unlike empty arrays, which are falsy). All objects, including empty ones, are truthy.

### Objects vs. Structs

Plain objects are untyped: any object can have any set of keys. Structs (defined with `struct` or `thing`) provide named types with declared fields, type annotations, default values, and methods. Under the hood, struct instances are objects with a `__type__` field. See [Struct Types](./structs.md) for details.
