# Collection Functions

Functions for working with arrays, objects, and sequences.

## len(collection) -> int

Returns the length of a string, array, or object.

```forge
len("hello")       // 5
len([1, 2, 3])     // 3
len({a: 1, b: 2})  // 2
```

## push(array, value) -> array

Returns a new array with `value` appended to the end.

```forge
let a = [1, 2, 3]
let b = push(a, 4)
say b  // [1, 2, 3, 4]
```

> **Note:** `push` returns a new array. The original array is not modified.

## pop(array) -> array

Returns a new array with the last element removed.

```forge
let a = [1, 2, 3]
let b = pop(a)
say b  // [1, 2]
```

## keys(object) -> array

Returns an array of the object's keys as strings.

```forge
let obj = { name: "Alice", age: 30 }
say keys(obj)  // ["name", "age"]
```

## values(object) -> array

Returns an array of the object's values.

```forge
let obj = { name: "Alice", age: 30 }
say values(obj)  // ["Alice", 30]
```

## contains(collection, item) -> bool

Checks if a collection contains an item.

- **String, substring**: checks if the substring exists in the string.
- **Array, value**: checks if the value exists in the array.
- **Object, key**: checks if the key exists in the object.

```forge
contains("hello world", "world")  // true
contains([1, 2, 3], 2)            // true
contains({a: 1}, "a")             // true
contains({a: 1}, "b")             // false
```

## range(start, end, step?) -> array

Generates an array of integers from `start` (inclusive) to `end` (exclusive). Optional `step` defaults to 1.

```forge
range(0, 5)        // [0, 1, 2, 3, 4]
range(1, 10, 2)    // [1, 3, 5, 7, 9]
range(5, 0, -1)    // [5, 4, 3, 2, 1]
```

## enumerate(array) -> array

Returns an array of `[index, value]` pairs.

```forge
let names = ["Alice", "Bob", "Charlie"]
for pair in enumerate(names) {
    say str(pair[0]) + ": " + pair[1]
}
// 0: Alice
// 1: Bob
// 2: Charlie
```

## sum(array) -> int | float

Returns the sum of all numeric elements in an array.

```forge
sum([1, 2, 3, 4])    // 10
sum([1.5, 2.5])       // 4.0
```

## min_of(array) -> int | float

Returns the minimum value in an array.

```forge
min_of([3, 1, 4, 1, 5])  // 1
```

## max_of(array) -> int | float

Returns the maximum value in an array.

```forge
max_of([3, 1, 4, 1, 5])  // 5
```

## unique(array) -> array

Returns a new array with duplicate values removed, preserving order.

```forge
unique([1, 2, 2, 3, 1])  // [1, 2, 3]
```

## zip(array_a, array_b) -> array

Combines two arrays into an array of `[a, b]` pairs. Truncates to the shorter array's length.

```forge
zip([1, 2, 3], ["a", "b", "c"])
// [[1, "a"], [2, "b"], [3, "c"]]
```

## flatten(array) -> array

Flattens nested arrays by one level.

```forge
flatten([[1, 2], [3, 4], [5]])  // [1, 2, 3, 4, 5]
```

## group_by(array, fn) -> object

Groups array elements by the string returned by `fn`. Returns an object where keys are group names and values are arrays.

```forge
let people = [
    { name: "Alice", dept: "eng" },
    { name: "Bob", dept: "sales" },
    { name: "Charlie", dept: "eng" }
]
let groups = group_by(people, fn(p) { p.dept })
say keys(groups)  // ["eng", "sales"]
say groups.eng    // [{name: "Alice", dept: "eng"}, {name: "Charlie", dept: "eng"}]
```

## chunk(array, size) -> array

Splits an array into chunks of the given size.

```forge
chunk([1, 2, 3, 4, 5], 2)
// [[1, 2], [3, 4], [5]]
```

## slice(array, start, end?) -> array

Returns a sub-array from `start` (inclusive) to `end` (exclusive). If `end` is omitted, slices to the end.

```forge
slice([1, 2, 3, 4, 5], 1, 3)  // [2, 3]
slice([1, 2, 3, 4, 5], 2)     // [3, 4, 5]
```

## partition(array, fn) -> array

Splits an array into two arrays: elements where `fn` returns truthy, and elements where it returns falsy.

```forge
let nums = [1, 2, 3, 4, 5, 6]
let result = partition(nums, fn(n) { n % 2 == 0 })
say result[0]  // [2, 4, 6]  (even)
say result[1]  // [1, 3, 5]  (odd)
```

## Functional Operations

### map(array, fn) -> array

Applies `fn` to each element and returns the results.

```forge
map([1, 2, 3], fn(x) { x * 2 })  // [2, 4, 6]
```

### filter(array, fn) -> array

Returns elements where `fn` returns truthy.

```forge
filter([1, 2, 3, 4], fn(x) { x > 2 })  // [3, 4]
```

### reduce(array, initial, fn) -> any

Reduces an array to a single value by applying `fn(accumulator, element)` for each element.

```forge
reduce([1, 2, 3, 4], 0, fn(acc, x) { acc + x })  // 10
```

### sort(array, comparator?) -> array

Returns a sorted copy of the array. Without a comparator, sorts numbers numerically and strings alphabetically. The comparator function receives two elements and returns a negative number, zero, or positive number.

```forge
sort([3, 1, 4, 1, 5])  // [1, 1, 3, 4, 5]

sort(["banana", "apple", "cherry"])  // ["apple", "banana", "cherry"]

// Custom sort: descending
sort([1, 2, 3], fn(a, b) { b - a })  // [3, 2, 1]
```

### reverse(array) -> array

Returns a reversed copy of the array.

```forge
reverse([1, 2, 3])  // [3, 2, 1]
```

### find(array, fn) -> any | null

Returns the first element where `fn` returns truthy, or `null` if none match.

```forge
find([1, 2, 3, 4], fn(x) { x > 2 })  // 3
find([1, 2], fn(x) { x > 5 })         // null
```

### flat_map(array, fn) -> array

Maps each element with `fn` and flattens the result by one level.

```forge
flat_map([1, 2, 3], fn(x) { [x, x * 10] })
// [1, 10, 2, 20, 3, 30]
```

### any(array, fn) -> bool

Returns `true` if `fn` returns truthy for at least one element.

```forge
any([1, 2, 3], fn(x) { x > 2 })  // true
any([1, 2, 3], fn(x) { x > 5 })  // false
```

### all(array, fn) -> bool

Returns `true` if `fn` returns truthy for every element.

```forge
all([2, 4, 6], fn(x) { x % 2 == 0 })  // true
all([2, 3, 6], fn(x) { x % 2 == 0 })  // false
```

### sample(array, n?) -> any | array

Returns a random element (no arguments) or `n` random elements from the array.

```forge
sample([1, 2, 3, 4, 5])     // e.g. 3
sample([1, 2, 3, 4, 5], 2)  // e.g. [4, 1]
```

### shuffle(array) -> array

Returns a randomly shuffled copy of the array.

```forge
shuffle([1, 2, 3, 4, 5])  // e.g. [3, 5, 1, 4, 2]
```
