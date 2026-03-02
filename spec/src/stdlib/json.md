# json

JSON parsing and serialization.

## Functions

### json.parse(string) -> any

Parses a JSON string and returns the corresponding Forge value. Objects become Forge objects, arrays become Forge arrays, and JSON primitives map to their Forge equivalents.

```forge
let data = json.parse('{"name": "Forge", "version": 3}')
say data.name     // "Forge"
say data.version   // 3

let arr = json.parse("[1, 2, 3]")
say arr[0]  // 1
```

### json.stringify(value) -> string

Serializes a Forge value into a compact JSON string (no extra whitespace).

```forge
let obj = { name: "Forge", tags: ["fast", "fun"] }
let s = json.stringify(obj)
say s  // {"name": "Forge", "tags": ["fast", "fun"]}
```

### json.pretty(value, indent?) -> string

Serializes a Forge value into a pretty-printed JSON string. The optional `indent` parameter specifies the number of spaces per indentation level (default: 2).

```forge
let obj = { name: "Forge", version: 3 }
say json.pretty(obj)
// {
//   "name": "Forge",
//   "version": 3
// }

say json.pretty(obj, 4)
// {
//     "name": "Forge",
//     "version": 3
// }
```

## Type Mapping

| JSON                  | Forge    |
| --------------------- | -------- |
| `null`                | `null`   |
| `true` / `false`      | `bool`   |
| integer number        | `int`    |
| floating-point number | `float`  |
| `"string"`            | `string` |
| `[...]`               | `array`  |
| `{...}`               | `object` |
