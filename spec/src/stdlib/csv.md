# csv

CSV parsing and serialization. Uses comma-separated values with automatic type inference for fields.

## Functions

### csv.parse(string) -> array

Parses a CSV string into an array of objects. The first line is treated as headers. Values are automatically converted to `int`, `float`, or `bool` where possible; otherwise they remain strings.

```forge
let data = csv.parse("name,age,active\nAlice,30,true\nBob,25,false")
say data[0].name    // "Alice"
say data[0].age     // 30
say data[1].active  // false
```

### csv.stringify(rows) -> string

Converts an array of objects into a CSV string. Headers are derived from the keys of the first object.

```forge
let rows = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 25 }
]
let output = csv.stringify(rows)
say output
// name,age
// Alice,30
// Bob,25
```

Values containing commas or quotes are automatically quoted.

### csv.read(path) -> array

Reads a CSV file from disk and parses it into an array of objects. Equivalent to `csv.parse(fs.read(path))`.

```forge
let users = csv.read("users.csv")
for user in users {
    say user.name + ": " + str(user.email)
}
```

### csv.write(path, rows) -> null

Serializes an array of objects as CSV and writes it to the file at `path`.

```forge
let data = [
    { product: "Widget", price: 9.99, qty: 100 },
    { product: "Gadget", price: 24.99, qty: 50 }
]
csv.write("inventory.csv", data)
```
