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

### http.pretty(response) -> null

Pretty-prints an HTTP response object to stderr with color formatting.

```forge
let resp = http.get("https://api.example.com/status")
http.pretty(resp)
```

## Server Decorators

Forge supports declarative HTTP servers using decorators. The `@server` decorator configures the server, and `@get`, `@post`, `@put`, `@delete` decorators define route handlers.

```forge
@server(port: 3000)

@get("/")
fn index() {
    return { message: "Welcome to Forge!" }
}

@get("/users/:id")
fn get_user(id) {
    return { id: id, name: "User " + id }
}

@post("/users")
fn create_user(body) {
    say "Creating user: " + body.name
    return { ok: true, name: body.name }
}

@delete("/users/:id")
fn delete_user(id) {
    return { deleted: id }
}
```

### Handler Parameters

Handler functions receive arguments based on parameter names:

- **Path parameters** (`:id`, `:name`) are passed by matching the parameter name.
- **`body`** or **`data`** receives the parsed JSON request body.
- **`query`** or **`qs`** receives query string parameters as an object.

### Server Features

- Built on **axum** and **tokio** for production-grade async performance.
- **CORS** is enabled by default (permissive policy).
- Return values are automatically serialized as JSON responses.
- WebSocket support via the `@ws` decorator.
