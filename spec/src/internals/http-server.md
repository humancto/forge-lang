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

### @patch(path?)

Registers a function as a PATCH handler.

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

Handler functions receive arguments based on their parameter names (`runtime::server::handler_args`, shared by both engines):

| Parameter Name              | Source                                                        |
| --------------------------- | ------------------------------------------------------------- |
| Name matching a path param  | URL path parameter                                            |
| `body` or `data`            | Request body, decoded by content type (`{}` when absent)      |
| `query` or `qs`             | Query string as an object                                     |
| `headers`                   | Request headers, lower-case names                             |
| `req` or `request`          | `{ method, path, params, query, headers, body }`              |
| Name matching a query param | Individual query parameter                                    |
| Other                       | `null`                                                        |

```forge
@post("/search")
fn search(body, query, req) {
    say "Search body: " + str(body)
    say "Query params: " + str(query)
    say "Method: " + req.method
    return { results: [] }
}
```

Request bodies are decoded before the handler runs (`runtime::http_response::parse_body`): JSON content types are parsed (`400` if invalid), `application/x-www-form-urlencoded` becomes an object of Strings, and anything else is a String (`415` if it is not UTF-8). These errors, like every error the server answers itself, have a JSON `{"error": ...}` body.

## Responses

A handler's return value is encoded as JSON on the engine that ran it, and the server builds the response from that JSON, so both engines answer byte-identically. A value built by `http.response(status, body?, headers?)` is an object tagged `__type__: "HttpResponse"`; the server validates it (`ResponseSpec::from_json`) and sends its status, headers and body (`null`: no body; String: as is, `text/plain` by default; anything else: JSON). Every other value is a `200` JSON body.

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

An invalid response value (an out-of-range status, a header value with a line break, ...) also answers 500, with `invalid http.response: ...`.

## CORS

CORS is same-origin by default. `@server(cors: "permissive")` allows any origin, method and header.

## Concurrency Model

Each request runs in its own fork of the program's state after the top level ran (a frozen VM template, or the interpreter's `fork_for_serving`), on tokio's blocking pool. Handlers run in parallel; mutations a handler makes to top-level variables are private to its request.

Database connections opened by the top level are shared by the forks (`stdlib::db_share`): a mutex serializes statements, and a transaction belongs to the request that opened it (other requests wait for it, at most 30 seconds) and is rolled back when that request ends.
