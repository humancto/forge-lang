# Async Functions

Async functions are functions that execute asynchronously and must be awaited to retrieve their result. Forge provides dual syntax: `async fn` (classic) and `forge` (natural).

## Syntax

```
AsyncFnDef    = ("async" "fn" | "forge") Identifier "(" ParamList ")" Block
```

## Defining Async Functions

Classic syntax:

```forge
async fn fetch_data() {
    let resp = http.get("https://api.example.com/data")
    return resp
}
```

Natural syntax:

```forge
forge fetch_data() {
    let resp = http.get("https://api.example.com/data")
    return resp
}
```

Both forms are semantically identical. The parser produces the same `FnDef` AST node with `is_async: true`.

## Semantics

An async function is defined with the `is_async` flag set to `true` in the AST. In the current implementation, async functions are stored as regular function values. The `is_async` flag is recorded in the AST but the interpreter treats async functions identically to synchronous functions during definition.

The distinction becomes relevant at the call site: async functions are expected to be called with `await` (or `hold`) to retrieve their result. Without `await`, the function executes synchronously and its return value is available immediately.

## Registration

When an async function definition is executed, the function is registered in the environment as a `Value::Function` just like a synchronous function. The `is_async` flag from the AST does not alter the stored function value.

```forge
forge get_value() {
    return 42
}

// The function is callable like any other function
let result = get_value()       // 42 (runs synchronously)
let result = await get_value() // 42 (await passes through non-handle values)
```

## Parameters and Return Values

Async functions support the same parameter syntax as regular functions, including default values and type annotations:

```forge
async fn fetch_user(id: Int, timeout: Int = 30) {
    let resp = http.get("https://api.example.com/users/" + str(id))
    return json.parse(resp.body)
}
```

Return values follow the same rules as synchronous functions. The `return` statement provides an explicit return value; without it, the function returns `null` or the last expression's value.

## Combining with Spawn

For true concurrent execution, combine async functions with `spawn`:

```forge
forge compute(n) {
    // Expensive computation
    return n * n
}

let handle = spawn { return compute(42) }
let result = await handle    // 1764
```

The `spawn` keyword is what creates actual concurrency (a new OS thread). The `async`/`forge` keyword marks intent but does not itself create a new thread.

## Natural Syntax: forge

The `forge` keyword serves double duty as both the language name and the natural-syntax alias for `async fn`. In a function definition context, `forge` is parsed as an async function definition:

```forge
forge load_config() {
    let text = fs.read("config.json")
    return json.parse(text)
}
```

This reads naturally as "forge a load_config function" while being functionally equivalent to `async fn load_config()`.
