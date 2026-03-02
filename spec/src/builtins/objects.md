# Object Functions

Functions for working with Forge objects (key-value maps).

## has_key(object, key) -> bool

Returns `true` if `object` contains the specified key.

```forge
let user = { name: "Alice", age: 30 }
has_key(user, "name")    // true
has_key(user, "email")   // false
```

## get(object, key, default?) -> any

Retrieves a value from an object by key. Returns `default` (or `null`) if the key does not exist. Supports **dot-path notation** for nested access.

```forge
let config = {
    db: {
        host: "localhost",
        port: 5432
    }
}

get(config, "db.host")           // "localhost"
get(config, "db.port")           // 5432
get(config, "db.name", "mydb")   // "mydb" (default)
get(config, "missing")           // null
```

Also works with arrays by index:

```forge
let data = { items: [10, 20, 30] }
get(data, "items.1")  // 20
```

## pick(object, fields) -> object

Returns a new object containing only the specified fields.

```forge
let user = { name: "Alice", age: 30, email: "alice@example.com" }
pick(user, ["name", "email"])
// { name: "Alice", email: "alice@example.com" }
```

## omit(object, fields) -> object

Returns a new object with the specified fields removed.

```forge
let user = { name: "Alice", age: 30, password: "secret" }
omit(user, ["password"])
// { name: "Alice", age: 30 }
```

## merge(...objects) -> object

Merges multiple objects into one. Later objects override earlier keys.

```forge
let defaults = { color: "blue", size: "medium" }
let overrides = { size: "large", weight: 10 }
merge(defaults, overrides)
// { color: "blue", size: "large", weight: 10 }
```

## entries(object) -> array

Returns an array of `[key, value]` pairs.

```forge
let obj = { a: 1, b: 2 }
entries(obj)
// [["a", 1], ["b", 2]]
```

## from_entries(array) -> object

Converts an array of `[key, value]` pairs into an object. The inverse of `entries`.

```forge
from_entries([["name", "Alice"], ["age", 30]])
// { name: "Alice", age: 30 }
```

## diff(object_a, object_b) -> object

Returns an object describing the differences between two objects. Each key that differs contains an object with `a` and `b` values.

```forge
let old = { name: "Alice", age: 30, city: "NYC" }
let new_val = { name: "Alice", age: 31, email: "a@b.com" }
diff(old, new_val)
// {
//   age: { a: 30, b: 31 },
//   city: { a: "NYC", b: null },
//   email: { a: null, b: "a@b.com" }
// }
```
