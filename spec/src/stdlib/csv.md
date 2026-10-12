# csv

CSV parsing and serialization. Uses comma-separated values with automatic type inference for fields.

## Functions

### csv.parse(string, options?) -> array

Parses a CSV string (RFC 4180) into an array of objects. The first record is treated as headers. Fields are separated by commas and records by LF or CRLF; a field in double quotes may contain commas, line breaks and doubled quotes (`""`), and is kept exactly as written. Whitespace around an unquoted field is ignored, blank lines are skipped, a missing field is `""`, and an unterminated quoted field is an error.

Unquoted fields that are exactly an integer (`42`, `-7`), a decimal (`3.25`) or `true`/`false` are converted to `int`, `float` or `bool`; everything else stays a string, so no information is lost: `02134`, `+1`, `-0`, `1e5`, `.5`, integers beyond 64 bits and every quoted field are strings.

`options` is an object: `header: false` returns each record as an array of fields, `types: false` keeps every field a string.

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

Values containing commas, quotes or line breaks, with surrounding whitespace, or strings that `csv.parse` would convert (`"42"`, `"true"`) are quoted, so `csv.parse(csv.stringify(rows))` gives the rows back.

### csv.read(path, options?) -> array

Reads a CSV file from disk and parses it into an array of objects. Equivalent to `csv.parse(fs.read(path), options)`.

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
