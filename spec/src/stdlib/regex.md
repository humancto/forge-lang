# regex

Regular expression operations. Uses Rust's `regex` crate syntax.

> **Important:** All `regex` functions take the **text first, pattern second**: `regex.test(text, pattern)`. This is the opposite of many other languages.

## Functions

### regex.test(text, pattern) -> bool

Returns `true` if `pattern` matches anywhere in `text`.

```forge
regex.test("hello world", "world")     // true
regex.test("hello world", "^world")    // false
regex.test("abc123", "\\d+")           // true
```

### regex.find(text, pattern) -> string | null

Returns the first match of `pattern` in `text`, or `null` if no match.

```forge
regex.find("order-4521-confirmed", "\\d+")  // "4521"
regex.find("no numbers here", "\\d+")       // null
```

### regex.find_all(text, pattern) -> array

Returns an array of all non-overlapping matches of `pattern` in `text`.

```forge
regex.find_all("call 555-1234 or 555-5678", "\\d{3}-\\d{4}")
// ["555-1234", "555-5678"]

regex.find_all("aabbaab", "a+")
// ["aa", "aa"]
```

### regex.replace(text, pattern, replacement) -> string

Replaces all occurrences of `pattern` in `text` with `replacement`.

```forge
regex.replace("hello world", "world", "Forge")
// "hello Forge"

regex.replace("2024-01-15", "(\\d{4})-(\\d{2})-(\\d{2})", "$2/$3/$1")
// "01/15/2024"
```

The `replacement` string supports capture group references (`$1`, `$2`, etc.).

### regex.split(text, pattern) -> array

Splits `text` by occurrences of `pattern` and returns an array of substrings.

```forge
regex.split("one,,two,,,three", ",+")
// ["one", "two", "three"]

regex.split("hello   world  foo", "\\s+")
// ["hello", "world", "foo"]
```
