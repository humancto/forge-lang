# http

HTTP client for making requests and a decorator-based HTTP server built on axum + tokio.

## Client Functions

All client functions return a response object with the following fields:

| Field     | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `status`  | `int`    | HTTP status code                 |
| `body`    | `string` | Raw response body                |
| `json`    | `any`    | Parsed JSON body (if applicable) |
| `headers` | `object` | Response headers                 |
| `url`     | `string` | Final URL (after redirects)      |
| `time`    | `int`    | Response time in milliseconds    |
| `method`  | `string` | HTTP method used                 |

### http.get(url, options?) -> object

Sends an HTTP GET request.

```forge
let resp = http.get("https://api.example.com/users")
say resp.status  // 200
say resp.json    // [{id: 1, name: "Alice"}, ...]
```

### http.post(url, options?) -> object

Sends an HTTP POST request.

```forge
let resp = http.post("https://api.example.com/users", {
    body: { name: "Alice", email: "alice@example.com" }
})
say resp.status  // 201
```

### http.put(url, options?) -> object

Sends an HTTP PUT request.

```forge
let resp = http.put("https://api.example.com/users/1", {
    body: { name: "Alice Updated" }
})
```

### http.delete(url, options?) -> object

Sends an HTTP DELETE request.

```forge
let resp = http.delete("https://api.example.com/users/1")
say resp.status  // 204
```

### http.patch(url, options?) -> object

Sends an HTTP PATCH request.

```forge
let resp = http.patch("https://api.example.com/users/1", {
    body: { active: false }
})
```

### http.head(url, options?) -> object

Sends an HTTP HEAD request (headers only, no body).

```forge
let resp = http.head("https://example.com")
say resp.status
```

### Options Object

| Field     | Type     | Description                                         |
| --------- | -------- | --------------------------------------------------- |
| `body`    | `any`    | Request body (auto-serialized as JSON)              |
| `headers` | `object` | Custom request headers                              |
| `auth`    | `string` | Bearer token (sets `Authorization: Bearer <token>`) |
| `timeout` | `int`    | Timeout in seconds (default: 30)                    |

```forge
let resp = http.get("https://api.example.com/me", {
    auth: "my-secret-token",
    headers: { "Accept": "application/json" },
    timeout: 10
})
```

### http.download(url, destination?) -> object

Downloads a file from `url` and saves it to `destination`. If no destination is provided, the filename is derived from the URL. Returns an object with `path`, `size`, and `status`.

```forge
let result = http.download("https://example.com/data.zip", "data.zip")
say result.size  // bytes downloaded
```

### http.crawl(url) -> object

Fetches a web page and extracts structured data. Returns an object with:

| Field         | Type     | Description                                     |
| ------------- | -------- | ----------------------------------------------- |
| `url`         | `string` | The URL crawled                                 |
| `status`      | `int`    | HTTP status code                                |
| `title`       | `string` | Page title                                      |
| `description` | `string` | Meta description                                |
| `links`       | `array`  | Array of absolute URLs found in href attributes |
| `text`        | `string` | Visible text content (first 500 characters)     |
| `html_length` | `int`    | Total HTML length in characters                 |

```forge
let page = http.crawl("https://example.com")
say page.title
say len(page.links)
```

### http.response(status, body?, headers?) -> object

Builds the response an HTTP handler returns when it needs a status code other than 200, response headers, or a body that is not JSON (see [Server Decorators](#server-decorators)).

- `status` — an Int from 200 to 599.
- `body` — `null` (the default) sends no body; a String is sent as is, with `Content-Type: text/plain; charset=utf-8` unless `headers` sets one; any other value is sent as JSON (`application/json` unless set).
- `headers` — an object of header names to values. A value may be a String, a number or a Bool, or an array of them (the header is sent once per element, e.g. `Set-Cookie`).

Invalid arguments raise an error when the response is built: a status outside 200-599, a body with status 204 or 304, an invalid header name, a header value containing a line break or other control character (so a value can never inject a header), and `Content-Length`, `Transfer-Encoding` or `Connection`, which the server sets.

```forge
@post("/items")
fn create(body) {
    return http.response(201, { id: 7 }, { Location: "/items/7" })
}

@get("/health")
fn health() {
    return http.response(200, "ok")          // text/plain
}

@delete("/items/:id")
fn remove(id) {
    return http.response(204)                // no body
}
```

The result is an object (`{ status, headers, body }` tagged as an `HttpResponse`); a handler may adjust its fields before returning it. It is validated again when it is sent; an invalid one answers 500.

### http.pretty(response) -> null

Pretty-prints an HTTP response object to stderr with color formatting.

```forge
let resp = http.get("https://api.example.com/status")
http.pretty(resp)
```

## Server Decorators

Forge supports declarative HTTP servers using decorators. The `@server` decorator configures the server, and `@get`, `@post`, `@put`, `@patch`, `@delete` decorators define route handlers.

```forge
@server(port: 3000)

@get("/")
fn index() {
    return { message: "Welcome to Forge!" }
}

@get("/users/:id")
fn get_user(id) {
    if id == "0" {
        return http.response(404, { error: "no such user" })
    }
    return { id: id, name: "User " + id }
}

@post("/users")
fn create_user(body, headers) {
    if get(headers, "authorization") == null {
        return http.response(401, { error: "unauthorized" })
    }
    return http.response(201, { ok: true, name: body.name }, { Location: "/users/1" })
}

@delete("/users/:id")
fn delete_user(id) {
    return http.response(204)
}
```

### Handler Parameters

Handler functions receive arguments based on parameter names:

- **Path parameters** (`:id`, `:name`) are passed by matching the parameter name.
- **`body`** or **`data`** receives the request body (`{}` when the request has none).
- **`query`** or **`qs`** receives query string parameters as an object.
- **`headers`** receives the request headers as an object with lower-case names (repeated headers joined with `, `).
- **`req`** or **`request`** receives the whole request: `{ method, path, params, query, headers, body }` (`body` is `null` when there is none).
- Any other name receives the query parameter of that name, or `null`.

Path, query and header values are Strings. Use `get(headers, "x-name")` for an optional header: indexing a missing key is an error.

### Request Bodies

The body is decoded by its `Content-Type`:

| Content type | `body` |
| --- | --- |
| `application/json`, `*/*+json` | the parsed JSON value |
| `application/x-www-form-urlencoded` | an object of Strings |
| anything else, or none | the body as a String |

Invalid JSON is answered `400` and a body that is not UTF-8 text `415`, with a JSON `{"error": ...}` body, before the handler runs. Bodies are limited to 2 MiB (`413`).

### Responses

A returned value is sent as `200` JSON. `http.response(status, body?, headers?)` sets the status, headers and body. A runtime error in a handler answers `500 {"error": message}`.

### Databases

A database the program's top level opens (`db.open`, `pg.connect`) is shared by all handlers. Statements of concurrent requests run one at a time on the connection; a transaction (`db.begin()` ... `db.commit()`, or `BEGIN` in SQL) belongs to the request that opened it — other requests wait for it — and is rolled back if the request ends without committing. `db.last_insert_rowid()` returns the request's own last insert. `db.open` or `db.close` inside a handler affects that request only.

### Server Features

- Built on **axum** and **tokio**; each request runs in an isolated fork of the program, concurrently with other requests.
- **CORS** is same-origin by default; `@server(cors: "permissive")` allows any origin.
- WebSocket support via the `@ws` decorator.
