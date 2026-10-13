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

The `replacement` string can refer to capture groups: `$0` is the whole match, `$1`, `$2`, ... the numbered groups, `$name` or `${name}` a named group (`(?<name>...)` or `(?P<name>...)`), and `$$` is a literal `$`. A reference to a group that did not take part in the match is empty. A name runs as far as letters, digits and `_` go, so `$1a` refers to a group named `1a` (empty): write `${1}a`. In a `"..."` string `{name}` interpolates, so write `${name}` references in a triple-quoted string:

```forge
regex.replace("2024-01-15", """(?<y>\d{4})-(\d{2})""", """${y}/${2}""")
// "2024/01-15"
```

### regex.captures(text, pattern) -> array | null

Returns the capture groups of the first match of `pattern` in `text`, or `null` if there is no match. Element 0 is the whole match, then every group in the pattern from left to right (named groups are numbered too); a group that did not take part in the match is `null`.

```forge
regex.captures("user=ada id=7", "user=(\\w+) id=(\\d+)")
// ["user=ada id=7", "ada", "7"]

regex.captures("port", "(\\w+)(:(\\d+))?")
// ["port", "port", null, null]

regex.captures("nothing", "user=(\\w+)")
// null
```

### regex.captures_all(text, pattern) -> array

Returns one array of groups (as `regex.captures` returns it) per non-overlapping match, in order; `[]` when nothing matches.

```forge
regex.captures_all("a=1, b=22", "(\\w)=(\\d+)")
// [["a=1", "a", "1"], ["b=22", "b", "22"]]
```

### regex.split(text, pattern) -> array

Splits `text` by occurrences of `pattern` and returns an array of substrings.

```forge
regex.split("one,,two,,,three", ",+")
// ["one", "two", "three"]

regex.split("hello   world  foo", "\\s+")
// ["hello", "world", "foo"]
```
