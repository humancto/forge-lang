# String Functions

Functions for string manipulation. All string functions are non-mutating and return new strings.

## split(string, delimiter) -> array

Splits a string by the given delimiter.

```forge
split("a,b,c", ",")     // ["a", "b", "c"]
split("hello world", " ") // ["hello", "world"]
```

## join(array, separator?) -> string

Joins array elements into a string with an optional separator. Default separator is empty string.

```forge
join(["a", "b", "c"], ", ")  // "a, b, c"
join(["x", "y", "z"])        // "xyz"
```

## replace(string, from, to) -> string

Replaces all occurrences of `from` with `to`.

```forge
replace("hello world", "world", "Forge")  // "hello Forge"
replace("aabbcc", "bb", "XX")             // "aaXXcc"
```

## starts_with(string, prefix) -> bool

Returns `true` if `string` begins with `prefix`.

```forge
starts_with("hello", "hel")   // true
starts_with("hello", "world") // false
```

## ends_with(string, suffix) -> bool

Returns `true` if `string` ends with `suffix`.

```forge
ends_with("hello.fg", ".fg")  // true
ends_with("hello.fg", ".rs")  // false
```

## lines(string) -> array

Splits a string into an array of lines.

```forge
lines("first\nsecond\nthird")
// ["first", "second", "third"]
```

## substring(string, start, end?) -> string

Extracts a substring from `start` (inclusive) to `end` (exclusive). If `end` is omitted, extracts to the end of the string.

```forge
substring("hello world", 0, 5)   // "hello"
substring("hello world", 6)      // "world"
```

## index_of(string, search) -> int

Returns the index of the first occurrence of `search` in `string`, or -1 if not found.

```forge
index_of("hello world", "world")  // 6
index_of("hello world", "xyz")    // -1
```

## last_index_of(string, search) -> int

Returns the index of the last occurrence of `search` in `string`, or -1 if not found.

```forge
last_index_of("abcabc", "abc")  // 3
last_index_of("hello", "xyz")   // -1
```

## pad_start(string, length, char?) -> string

Pads the beginning of a string to reach the target `length`. Default pad character is a space.

```forge
pad_start("42", 5, "0")    // "00042"
pad_start("hi", 10)        // "        hi"
```

## pad_end(string, length, char?) -> string

Pads the end of a string to reach the target `length`. Default pad character is a space.

```forge
pad_end("hi", 10, ".")     // "hi........"
pad_end("test", 8)         // "test    "
```

## capitalize(string) -> string

Returns the string with the first character in uppercase.

```forge
capitalize("hello")  // "Hello"
capitalize("HELLO")  // "HELLO"
```

## title(string) -> string

Returns the string with the first character of each word capitalized.

```forge
title("hello world")        // "Hello World"
title("the quick brown fox") // "The Quick Brown Fox"
```

## repeat_str(string, count) -> string

Returns the string repeated `count` times.

```forge
repeat_str("ha", 3)    // "hahaha"
repeat_str("-", 20)    // "--------------------"
```

## count(string, substring) -> int

Counts the number of non-overlapping occurrences of `substring` in `string`.

```forge
count("banana", "an")   // 2
count("hello", "l")     // 2
```

## slugify(string) -> string

Converts a string to a URL-friendly slug: lowercase, non-alphanumeric characters replaced with hyphens.

```forge
slugify("Hello World!")           // "hello-world"
slugify("The Quick Brown Fox")    // "the-quick-brown-fox"
```

## snake_case(string) -> string

Converts a string to snake_case. Handles camelCase, PascalCase, and spaces.

```forge
snake_case("helloWorld")     // "hello_world"
snake_case("MyComponent")    // "my_component"
snake_case("some string")    // "some_string"
```

## camel_case(string) -> string

Converts a string to camelCase.

```forge
camel_case("hello_world")    // "helloWorld"
camel_case("some string")    // "someString"
```
