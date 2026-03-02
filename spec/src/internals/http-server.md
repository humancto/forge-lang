# HTTP Server

Forge includes a built-in HTTP server powered by **axum** and **tokio**. Servers are defined declaratively using decorators and launched automatically when the interpreter detects a `@server` directive.

## Architecture

```
Forge Source
    |
    v
Parser extracts decorators (@server, @get, @post, ...)
    |
    v
runtime/server.rs builds axum Router
    |
    v
axum + tokio serve requests
    |
    v
Each request locks the Interpreter mutex,
calls the handler function, serializes the return value as JSON
```

The server implementation lives in `src/runtime/server.rs` (~354 lines).

## Server Configuration

The `@server` decorator configures the server:

```forge
@server(port: 3000, host: "0.0.0.0")
```

| Parameter | Type     | Default     | Description  |
| --------- | -------- | ----------- | ------------ |
| `port`    | `int`    | 8080        | Listen port  |
| `host`    | `string` | "127.0.0.1" | Bind address |

## Route Decorators

### @get(path?)

Registers a function as a GET handler.

```forge
@get("/users")
fn list_users() {
    return [{ name: "Alice" }, { name: "Bob" }]
}
```

### @post(path?)

Registers a function as a POST handler.

```forge
@post("/users")
fn create_user(body) {
    say "Creating: " + body.name
    return { ok: true }
}
```

### @put(path?)

Registers a function as a PUT handler.

### @delete(path?)

Registers a function as a DELETE handler.

### @ws(path?)

Registers a function as a WebSocket handler. The function receives each incoming message as a string and returns a response string.

```forge
@ws("/chat")
fn handle_message(msg) {
    return "Echo: " + msg
}
```

## Path Parameters

Path parameters use colon syntax (`:param`). They are automatically mapped to function parameters by name.

```forge
@get("/users/:id")
fn get_user(id) {
    return { id: id, name: "User " + id }
}
// GET /users/42 -> { id: "42", name: "User 42" }
```

The Forge path syntax (`:id`) is internally converted to axum's brace syntax (`{id}`).

## Handler Parameters

Handler functions receive arguments based on their parameter names:

| Parameter Name              | Source                     |
| --------------------------- | -------------------------- |
| Name matching a path param  | URL path parameter         |
| `body` or `data`            | Parsed JSON request body   |
| `query` or `qs`             | Query string as an object  |
| Name matching a query param | Individual query parameter |
| Other                       | `null`                     |

```forge
@post("/search")
fn search(body, query) {
    say "Search body: " + str(body)
    say "Query params: " + str(query)
    return { results: [] }
}
```

## JSON Serialization

Return values from handlers are automatically serialized to JSON:

| Forge Type     | JSON           |
| -------------- | -------------- |
| `int`          | number         |
| `float`        | number         |
| `bool`         | boolean        |
| `string`       | string         |
| `null`         | null           |
| `array`        | array          |
| `object`       | object         |
| `ResultOk(v)`  | `{"Ok": v}`    |
| `ResultErr(v)` | `{"Err": v}`   |
| Other          | `"<TypeName>"` |

## Error Handling

If a handler function throws a runtime error, the server returns HTTP 500 with:

```json
{ "error": "error message here" }
```

## CORS

CORS is enabled by default with a permissive policy (all origins, all methods, all headers). This is suitable for development; production deployments should add appropriate restrictions at the reverse proxy level.

## Concurrency Model

The interpreter is wrapped in an `Arc<Mutex<Interpreter>>`. Each incoming request acquires the mutex lock, calls the handler, and releases it. This means handlers execute serially -- only one request is processed at a time.

For high-throughput scenarios, consider running multiple Forge server instances behind a load balancer.
