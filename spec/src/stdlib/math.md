# math

Mathematical operations and constants. All trigonometric functions use radians.

## Constants

| Name       | Type    | Value             |
| ---------- | ------- | ----------------- |
| `math.pi`  | `float` | 3.141592653589793 |
| `math.e`   | `float` | 2.718281828459045 |
| `math.inf` | `float` | Infinity          |

```forge
say math.pi    // 3.141592653589793
say math.e     // 2.718281828459045
```

## Functions

### math.sqrt(n) -> float

Returns the square root of `n`.

```forge
math.sqrt(144)   // 12.0
math.sqrt(2)     // 1.4142135623730951
```

### math.pow(base, exp) -> int | float

Returns `base` raised to the power of `exp`. Returns `int` when both arguments are non-negative integers; returns `float` otherwise.

```forge
math.pow(2, 10)    // 1024
math.pow(2.0, 0.5) // 1.4142135623730951
math.pow(2, -1)    // 0.5
```

### math.abs(n) -> int | float

Returns the absolute value of `n`. Preserves the input type.

```forge
math.abs(-42)    // 42
math.abs(-3.14)  // 3.14
```

### math.max(a, b) -> int | float

Returns the greater of `a` and `b`.

```forge
math.max(10, 20)     // 20
math.max(3.14, 2.71) // 3.14
```

### math.min(a, b) -> int | float

Returns the lesser of `a` and `b`.

```forge
math.min(10, 20)     // 10
math.min(3.14, 2.71) // 2.71
```

### math.floor(n) -> int

Returns the largest integer less than or equal to `n`.

```forge
math.floor(3.7)   // 3
math.floor(-1.2)  // -2
math.floor(5)     // 5
```

### math.ceil(n) -> int

Returns the smallest integer greater than or equal to `n`.

```forge
math.ceil(3.2)    // 4
math.ceil(-1.8)   // -1
math.ceil(5)      // 5
```

### math.round(n) -> int

Returns the nearest integer to `n`, rounding half away from zero.

```forge
math.round(3.5)   // 4
math.round(3.4)   // 3
math.round(-2.5)  // -3
```

### math.random() -> float

Returns a pseudo-random float between 0.0 and 1.0 (exclusive).

```forge
let r = math.random()  // e.g. 0.482371...
```

### math.random_int(min, max) -> int

Returns a pseudo-random integer in the inclusive range `[min, max]`. Errors if `min > max`.

```forge
let die = math.random_int(1, 6)   // 1-6
let coin = math.random_int(0, 1)  // 0 or 1
```

### math.sin(n) -> float

Returns the sine of `n` (in radians).

```forge
math.sin(0)          // 0.0
math.sin(math.pi/2)  // 1.0
```

### math.cos(n) -> float

Returns the cosine of `n` (in radians).

```forge
math.cos(0)       // 1.0
math.cos(math.pi) // -1.0
```

### math.tan(n) -> float

Returns the tangent of `n` (in radians).

```forge
math.tan(0)          // 0.0
math.tan(math.pi/4)  // ~1.0
```

### math.log(n) -> float

Returns the natural logarithm (base _e_) of `n`.

```forge
math.log(1)       // 0.0
math.log(math.e)  // 1.0
```

### math.clamp(value, min, max) -> int | float

Clamps `value` to the range `[min, max]`.

```forge
math.clamp(5, 1, 10)    // 5
math.clamp(-5, 0, 10)   // 0
math.clamp(15, 0, 10)   // 10
```
