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

## Numbers

A number keeps its kind in both directions, on both engines and in HTTP
responses:

- **Parsing.** A number written without a fraction or exponent is an Int when
  it fits in 64 bits. `-0` is the Int `0`, as `-0` is in Forge source. An
  integer outside the Int range becomes a Float (and may lose precision), the
  way Int arithmetic that overflows becomes a Float (Forge source rejects such
  a literal). A number with a fraction or an exponent is a Float: `1.0`,
  `1e2` (`100.0`), `-0.0`.
- **Writing.** An Int is written as digits; a Float always has a fraction or
  an exponent (`1.0`, `2.5`, `1e+21`), with the shortest digits that read back
  as the same Float, so `json.parse(json.compact(x)) == x` keeps every Float a
  Float. NaN and the infinities, which JSON cannot represent, are written as
  `null` (as in JavaScript). `str(1.0)` and `println(1.0)` still show `1`.

## Strings

`json.stringify`, `json.compact` and `json.pretty` escape `"` and `\`, write
the short escapes `\b`, `\f`, `\n`, `\r`, `\t`, and any other control
character as `\u00XX`; every other character is written as is (UTF-8).
`json.pretty`'s indent is an Int from 0 to 16 (default 2).
