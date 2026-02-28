# RFC 0002: HTTP as a Language Primitive

- **Status:** Implemented
- **Author:** Archith Rapaka
- **Date:** 2026-01-20

## Summary

HTTP server and client capabilities are built into the Forge language runtime. Developers define routes using function decorators and make requests using built-in functions — no external frameworks or libraries required.

## Motivation

In every mainstream language, building an HTTP server requires:

1. Choosing a framework (Express, Flask, Gin, Actix, etc.)
2. Installing it and its transitive dependencies
3. Learning its specific API, middleware model, and conventions
4. Managing version updates independently of the language

This is friction that doesn't need to exist. Every internet application serves HTTP. The language should support it natively.

## Design

### Server

Routes are declared using decorators on functions:

```
@server(port: 3000)

@get("/hello/:name")
fn hello(name: String) -> Json {
    return { greeting: "Hello, {name}!" }
}

@post("/users")
fn create_user(body: Json) -> Json {
    return { created: true, name: body.name }
}
```

The `@server` decorator configures the HTTP server. Route decorators (`@get`, `@post`, `@put`, `@delete`, `@ws`) bind functions to URL patterns. Route parameters (`:name`) become function parameters.

The server starts automatically after program execution if `@server` is present.

### Client

HTTP requests use built-in functions:

```
let resp = fetch("https://api.example.com/data")
say resp.status     // 200
say resp.json.name  // auto-parsed JSON

let data = http.post("https://api.example.com/users", { name: "Alice" })
```

The response object contains `status`, `ok`, `body`, `json` (auto-parsed), `headers`, `time`, and `method`.

### Implementation

- Server: powered by axum + tokio (production-grade, used by Cloudflare and Discord)
- Client: powered by reqwest + rustls (HTTPS via pure Rust TLS)
- JSON serialization: serde_json (zero-copy where possible)

## Alternatives Considered

### "Embed Express/Flask-style routing"

Rejected. Framework conventions vary wildly. Decorator-based routing is simple, declarative, and doesn't require understanding middleware chains.

### "Use a DSL for route definitions"

Rejected. Functions with decorators are more flexible — the handler is just a function, testable and composable like any other function.

### "Don't include a server — just a client"

Rejected. Forge is for building internet software. Most internet software serves HTTP. The server is essential.
