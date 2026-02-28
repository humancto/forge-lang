# Part III: Building Real Things

---

## Chapter 20: Building REST APIs

Every modern application needs an API. Whether you're building a mobile backend, a
microservice, or a simple webhook receiver, the ability to stand up an HTTP server quickly
and cleanly is a superpower. Forge makes this trivially easy with its decorator-based
routing model—no framework boilerplate, no configuration files, no dependency management.
You write your handlers, attach decorators, and you have a production-ready API server
backed by Rust's Axum and Tokio under the hood.

### The Decorator-Based Routing Model

Forge's API server is activated by two things: the `@server` decorator that configures
your server, and route decorators (`@get`, `@post`, `@put`, `@delete`, `@ws`) that bind
functions to HTTP endpoints.

Here is the minimal shape of every Forge API:

```forge
@server(port: 8080)

@get("/hello")
fn hello() -> Json {
    return { message: "Hello from Forge!" }
}
```

The `@server(port: 8080)` decorator tells the Forge runtime to start an HTTP server on
port 8080 after evaluating the file. The `@get("/hello")` decorator registers the
`hello` function as a handler for `GET /hello`. The `-> Json` return type annotation
tells the framework to serialize the return value as a JSON response with the appropriate
`Content-Type` header.

These are not magic comments. They are first-class syntax elements parsed by the Forge
compiler and used by the runtime to build an Axum router. CORS is enabled by default
(permissive mode), so your API works out of the box with browser clients.

### Route Parameters and Query Strings

Route parameters use the `:param` syntax familiar from Express and Sinatra. Any segment
prefixed with a colon becomes a named parameter extracted from the URL path and passed
to your handler function as a `String` argument:

```forge
@get("/users/:id")
fn get_user(id: String) -> Json {
    return { user_id: id }
}
```

A request to `GET /users/42` calls `get_user` with `id` set to `"42"`. You can have
multiple route parameters:

```forge
@get("/repos/:owner/:name")
fn get_repo(owner: String, name: String) -> Json {
    return { owner: owner, repo: name }
}
```

Query strings are also available. Forge automatically parses query parameters and makes
them accessible through the function's parameters. Parameters not matched by route
segments are looked up in the query string.

### Request Bodies (POST/PUT)

For `POST` and `PUT` routes, Forge automatically parses the JSON request body and
passes it to your handler as a `body` parameter of type `Json`:

```forge
@post("/users")
fn create_user(body: Json) -> Json {
    let name = body.name
    return { created: true, name: name }
}
```

The `body` parameter is a Forge object parsed from the incoming JSON. You access its
fields with dot notation, exactly like any other Forge object.

### WebSocket Support

Forge supports WebSocket endpoints with the `@ws` decorator. A WebSocket handler
receives each incoming text message as a string parameter and returns a response string:

```forge
@ws("/chat")
fn chat(message: String) -> Json {
    return { echo: message }
}
```

When a WebSocket client connects to `/chat` and sends a message, Forge calls your
handler with the message text and sends back the return value. This makes it trivial
to build real-time features.

### Project 1: Hello API — Simple Greeting Service

Let's start with a complete, runnable API that demonstrates routing fundamentals.

```forge
// hello_api.fg — Your first Forge REST API
// Run:  forge run hello_api.fg
// Test: curl http://localhost:3000/hello/World

@server(port: 3000)

@get("/")
fn index() -> Json {
    return {
        name: "Hello API",
        version: "1.0.0",
        endpoints: ["/hello/:name", "/health", "/time"]
    }
}

@get("/hello/:name")
fn hello(name: String) -> Json {
    let greeting = "Hello, {name}!"
    return { greeting: greeting, language: "Forge" }
}

@get("/health")
fn health() -> Json {
    return { status: "ok" }
}

@get("/time")
fn time() -> Json {
    let now = sh("date -u +%Y-%m-%dT%H:%M:%SZ")
    return { utc: now }
}

@post("/echo")
fn echo(body: Json) -> Json {
    return body
}

@get("/add/:a/:b")
fn add(a: String, b: String) -> Json {
    let x = int(a)
    let y = int(b)
    let sum = x + y
    return { a: x, b: y, sum: sum }
}

say "Hello API starting on http://localhost:3000"
```

Save this as `hello_api.fg` and run it:

```bash
$ forge run hello_api.fg
Hello API starting on http://localhost:3000
Forge server listening on 0.0.0.0:3000
```

Now test each endpoint:

```bash
# Root endpoint — API discovery
$ curl -s http://localhost:3000/ | python3 -m json.tool
{
    "name": "Hello API",
    "version": "1.0.0",
    "endpoints": ["/hello/:name", "/health", "/time"]
}

# Greeting with route parameter
$ curl -s http://localhost:3000/hello/Forge
{"greeting":"Hello, Forge!","language":"Forge"}

# Health check
$ curl -s http://localhost:3000/health
{"status":"ok"}

# Arithmetic via route params
$ curl -s http://localhost:3000/add/17/25
{"a":17,"b":25,"sum":42}

# POST echo — sends back whatever you send
$ curl -s -X POST http://localhost:3000/echo \
  -H "Content-Type: application/json" \
  -d '{"message":"ping"}'
{"message":"ping"}
```

**Walkthrough.** The `@server(port: 3000)` line configures the port. Each `@get` or
`@post` decorator binds a handler function to an HTTP method and path. Route parameters
like `:name`, `:a`, and `:b` become function arguments. The `-> Json` return type
tells Forge to serialize the returned object as JSON. The `say` statement at the bottom
executes during startup, before the server begins accepting connections.

### Project 2: Notes API — Full CRUD with SQLite

This project builds a complete note-taking API with persistent storage. It demonstrates
all four CRUD operations, database integration, and error handling.

```forge
// notes_api.fg — Full CRUD REST API with SQLite
// Run:  forge run notes_api.fg
// Data persists in notes.db between restarts

@server(port: 3000)

// Initialize database on startup
db.open("notes.db")
db.execute("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, body TEXT NOT NULL, created_at TEXT DEFAULT CURRENT_TIMESTAMP)")

// List all notes
@get("/notes")
fn list_notes() -> Json {
    let notes = db.query("SELECT * FROM notes ORDER BY created_at DESC")
    return { count: len(notes), notes: notes }
}

// Get a single note by ID
@get("/notes/:id")
fn get_note(id: String) -> Json {
    let query_str = "SELECT * FROM notes WHERE id = {id}"
    let results = db.query(query_str)
    if len(results) == 0 {
        return { error: "Note not found", id: id }
    }
    return results[0]
}

// Create a new note
@post("/notes")
fn create_note(body: Json) -> Json {
    let title = body.title
    let note_body = body.body
    if title == null {
        return { error: "title is required" }
    }
    if note_body == null {
        return { error: "body is required" }
    }
    let stmt = "INSERT INTO notes (title, body) VALUES ('{title}', '{note_body}')"
    db.execute(stmt)
    let results = db.query("SELECT * FROM notes ORDER BY id DESC LIMIT 1")
    return { created: true, note: results[0] }
}

// Update an existing note
@put("/notes/:id")
fn update_note(id: String, body: Json) -> Json {
    let check = db.query("SELECT * FROM notes WHERE id = {id}")
    if len(check) == 0 {
        return { error: "Note not found", id: id }
    }
    let title = body.title
    let note_body = body.body
    if title == null {
        return { error: "title is required" }
    }
    if note_body == null {
        return { error: "body is required" }
    }
    let stmt = "UPDATE notes SET title = '{title}', body = '{note_body}' WHERE id = {id}"
    db.execute(stmt)
    let results = db.query("SELECT * FROM notes WHERE id = {id}")
    return { updated: true, note: results[0] }
}

// Delete a note
@delete("/notes/:id")
fn delete_note(id: String) -> Json {
    let check = db.query("SELECT * FROM notes WHERE id = {id}")
    if len(check) == 0 {
        return { error: "Note not found", id: id }
    }
    db.execute("DELETE FROM notes WHERE id = {id}")
    return { deleted: true, id: id }
}

// API info
@get("/")
fn api_info() -> Json {
    return {
        name: "Notes API",
        version: "1.0.0",
        endpoints: [
            "GET    /notes       - List all notes",
            "GET    /notes/:id   - Get a note",
            "POST   /notes       - Create a note",
            "PUT    /notes/:id   - Update a note",
            "DELETE /notes/:id   - Delete a note"
        ]
    }
}

say term.bold("Notes API")
say "Listening on http://localhost:3000"
say "Database: notes.db"
```

Test the complete CRUD lifecycle:

```bash
# Create a note
$ curl -s -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{"title":"First Note","body":"Hello from Forge!"}' | python3 -m json.tool
{
    "created": true,
    "note": {
        "id": 1,
        "title": "First Note",
        "body": "Hello from Forge!",
        "created_at": "2026-02-28 12:00:00"
    }
}

# Create another note
$ curl -s -X POST http://localhost:3000/notes \
  -H "Content-Type: application/json" \
  -d '{"title":"Second Note","body":"Forge makes APIs easy."}'

# List all notes
$ curl -s http://localhost:3000/notes | python3 -m json.tool
{
    "count": 2,
    "notes": [
        {"id": 2, "title": "Second Note", "body": "Forge makes APIs easy.", ...},
        {"id": 1, "title": "First Note", "body": "Hello from Forge!", ...}
    ]
}

# Update a note
$ curl -s -X PUT http://localhost:3000/notes/1 \
  -H "Content-Type: application/json" \
  -d '{"title":"First Note (edited)","body":"Updated content."}'

# Delete a note
$ curl -s -X DELETE http://localhost:3000/notes/2
{"deleted":true,"id":"2"}

# Verify deletion
$ curl -s http://localhost:3000/notes
{"count":1,"notes":[...]}
```

**Architecture of the Notes API:**

```
                    ┌──────────────────────────────────────────┐
                    │            notes_api.fg                  │
                    │                                          │
  HTTP Request ───► │  @server(port: 3000)                     │
                    │                                          │
                    │  ┌─────────────┐   ┌──────────────────┐  │
                    │  │ Route Layer │──►│ Handler Function │  │
                    │  │ @get, @post │   │ fn list_notes()  │  │
                    │  │ @put, @del  │   │ fn create_note() │  │
                    │  └─────────────┘   └────────┬─────────┘  │
                    │                             │            │
                    │                    ┌────────▼─────────┐  │
                    │                    │ SQLite Database  │  │
                    │                    │   notes.db       │  │
                    │                    └──────────────────┘  │
                    └──────────────────────────────────────────┘
```

### Project 3: URL Shortener — Complete Service with Database

A URL shortener is a classic API project that combines database operations, string
manipulation, and clean API design. This version generates short codes using a
hash-based approach.

```forge
// shortener.fg — URL Shortener API
// Run:  forge run shortener.fg
// Test: curl -X POST http://localhost:3000/shorten -d '{"url":"https://example.com"}'

@server(port: 3000)

// Initialize database
db.open("urls.db")
db.execute("CREATE TABLE IF NOT EXISTS urls (code TEXT PRIMARY KEY, original TEXT NOT NULL, clicks INTEGER DEFAULT 0, created_at TEXT DEFAULT CURRENT_TIMESTAMP)")

// Generate a short code from a URL
fn make_code(url) {
    let hash = crypto.sha256(url)
    let code = slice(hash, 0, 7)
    return code
}

// Shorten a URL
@post("/shorten")
fn shorten(body: Json) -> Json {
    let url = body.url
    if url == null {
        return { error: "url is required" }
    }
    let code = make_code(url)
    let existing = db.query("SELECT * FROM urls WHERE code = '{code}'")
    if len(existing) > 0 {
        return { code: code, short_url: "http://localhost:3000/r/{code}", existed: true }
    }
    db.execute("INSERT INTO urls (code, original) VALUES ('{code}', '{url}')")
    return { code: code, short_url: "http://localhost:3000/r/{code}", created: true }
}

// Redirect (returns the original URL — client follows it)
@get("/r/:code")
fn redirect(code: String) -> Json {
    let results = db.query("SELECT * FROM urls WHERE code = '{code}'")
    if len(results) == 0 {
        return { error: "Short URL not found" }
    }
    db.execute("UPDATE urls SET clicks = clicks + 1 WHERE code = '{code}'")
    let original = results[0].original
    return { redirect: original }
}

// Stats for a short URL
@get("/stats/:code")
fn stats(code: String) -> Json {
    let results = db.query("SELECT * FROM urls WHERE code = '{code}'")
    if len(results) == 0 {
        return { error: "Short URL not found" }
    }
    return results[0]
}

// List all shortened URLs
@get("/urls")
fn list_urls() -> Json {
    let urls = db.query("SELECT * FROM urls ORDER BY created_at DESC")
    return { count: len(urls), urls: urls }
}

// Landing page
@get("/")
fn landing() -> Json {
    return {
        service: "Forge URL Shortener",
        usage: "POST /shorten with {url: 'https://...'}",
        endpoints: [
            "POST /shorten     - Create short URL",
            "GET  /r/:code     - Resolve short URL",
            "GET  /stats/:code - View click stats",
            "GET  /urls        - List all URLs"
        ]
    }
}

say term.bold("Forge URL Shortener")
say "Running on http://localhost:3000"
```

Test the shortener end-to-end:

```bash
# Shorten a URL
$ curl -s -X POST http://localhost:3000/shorten \
  -H "Content-Type: application/json" \
  -d '{"url":"https://github.com/forge-lang/forge"}' | python3 -m json.tool
{
    "code": "a1b2c3d",
    "short_url": "http://localhost:3000/r/a1b2c3d",
    "created": true
}

# Resolve the short URL
$ curl -s http://localhost:3000/r/a1b2c3d
{"redirect":"https://github.com/forge-lang/forge"}

# Check click stats
$ curl -s http://localhost:3000/stats/a1b2c3d
{"code":"a1b2c3d","original":"https://github.com/forge-lang/forge","clicks":1,...}

# List all URLs
$ curl -s http://localhost:3000/urls
{"count":1,"urls":[...]}
```

### Error Handling in API Routes

Forge API handlers return JSON objects. To signal errors, return an object with an
`error` field. While Forge does not yet support setting custom HTTP status codes from
handlers (the runtime always returns 200), you can structure your responses to
distinguish success from failure:

```forge
@get("/users/:id")
fn get_user(id: String) -> Json {
    let results = db.query("SELECT * FROM users WHERE id = {id}")
    if len(results) == 0 {
        return { error: "not_found", message: "No user with that ID" }
    }
    return { ok: true, user: results[0] }
}
```

Client code can check for the `error` field to determine whether the request succeeded.

### CORS and Production Considerations

Forge enables permissive CORS by default—all origins, methods, and headers are allowed.
This is ideal for development but should be locked down in production. For production
deployments, consider placing your Forge API behind a reverse proxy like Nginx or
Caddy that handles TLS termination, rate limiting, and CORS policies.

```
                     ┌───────────┐      ┌───────────────┐
  Client ──HTTPS──► │   Nginx   │─HTTP─►│  Forge API    │
                     │  :443     │      │  :3000        │
                     │  TLS      │      │  CORS off     │
                     │  Rate lim │      │  Internal     │
                     └───────────┘      └───────────────┘
```

The server binds to `0.0.0.0` by default, accepting connections on all interfaces.
To bind to localhost only, pass the `host` argument:

```forge
@server(port: 3000, host: "127.0.0.1")
```

### Going Further

- **Middleware patterns.** Use helper functions called at the top of handlers to validate
  authentication tokens, check rate limits, or log requests before processing.
- **Database migrations.** Run `CREATE TABLE IF NOT EXISTS` statements at startup as
  shown in the Notes API. For schema changes, add `ALTER TABLE` statements guarded by
  version checks.
- **API versioning.** Use path prefixes like `/v1/notes` and `/v2/notes` with separate
  handler functions.
- **WebSocket chat.** Combine the `@ws` decorator with database-backed message history
  to build a real-time chat application.

---

## Chapter 21: HTTP Client and Web Automation

Building APIs is only half the story. Modern applications also _consume_ APIs—pulling
data from GitHub, checking service health, downloading files, and scraping web content.
Forge's HTTP client capabilities turn these tasks into one-liners. Where other languages
require installing libraries, importing modules, and managing async runtimes, Forge gives
you `fetch()` and the `http` module as built-in primitives.

### fetch() Basics

The `fetch()` function is Forge's Swiss Army knife for HTTP requests. At its simplest,
it takes a URL and returns a response object:

```forge
let resp = fetch("https://api.github.com/zen")
say resp.body
```

The response object contains these fields:

| Field    | Type    | Description                       |
| -------- | ------- | --------------------------------- |
| `status` | Integer | HTTP status code (200, 404, etc.) |
| `ok`     | Boolean | `true` if status is 2xx           |
| `body`   | String  | Raw response body                 |
| `json`   | Object  | Parsed JSON (if applicable)       |

For POST requests, pass an options object as the second argument:

```forge
let resp = fetch("https://httpbin.org/post", {
    method: "POST",
    body: { name: "Forge", type: "language" }
})
let status = resp.status
say "Status: {status}"
```

### The http Module

The `http` module provides named methods for each HTTP verb, offering a cleaner syntax
when you don't need the full flexibility of `fetch()`:

```forge
let resp = http.get("https://api.github.com/repos/rust-lang/rust")
let resp = http.post("https://httpbin.org/post", { key: "value" })
let resp = http.put("https://httpbin.org/put", { updated: true })
let resp = http.delete("https://httpbin.org/delete")
```

Each returns the same response object structure as `fetch()`.

### Working with API Responses

API responses typically contain JSON. Access nested fields with dot notation:

```forge
let resp = fetch("https://api.github.com/repos/rust-lang/rust")
let name = resp.json.full_name
let stars = resp.json.stargazers_count
say "Repo: {name}"
say "Stars: {stars}"
```

For APIs that return arrays, iterate with `for`:

```forge
let resp = fetch("https://api.github.com/users/torvalds/repos?per_page=5")
for repo in resp.json {
    let name = repo.name
    say "  - {name}"
}
```

### download and crawl

Forge provides two high-level keywords for common web tasks.

**download** saves a remote file to disk:

```forge
download "https://example.com/data.csv" to "local_data.csv"
```

Or using the `http` module:

```forge
http.download("https://example.com/data.csv", "local_data.csv")
```

**crawl** fetches a web page and returns its HTML content as a string, suitable for
parsing and extraction:

```forge
let html = crawl "https://example.com"
say html
```

### Project 1: API Consumer — GitHub Repository Dashboard

This program fetches a user's GitHub repositories and displays them as a formatted
terminal table with color-coded statistics.

```forge
// github_repos.fg — GitHub repository dashboard
// Run: forge run github_repos.fg
// Note: Uses the public GitHub API (no token needed for public repos)

say term.banner("GitHub Repository Dashboard")
say ""

let username = "torvalds"
let url = "https://api.github.com/users/{username}/repos?per_page=10&sort=updated"
say term.blue("Fetching repos for @{username}...")
say ""

let resp = fetch(url)

if resp.ok == false {
    let status = resp.status
    say term.error("Failed to fetch: HTTP {status}")
} else {
    let repos = resp.json

    // Build table data
    let mut rows = []
    for repo in repos {
        let name = repo.name
        let stars = repo.stargazers_count
        let forks = repo.forks_count
        let lang = repo.language
        if lang == null {
            lang = "N/A"
        }
        let row = { Name: name, Language: lang, Stars: stars, Forks: forks }
        rows = append(rows, row)
    }

    term.table(rows)
    say ""

    // Summary statistics
    let star_counts = map(repos, fn(r) { return r.stargazers_count })
    let total_stars = reduce(star_counts, 0, fn(acc, x) { return acc + x })
    let repo_count = len(repos)
    say term.bold("Summary:")
    say "  Repositories shown: {repo_count}"
    say "  Total stars: {total_stars}"
    say ""

    // Star distribution sparkline
    say term.blue("Star distribution:")
    term.sparkline(star_counts)
    say ""

    // Bar chart of top repos by stars
    say term.blue("Top repos by stars:")
    let sorted_repos = sort(repos, fn(a, b) { return b.stargazers_count - a.stargazers_count })
    let mut i = 0
    let limit = math.min(5, len(sorted_repos))
    while i < limit {
        let repo = sorted_repos[i]
        let rname = repo.name
        let rstars = repo.stargazers_count
        term.bar(rname, float(rstars), float(total_stars))
        i = i + 1
    }
}

say ""
term.success("Dashboard complete!")
```

**Expected output:**

```
╔══════════════════════════════════════╗
║    GitHub Repository Dashboard       ║
╚══════════════════════════════════════╝

Fetching repos for @torvalds...

┌──────────────┬──────────┬───────┬───────┐
│ Name         │ Language │ Stars │ Forks │
├──────────────┼──────────┼───────┼───────┤
│ linux        │ C        │ 18000 │ 52000 │
│ subsurface   │ C++      │  2500 │   980 │
│ ...          │ ...      │   ... │   ... │
└──────────────┴──────────┴───────┴───────┘

Summary:
  Repositories shown: 10
  Total stars: 21342

Star distribution:
▁▁▁▁▁▁▁█▁▁

✓ Dashboard complete!
```

### Project 2: Health Monitor — Multi-URL Status Checker

This tool checks the availability of multiple services and generates a color-coded
status report, useful for monitoring dashboards or on-call scripts.

```forge
// health_monitor.fg — Service health checker
// Run: forge run health_monitor.fg

say term.banner("Service Health Monitor")
say ""

let services = [
    { name: "GitHub",      url: "https://api.github.com" },
    { name: "Google",      url: "https://www.google.com" },
    { name: "HTTPBin",     url: "https://httpbin.org/get" },
    { name: "Example.com", url: "https://example.com" },
    { name: "BadURL",      url: "https://this-does-not-exist.invalid" }
]

let mut results = []
let mut up_count = 0
let mut down_count = 0

for service in services {
    let name = service.name
    let url = service.url
    say "  Checking {name}..."
    let resp = fetch(url)
    let status = resp.status
    if resp.ok {
        let entry = { Service: name, Status: status, Result: "UP" }
        results = append(results, entry)
        up_count = up_count + 1
    } else {
        let entry = { Service: name, Status: status, Result: "DOWN" }
        results = append(results, entry)
        down_count = down_count + 1
    }
}

say ""
say term.bold("Results:")
term.table(results)
say ""

say term.green("  Up:   {up_count}")
say term.red("  Down: {down_count}")
say ""

let total = len(services)
if down_count == 0 {
    term.success("All {total} services are healthy!")
} else {
    term.warning("{down_count} of {total} services are down.")
}
```

**Expected output:**

```
╔══════════════════════════════════════╗
║      Service Health Monitor          ║
╚══════════════════════════════════════╝

  Checking GitHub...
  Checking Google...
  Checking HTTPBin...
  Checking Example.com...
  Checking BadURL...

Results:
┌─────────────┬────────┬────────┐
│ Service     │ Status │ Result │
├─────────────┼────────┼────────┤
│ GitHub      │ 200    │ UP     │
│ Google      │ 200    │ UP     │
│ HTTPBin     │ 200    │ UP     │
│ Example.com │ 200    │ UP     │
│ BadURL      │ 0      │ DOWN   │
└─────────────┴────────┴────────┘

  Up:   4
  Down: 1

⚠ 1 of 5 services are down.
```

### Project 3: Web Scraper — Crawl and Extract

This project uses Forge's `crawl` keyword to fetch web pages and extract useful
information using string processing.

```forge
// scraper.fg — Simple web scraper
// Run: forge run scraper.fg

say term.banner("Forge Web Scraper")
say ""

let url = "https://example.com"
say "Crawling {url}..."
let html = crawl url
say ""

// Extract the page title
let title_start = "title>"
let title_end = "</title"
let parts = split(html, title_start)
if len(parts) > 1 {
    let after_tag = parts[1]
    let title_parts = split(after_tag, title_end)
    let title = title_parts[0]
    say term.bold("Page Title:")
    say "  {title}"
    say ""
}

// Count occurrences of key HTML elements
let html_lower = lowercase(html)
let p_tags = split(html_lower, "<p")
let a_tags = split(html_lower, "<a ")
let div_tags = split(html_lower, "<div")
let p_count = len(p_tags) - 1
let a_count = len(a_tags) - 1
let div_count = len(div_tags) - 1

say term.bold("Element Counts:")
say "  <p>   tags: {p_count}"
say "  <a>   tags: {a_count}"
say "  <div> tags: {div_count}"
say ""

// Page size stats
let page_size = len(html)
say term.bold("Page Statistics:")
say "  Total HTML size: {page_size} characters"
let line_list = split(html, "\n")
let line_count = len(line_list)
say "  Line count: {line_count}"
say ""

// Save to file
let filename = "scraped_output.html"
fs.write(filename, html)
say term.green("Saved HTML to {filename}")
say ""

term.success("Scraping complete!")
```

### Going Further

- **Authenticated requests.** Pass headers to `fetch()` for APIs that require
  Bearer tokens or API keys.
- **Pagination.** Use a `while` loop to follow `next` links in paginated API responses.
- **Retry logic.** Wrap `fetch()` calls in Forge's `retry 3 times { }` block for
  resilient HTTP clients.
- **Parallel fetching.** Use `forge` (async) functions with `hold` (await) to fetch
  multiple URLs concurrently.

---

## Chapter 22: Data Processing Pipelines

Data processing is the bread and butter of practical programming. You receive data in
one format, transform it, analyze it, and present the results. Forge excels at this
workflow because it combines first-class JSON, a built-in SQLite database, CSV
parsing, functional data transformations, and terminal visualization into a single,
cohesive toolkit. No imports. No dependencies. Just data in, insight out.

### The CSV → Database → Analysis → Visualization Pattern

The canonical Forge data pipeline follows four stages:

```
  ┌──────────┐     ┌──────────┐     ┌───────────┐     ┌──────────────┐
  │  Ingest  │────►│  Store   │────►│  Analyze  │────►│  Visualize   │
  │          │     │          │     │           │     │              │
  │ CSV file │     │ SQLite   │     │ SQL agg.  │     │ term.table() │
  │ JSON API │     │ :memory: │     │ map/filter│     │ term.bar()   │
  │ log file │     │          │     │ reduce    │     │ sparkline    │
  └──────────┘     └──────────┘     └───────────┘     └──────────────┘
```

Each stage uses built-in Forge primitives. No third-party libraries, no setup.

### Functional Data Transformation Chains

Forge's `map`, `filter`, and `reduce` functions form the backbone of data
transformations. They compose naturally:

```forge
let data = [10, 25, 3, 47, 8, 31, 15]

// Filter → Map → Reduce pipeline
let big = filter(data, fn(x) { return x > 10 })
let doubled = map(big, fn(x) { return x * 2 })
let total = reduce(doubled, 0, fn(acc, x) { return acc + x })

say "Values > 10, doubled, summed: {total}"
```

For database query results (arrays of objects), you can extract and transform specific
fields:

```forge
let rows = db.query("SELECT product, price FROM items")
let prices = map(rows, fn(r) { return float(r.price) })
let avg = reduce(prices, 0.0, fn(a, x) { return a + x }) / len(prices)
```

### Project 1: Sales Analytics — CSV Import, SQL Aggregation, Terminal Charts

This complete program imports sales data, loads it into an in-memory SQLite database,
runs aggregation queries, and produces a full terminal dashboard with tables, bar charts,
and sparklines.

```forge
// sales_analytics.fg — Full sales data pipeline
// Run: forge run sales_analytics.fg
// Note: Creates sample data if sales.csv doesn't exist

say term.banner("Sales Analytics Dashboard")
say ""

// Step 1: Generate sample data if needed
let csv_file = "sales.csv"
if fs.exists(csv_file) == false {
    say term.blue("Generating sample sales data...")
    let sample_csv = "date,product,quantity,unit_price,region
2026-01-05,Widget,10,29.99,North
2026-01-08,Gadget,5,49.99,South
2026-01-12,Widget,8,29.99,East
2026-01-15,Gizmo,3,99.99,North
2026-01-18,Gadget,12,49.99,West
2026-01-22,Widget,15,29.99,South
2026-01-25,Gizmo,7,99.99,East
2026-02-01,Gadget,9,49.99,North
2026-02-05,Widget,20,29.99,West
2026-02-08,Gizmo,4,99.99,South
2026-02-12,Gadget,6,49.99,East
2026-02-15,Widget,11,29.99,North
2026-02-18,Gizmo,8,99.99,West
2026-02-22,Widget,14,29.99,South
2026-02-25,Gadget,10,49.99,North"
    fs.write(csv_file, sample_csv)
    say "  Created {csv_file} with 15 records"
    say ""
}

// Step 2: Read CSV and load into database
say term.blue("Loading data into database...")
let raw = fs.read(csv_file)
let records = csv.parse(raw)
say "  Parsed {len(records)} records from CSV"

db.open(":memory:")
db.execute("CREATE TABLE sales (id INTEGER PRIMARY KEY AUTOINCREMENT, date TEXT, product TEXT, quantity INTEGER, unit_price REAL, region TEXT)")

for row in records {
    let d = row.date
    let p = row.product
    let q = row.quantity
    let u = row.unit_price
    let r = row.region
    let stmt = "INSERT INTO sales (date, product, quantity, unit_price, region) VALUES ('{d}', '{p}', {q}, {u}, '{r}')"
    db.execute(stmt)
}
say "  Loaded into SQLite"
say ""

// Step 3: Analysis
say term.bold("═══ Sales by Product ═══")
let by_product = db.query("SELECT product, SUM(quantity) as total_qty, ROUND(SUM(quantity * unit_price), 2) as revenue FROM sales GROUP BY product ORDER BY revenue DESC")
term.table(by_product)
say ""

say term.bold("═══ Sales by Region ═══")
let by_region = db.query("SELECT region, COUNT(*) as transactions, SUM(quantity) as total_qty, ROUND(SUM(quantity * unit_price), 2) as revenue FROM sales GROUP BY region ORDER BY revenue DESC")
term.table(by_region)
say ""

say term.bold("═══ Monthly Trend ═══")
let by_month = db.query("SELECT substr(date, 1, 7) as month, SUM(quantity) as units, ROUND(SUM(quantity * unit_price), 2) as revenue FROM sales GROUP BY month ORDER BY month")
term.table(by_month)
say ""

// Step 4: Visualize
say term.bold("═══ Revenue by Product (Bar Chart) ═══")
let revenues = map(by_product, fn(r) { return float(r.revenue) })
let max_revenue = reduce(revenues, 0.0, fn(a, x) { if x > a { return x } else { return a } })
for row in by_product {
    let pname = row.product
    let rev = float(row.revenue)
    term.bar(pname, rev, max_revenue)
}
say ""

say term.bold("═══ Revenue by Region (Bar Chart) ═══")
let region_revs = map(by_region, fn(r) { return float(r.revenue) })
let max_region = reduce(region_revs, 0.0, fn(a, x) { if x > a { return x } else { return a } })
for row in by_region {
    let rname = row.region
    let rev = float(row.revenue)
    term.bar(rname, rev, max_region)
}
say ""

// Step 5: Summary statistics
let all_sales = db.query("SELECT quantity * unit_price as total FROM sales")
let all_amounts = map(all_sales, fn(r) { return float(r.total) })
let grand_total = reduce(all_amounts, 0.0, fn(a, x) { return a + x })
let sale_count = len(all_amounts)
let avg_sale = grand_total / sale_count

say term.bold("═══ Summary ═══")
say "  Total revenue:      ${grand_total}"
say "  Total transactions: {sale_count}"
say "  Average sale value: ${avg_sale}"
say ""

// Step 6: Revenue trend sparkline
say term.bold("═══ Transaction Values ═══")
term.sparkline(all_amounts)
say ""

db.close()
term.success("Analysis complete!")
```

**Expected output:**

```
╔══════════════════════════════════════╗
║     Sales Analytics Dashboard        ║
╚══════════════════════════════════════╝

Loading data into database...
  Parsed 15 records from CSV
  Loaded into SQLite

═══ Sales by Product ═══
┌─────────┬───────────┬──────────┐
│ product │ total_qty │ revenue  │
├─────────┼───────────┼──────────┤
│ Gizmo   │ 22        │ 2199.78  │
│ Gadget  │ 42        │ 2099.58  │
│ Widget  │ 78        │ 2339.22  │
└─────────┴───────────┴──────────┘

═══ Revenue by Product (Bar Chart) ═══
Widget  ████████████████████████████████ $2339.22
Gizmo   █████████████████████████████   $2199.78
Gadget  ████████████████████████████    $2099.58

═══ Summary ═══
  Total revenue:      $6638.58
  Total transactions: 15
  Average sale value: $442.572

✓ Analysis complete!
```

### Project 2: Log Analyzer — Parse, Extract, Report

This program reads a log file, parses entries by severity level, and generates a
summary report. It demonstrates string processing, pattern counting, and structured
output.

```forge
// log_analyzer.fg — Parse log files and generate reports
// Run: forge run log_analyzer.fg

say term.banner("Log File Analyzer")
say ""

// Generate a sample log file
let log_file = "app.log"
if fs.exists(log_file) == false {
    say term.blue("Generating sample log file...")
    let log_content = "2026-02-28 08:00:01 INFO  Server started on port 3000
2026-02-28 08:00:02 INFO  Database connection established
2026-02-28 08:01:15 INFO  GET /api/users 200 45ms
2026-02-28 08:01:16 DEBUG Query executed: SELECT * FROM users
2026-02-28 08:02:30 WARN  Slow query detected: 850ms
2026-02-28 08:03:00 INFO  POST /api/users 201 120ms
2026-02-28 08:03:45 ERROR Failed to send email: connection timeout
2026-02-28 08:04:10 INFO  GET /api/users/5 200 30ms
2026-02-28 08:05:00 WARN  Rate limit approaching for IP 192.168.1.100
2026-02-28 08:05:30 INFO  PUT /api/users/5 200 65ms
2026-02-28 08:06:00 DEBUG Cache miss for key: user:5
2026-02-28 08:06:15 ERROR Database connection lost: retry in 5s
2026-02-28 08:06:20 INFO  Database reconnected
2026-02-28 08:07:00 INFO  GET /api/health 200 5ms
2026-02-28 08:08:00 WARN  Memory usage at 85%
2026-02-28 08:09:00 INFO  DELETE /api/users/3 200 40ms
2026-02-28 08:10:00 ERROR Unhandled exception in /api/reports
2026-02-28 08:10:01 INFO  Error recovery complete
2026-02-28 08:11:00 INFO  GET /api/users 200 50ms
2026-02-28 08:12:00 DEBUG GC pause: 12ms"
    fs.write(log_file, log_content)
    say "  Created {log_file}"
    say ""
}

// Read and parse the log file
let content = fs.read(log_file)
let lines = split(content, "\n")
let total_lines = len(lines)

say term.blue("Parsing {total_lines} log entries...")
say ""

// Count by severity level
let mut info_count = 0
let mut warn_count = 0
let mut error_count = 0
let mut debug_count = 0
let mut errors = []
let mut warnings = []

for line in lines {
    if len(line) == 0 {
        // skip empty lines
    } else if contains(line, "ERROR") {
        error_count = error_count + 1
        errors = append(errors, line)
    } else if contains(line, "WARN") {
        warn_count = warn_count + 1
        warnings = append(warnings, line)
    } else if contains(line, "DEBUG") {
        debug_count = debug_count + 1
    } else if contains(line, "INFO") {
        info_count = info_count + 1
    }
}

// Display severity breakdown
say term.bold("═══ Severity Breakdown ═══")
let severity_data = [
    { Level: "INFO",  Count: info_count },
    { Level: "WARN",  Count: warn_count },
    { Level: "ERROR", Count: error_count },
    { Level: "DEBUG", Count: debug_count }
]
term.table(severity_data)
say ""

// Bar chart
say term.bold("═══ Log Level Distribution ═══")
let max_count = float(math.max(math.max(info_count, warn_count), math.max(error_count, debug_count)))
term.bar("INFO ", float(info_count), max_count)
term.bar("WARN ", float(warn_count), max_count)
term.bar("ERROR", float(error_count), max_count)
term.bar("DEBUG", float(debug_count), max_count)
say ""

// Show errors and warnings
if error_count > 0 {
    say term.red(term.bold("═══ Errors ═══"))
    for err in errors {
        say term.red("  {err}")
    }
    say ""
}

if warn_count > 0 {
    say term.yellow(term.bold("═══ Warnings ═══"))
    for w in warnings {
        say term.yellow("  {w}")
    }
    say ""
}

// HTTP endpoint analysis
say term.bold("═══ HTTP Requests ═══")
let mut get_count = 0
let mut post_count = 0
let mut put_count = 0
let mut delete_count = 0

for line in lines {
    if contains(line, "GET /") {
        get_count = get_count + 1
    } else if contains(line, "POST /") {
        post_count = post_count + 1
    } else if contains(line, "PUT /") {
        put_count = put_count + 1
    } else if contains(line, "DELETE /") {
        delete_count = delete_count + 1
    }
}

let http_data = [
    { Method: "GET",    Requests: get_count },
    { Method: "POST",   Requests: post_count },
    { Method: "PUT",    Requests: put_count },
    { Method: "DELETE", Requests: delete_count }
]
term.table(http_data)
say ""

// Summary
say term.bold("═══ Summary ═══")
say "  Total entries:  {total_lines}"
say "  Error rate:     {error_count}/{total_lines}"
let total_http = get_count + post_count + put_count + delete_count
say "  HTTP requests:  {total_http}"
say ""

if error_count > 0 {
    term.warning("Log analysis found {error_count} error(s). Review above.")
} else {
    term.success("No errors found in log.")
}
```

### Project 3: Data Converter — JSON to CSV to Database Roundtrip

This project demonstrates converting data between formats—a common task in data
engineering. It takes JSON data, writes it to CSV, reads the CSV back, loads it into
a database, queries it, and exports the results as JSON.

```forge
// converter.fg — Format conversion pipeline: JSON → CSV → SQLite → JSON
// Run: forge run converter.fg

say term.banner("Data Format Converter")
say ""

// Step 1: Start with JSON data
say term.blue("Step 1: Create JSON dataset")
let employees = [
    { name: "Alice Chen",    department: "Engineering", salary: 125000, years: 5 },
    { name: "Bob Martinez",  department: "Marketing",   salary: 95000,  years: 3 },
    { name: "Carol Kim",     department: "Engineering", salary: 135000, years: 7 },
    { name: "David Johnson", department: "Sales",       salary: 88000,  years: 2 },
    { name: "Eva Schmidt",   department: "Engineering", salary: 142000, years: 9 },
    { name: "Frank Brown",   department: "Marketing",   salary: 102000, years: 4 },
    { name: "Grace Liu",     department: "Sales",       salary: 91000,  years: 3 },
    { name: "Henry Wilson",  department: "Engineering", salary: 118000, years: 4 }
]
let emp_count = len(employees)
say "  Created {emp_count} employee records"
say ""

// Step 2: Export to CSV
say term.blue("Step 2: Export to CSV")
let csv_file = "employees.csv"
csv.write(csv_file, employees)
say "  Written to {csv_file}"
let file_size = fs.size(csv_file)
say "  File size: {file_size} bytes"
say ""

// Step 3: Read CSV back
say term.blue("Step 3: Read CSV back")
let csv_content = fs.read(csv_file)
let parsed = csv.parse(csv_content)
say "  Parsed {len(parsed)} records from CSV"
say ""

// Step 4: Load into SQLite
say term.blue("Step 4: Load into SQLite")
db.open(":memory:")
db.execute("CREATE TABLE employees (name TEXT, department TEXT, salary REAL, years INTEGER)")
for emp in parsed {
    let n = emp.name
    let d = emp.department
    let s = emp.salary
    let y = emp.years
    db.execute("INSERT INTO employees VALUES ('{n}', '{d}', {s}, {y})")
}
say "  Loaded into in-memory database"
say ""

// Step 5: Run analytics queries
say term.bold("═══ All Employees ═══")
let all = db.query("SELECT * FROM employees ORDER BY salary DESC")
term.table(all)
say ""

say term.bold("═══ Department Summary ═══")
let dept_summary = db.query("SELECT department, COUNT(*) as headcount, ROUND(AVG(salary), 0) as avg_salary, MAX(salary) as max_salary FROM employees GROUP BY department ORDER BY avg_salary DESC")
term.table(dept_summary)
say ""

say term.bold("═══ Salary Distribution ═══")
let salaries = map(all, fn(r) { return float(r.salary) })
let max_salary = reduce(salaries, 0.0, fn(a, x) { if x > a { return x } else { return a } })
for row in all {
    let ename = row.name
    let esal = float(row.salary)
    term.bar(ename, esal, max_salary)
}
say ""

// Step 6: Export results as JSON
say term.blue("Step 6: Export analysis as JSON")
let report = {
    generated: sh("date -u +%Y-%m-%dT%H:%M:%SZ"),
    total_employees: len(all),
    departments: dept_summary,
    employees: all
}
let report_json = json.pretty(report)
fs.write("employee_report.json", report_json)
say "  Written to employee_report.json"
say ""

db.close()

// Clean up
fs.remove(csv_file)
say term.green("  Cleaned up temporary CSV")
say ""

term.success("Conversion pipeline complete!")
```

**Data flow:**

```
  ┌──────────┐     ┌──────────┐     ┌───────────┐     ┌──────────┐
  │  Forge   │────►│   CSV    │────►│  SQLite   │────►│   JSON   │
  │  Objects │     │   File   │     │  :memory: │     │  Report  │
  │ (in-mem) │     │ .csv     │     │  queries  │     │  .json   │
  └──────────┘     └──────────┘     └───────────┘     └──────────┘
       │                                                    │
       └──────── Full roundtrip — data integrity ───────────┘
```

### Going Further

- **Streaming large files.** For very large CSV files, process them in chunks by reading
  line-by-line rather than loading the entire file into memory.
- **Scheduled pipelines.** Combine data processing with Forge's `schedule every 1 hour`
  block to run pipelines on a recurring basis.
- **Multi-source joins.** Load data from multiple CSV files into separate database tables
  and use SQL JOINs for cross-source analysis.
- **Export formats.** Generate HTML reports by building template strings, or pipe
  JSON output to downstream services via `http.post()`.

---

## Chapter 23: DevOps and System Automation

System administrators and DevOps engineers spend their days automating repetitive tasks:
checking system health, deploying applications, rotating backups, validating
configurations. Traditional tools for this—Bash scripts, Python scripts, Ansible
playbooks—each have tradeoffs. Bash is powerful but cryptic. Python requires virtual
environments. Ansible requires YAML fluency.

Forge occupies a sweet spot: it has shell integration as a first-class primitive, a
real programming language's control flow and data structures, and built-in file system,
JSON, and terminal formatting—all in a single binary with zero dependencies.

### The Complete Shell Toolkit

Forge provides 11 shell-related functions that cover every common DevOps task:

| Function             | Returns     | Description                                   |
| -------------------- | ----------- | --------------------------------------------- |
| `sh(cmd)`            | String      | Run command, return trimmed stdout            |
| `shell(cmd)`         | Object      | Full result: stdout, stderr, status, ok       |
| `sh_lines(cmd)`      | Array       | Run command, return output as array of lines  |
| `sh_json(cmd)`       | Any         | Run command, parse stdout as JSON             |
| `sh_ok(cmd)`         | Boolean     | Run command, return true if exit code is 0    |
| `which(name)`        | String/null | Find executable path (e.g. `which("docker")`) |
| `cwd()`              | String      | Current working directory                     |
| `cd(path)`           | String      | Change directory, return path                 |
| `lines(s)`           | Array       | Split string into array of lines              |
| `pipe_to(data, cmd)` | Object      | Pipe string into shell command (stdin)        |
| `run_command(cmd)`   | Object      | Run command (split by whitespace, no shell)   |

Use `sh()` and `shell()` for basic execution. Use `sh_lines()` to parse process
listings or command output line-by-line. Use `sh_json()` when a command emits JSON
(e.g. `docker inspect`, `kubectl get -o json`). Use `sh_ok()` for quick success checks.
Use `which()` to verify required tools exist before running. Use `cwd()` and `cd()` for
directory navigation. Use `lines()` to process multi-line strings. Use `pipe_to()` to
filter or transform data through shell commands.

### Shell Integration: shell() and sh()

Forge provides two functions for running shell commands:

**`shell(cmd)`** returns a full result object:

```forge
let result = shell("ls -la /tmp")
say result.stdout
say result.stderr
say result.status
say result.ok
```

| Field    | Type    | Description              |
| -------- | ------- | ------------------------ |
| `stdout` | String  | Standard output          |
| `stderr` | String  | Standard error           |
| `status` | Integer | Exit code (0 = success)  |
| `ok`     | Boolean | `true` if exit code is 0 |

**`sh(cmd)`** returns just the stdout string, trimmed. It's a convenience wrapper
for the common case where you just want the output:

```forge
let hostname = sh("hostname")
let date = sh("date")
say "Host: {hostname}, Date: {date}"
```

Use `shell()` when you need to check for errors or capture stderr. Use `sh()` when you
just want the output of a command that you trust to succeed.

### Environment Management

The `env` module reads and writes environment variables:

```forge
let home = env.get("HOME")
let path = env.get("PATH")
let has_key = env.has("API_KEY")
env.set("MY_VAR", "my_value")
```

This is essential for deployment scripts that read configuration from the environment,
following the twelve-factor app methodology.

### File System Operations for Automation

The `fs` module covers everything an automation script needs:

```forge
// Read and write
fs.write("/tmp/config.json", json.stringify(config))
let content = fs.read("/tmp/config.json")

// Check existence and get metadata
if fs.exists("/tmp/config.json") {
    let size = fs.size("/tmp/config.json")
    say "Config is {size} bytes"
}

// Directory operations
fs.mkdir("/tmp/backups")
let files = fs.list("/tmp/backups")

// Clean up
fs.remove("/tmp/old_file.txt")
```

### Object Helpers for Configuration Management

DevOps scripts often work with nested config objects, environment overrides, and API
responses. Forge's object helpers make this safe and readable:

- **`merge(defaults, overrides)`** — Merge config objects. Later values win. Ideal for
  combining defaults with environment-specific overrides:

```forge
let defaults = { port: 3000, host: "0.0.0.0", log_level: "info" }
let overrides = { port: 8080, log_level: "debug" }
let config = merge(defaults, overrides)
say config.port
```

- **`pick(obj, ["field1", "field2"])`** — Extract only the fields you need. Use when
  stripping sensitive or irrelevant fields from API responses:

```forge
let raw = sh_json("docker inspect mycontainer")
let relevant = pick(raw[0], ["Id", "State", "Mounts"])
```

- **`get(obj, "dot.path", default)`** — Safe nested access. Never crashes on missing
  keys. Use for optional config or nested API responses:

```forge
let resp = fetch("https://api.example.com/status")
let status = get(resp.json, "data.health", "unknown")
let retries = get(config, "retry.count", 3)
```

- **`has_key(obj, "key")`** — Check if a key exists before accessing. Use when config
  fields are optional:

```forge
if has_key(config, "ssl_cert") {
    say "SSL configured: {config.ssl_cert}"
}
```

### Project 1: System Health Checker — Full Diagnostic Script

This comprehensive diagnostic tool checks CPU, memory, disk, network, and process
information, presenting everything in a clean terminal dashboard.

```forge
// system_health.fg — Comprehensive system health checker
// Run: forge run system_health.fg

say term.banner("System Health Report")
say ""

let timestamp = sh("date")
say term.blue("Report generated: {timestamp}")
say ""

// ─── System Information ─────────────────────────────────────────
say term.bold("═══ System Information ═══")
let user = sh("whoami")
let host = sh("hostname")
let os_name = sh("uname -s")
let arch = sh("uname -m")
let kernel = sh("uname -r")

let sys_info = [
    { Property: "User",     Value: user },
    { Property: "Hostname", Value: host },
    { Property: "OS",       Value: os_name },
    { Property: "Kernel",   Value: kernel },
    { Property: "Arch",     Value: arch }
]
term.table(sys_info)
say ""

// ─── Uptime ──────────────────────────────────────────────────────
say term.bold("═══ Uptime ═══")
let uptime = sh("uptime")
say "  {uptime}"
say ""

// ─── Disk Usage ──────────────────────────────────────────────────
say term.bold("═══ Disk Usage ═══")
let disk_result = shell("df -h / /tmp 2>/dev/null")
if disk_result.ok {
    let disk_lines = split(disk_result.stdout, "\n")
    for line in disk_lines {
        if len(line) > 0 {
            say "  {line}"
        }
    }
}
say ""

// ─── Memory ──────────────────────────────────────────────────────
say term.bold("═══ Memory ═══")
let mem_result = shell("vm_stat 2>/dev/null || free -h 2>/dev/null")
if mem_result.ok {
    let mem_lines = split(mem_result.stdout, "\n")
    let mut shown = 0
    for line in mem_lines {
        if shown < 5 {
            if len(line) > 0 {
                say "  {line}"
                shown = shown + 1
            }
        }
    }
}
say ""

// ─── Environment ─────────────────────────────────────────────────
say term.bold("═══ Environment ═══")
let home = env.get("HOME")
let user_shell = env.get("SHELL")
let path_val = env.get("PATH")
let path_sep = ":"
let path_entries = split(path_val, path_sep)
let path_count = len(path_entries)
let env_info = [
    { Variable: "HOME",         Value: home },
    { Variable: "SHELL",        Value: user_shell },
    { Variable: "PATH entries", Value: path_count }
]
term.table(env_info)
say ""

// ─── Key Processes ───────────────────────────────────────────────
say term.bold("═══ Active Processes ═══")
let proc_count = sh("ps aux | wc -l")
say "  Total processes: {proc_count}"
say ""

// ─── Network Check ───────────────────────────────────────────────
say term.bold("═══ Network Connectivity ═══")
let targets = [
    { name: "Google DNS",    host: "8.8.8.8" },
    { name: "Cloudflare",    host: "1.1.1.1" }
]

let mut net_results = []
for target in targets {
    let tname = target.name
    let thost = target.host
    let ping = shell("ping -c 1 -W 2 {thost} 2>/dev/null")
    if ping.ok {
        let entry = { Target: tname, Host: thost, Status: "Reachable" }
        net_results = append(net_results, entry)
    } else {
        let entry = { Target: tname, Host: thost, Status: "Unreachable" }
        net_results = append(net_results, entry)
    }
}
term.table(net_results)
say ""

// ─── File System Check ──────────────────────────────────────────
say term.bold("═══ File System Test ═══")
let test_file = "/tmp/forge_health_check.txt"
let test_content = "Health check at {timestamp}"
fs.write(test_file, test_content)
let readback = fs.read(test_file)
let fsize = fs.size(test_file)
let write_ok = readback == test_content
fs.remove(test_file)

say "  Write test:  {write_ok}"
say "  File size:   {fsize} bytes"
say "  Cleanup:     done"
say ""

// ─── Security ────────────────────────────────────────────────────
say term.bold("═══ Security Hashes ═══")
let check_hash = crypto.sha256("system-health-{host}-{timestamp}")
say "  Report hash: {check_hash}"
say ""

// ─── Final Verdict ───────────────────────────────────────────────
term.success("Health check complete — all systems nominal")
```

### Project: Infrastructure Monitor

This project uses the complete shell toolkit in a realistic DevOps script. It checks
required tools, verifies services, parses process listings and JSON, filters log data,
and navigates directories—demonstrating all 11 shell functions plus object helpers.

```forge
// infrastructure_monitor.fg — DevOps script using the complete shell toolkit
// Run: forge run infrastructure_monitor.fg

say term.banner("Infrastructure Monitor")
say ""

// 1. which() — Check required tools
say term.bold("═══ Tool Check ═══")
let tools = ["docker", "git", "curl"]
let mut missing = []
for tool in tools {
    let path = which(tool)
    if path {
        say term.green("  {tool}: {path}")
    } else {
        missing = append(missing, tool)
        say term.red("  {tool}: NOT FOUND")
    }
}
say ""

// 2. sh_ok() — Check if services are running
say term.bold("═══ Service Status ═══")
if sh_ok("pgrep -q -x node") {
    say term.green("  Node: running")
} else {
    say term.yellow("  Node: not running")
}
if sh_ok("pgrep -q -x postgres") {
    say term.green("  Postgres: running")
} else {
    say term.yellow("  Postgres: not running")
}
say ""

// 3. cwd() and cd() — Directory navigation
say term.bold("═══ Directory ═══")
let start_dir = cwd()
say "  Started in: {start_dir}"
cd("/tmp")
let tmp_dir = cwd()
say "  Switched to: {tmp_dir}"
cd(start_dir)
say "  Restored: {cwd()}"
say ""

// 4. sh_lines() — Parse process listings
say term.bold("═══ Top Processes ═══")
let proc_lines = sh_lines("ps aux | head -6")
let proc_count = len(proc_lines)
say "  Showing {proc_count} process lines"
for line in proc_lines {
    if len(line) > 0 {
        say "    {line}"
    }
}
say ""

// 5. sh_json() — Parse JSON from system commands
say term.bold("═══ JSON from Command ═══")
let ob = "{"
let cb = "}"
let json_cmd = "echo '" + ob + "\"version\": \"1.0\", \"services\": [\"api\", \"worker\"]" + cb + "'"
let json_data = sh_json(json_cmd)
if has_key(json_data, "version") {
    let ver = json_data.version
    say "  Config version: {ver}"
}
let svc_list = get(json_data, "services", [])
let svc_count = len(svc_list)
say "  Services defined: {svc_count}"
say ""

// 6. pipe_to() — Filter log data
say term.bold("═══ Log Filter ═══")
let sample_log = "ERROR db timeout\nINFO request OK\nWARN slow query\nERROR disk full\nINFO shutdown"
let filtered = pipe_to(sample_log, "grep ERROR")
let filtered_lines = lines(filtered.stdout)
say "  Errors in sample log: {len(filtered_lines)}"
for err_line in filtered_lines {
    say "    {err_line}"
}
say ""

// 7. lines() — Process multi-line strings
say term.bold("═══ Multi-line Parse ═══")
let hosts_block = "127.0.0.1 localhost\n::1 ip6-localhost"
let host_lines = lines(hosts_block)
say "  Host entries: {len(host_lines)}"
say ""

// 8. merge(), pick(), get() — Config management
say term.bold("═══ Config Merge ═══")
let defaults = { port: 3000, debug: false }
let env_overrides = { port: 8080, debug: true }
let config = merge(defaults, env_overrides)
let deploy_config = pick(config, ["port", "debug"])
say "  Merged: {deploy_config}"
let workers = get(config, "workers", 4)
say "  Workers (default): {workers}"
say ""

// 9. run_command() — Safe argv-style execution (no shell)
say term.bold("═══ run_command ═══")
let echo_result = run_command("echo hello from forge")
say "  Output: {echo_result.stdout}"
say ""

term.success("Infrastructure monitor complete!")
```

**Walkthrough.** The script uses `which()` to verify docker, git, and curl are
installed. It uses `sh_ok()` to check if Node and Postgres processes are running.
It uses `cwd()` and `cd()` to save, change, and restore the working directory. It
uses `sh_lines()` to capture and display process output line-by-line. It uses
`sh_json()` to parse JSON emitted by a command. It uses `pipe_to()` to pipe a
multi-line log string into `grep ERROR` and then `lines()` to process the filtered
output. It demonstrates `merge()`, `pick()`, `get()`, and `has_key()` for config
management. Finally, it uses `run_command()` for safe command execution without a shell.

### Project 2: Deploy Script — Config, Validation, Execution

This deployment automation script reads a JSON configuration file, validates it,
runs pre-deployment checks, and executes the deployment steps.

```forge
// deploy.fg — Deployment automation script
// Run: forge run deploy.fg

say term.banner("Forge Deploy")
say ""

// Step 1: Load or create deploy configuration
let config_file = "deploy.json"
if fs.exists(config_file) == false {
    say term.blue("Creating default deploy config...")
    let default_config = {
        app_name: "myservice",
        version: "1.2.0",
        environment: "staging",
        port: 8080,
        health_check: "/health",
        build_cmd: "echo 'Building...'",
        test_cmd: "echo 'Tests passed'",
        pre_deploy: ["echo 'Pre-deploy hook 1'", "echo 'Pre-deploy hook 2'"],
        post_deploy: ["echo 'Post-deploy cleanup'"]
    }
    fs.write(config_file, json.pretty(default_config))
    say "  Created {config_file}"
    say ""
}

let config_raw = fs.read(config_file)
let config = json.parse(config_raw)
let app = config.app_name
let version = config.version
let environment = config.environment

say term.bold("Deploy Configuration:")
say "  App:         {app}"
say "  Version:     {version}"
say "  Environment: {environment}"
say ""

// Step 2: Validate configuration
say term.bold("═══ Validation ═══")
let mut errors = []
if app == null {
    errors = append(errors, "app_name is required")
}
if version == null {
    errors = append(errors, "version is required")
}
if environment == null {
    errors = append(errors, "environment is required")
}

if len(errors) > 0 {
    say term.error("Configuration errors:")
    for err in errors {
        say term.red("  ✗ {err}")
    }
    say ""
    say "Deploy aborted."
} else {
    say term.green("  ✓ Configuration valid")
    say ""

    // Step 3: Run build
    say term.bold("═══ Build ═══")
    let build_cmd = config.build_cmd
    let build = shell(build_cmd)
    if build.ok {
        say term.green("  ✓ Build succeeded")
    } else {
        say term.error("  ✗ Build failed")
        let stderr = build.stderr
        say "  {stderr}"
    }
    say ""

    // Step 4: Run tests
    say term.bold("═══ Tests ═══")
    let test_cmd = config.test_cmd
    let tests = shell(test_cmd)
    if tests.ok {
        say term.green("  ✓ Tests passed")
    } else {
        say term.error("  ✗ Tests failed — aborting deploy")
        let stderr = tests.stderr
        say "  {stderr}"
    }
    say ""

    // Step 5: Pre-deploy hooks
    say term.bold("═══ Pre-Deploy Hooks ═══")
    let pre = config.pre_deploy
    let mut hook_num = 1
    for cmd in pre {
        let result = shell(cmd)
        if result.ok {
            say term.green("  ✓ Hook {hook_num}: passed")
        } else {
            say term.red("  ✗ Hook {hook_num}: failed")
        }
        hook_num = hook_num + 1
    }
    say ""

    // Step 6: Deploy
    say term.bold("═══ Deploying ═══")
    let deploy_hash = crypto.sha256("{app}-{version}-{environment}")
    let short_hash = slice(deploy_hash, 0, 8)
    say "  Deploy ID: {short_hash}"
    say "  App:       {app}"
    say "  Version:   {version}"
    say "  Target:    {environment}"
    say ""

    // Step 7: Post-deploy hooks
    say term.bold("═══ Post-Deploy Hooks ═══")
    let post = config.post_deploy
    for cmd in post {
        let result = shell(cmd)
        if result.ok {
            say term.green("  ✓ Post-deploy: done")
        } else {
            say term.red("  ✗ Post-deploy: failed")
        }
    }
    say ""

    // Step 8: Write deployment log
    let log_entry = {
        app: app,
        version: version,
        environment: environment,
        deploy_id: short_hash,
        timestamp: sh("date -u +%Y-%m-%dT%H:%M:%SZ"),
        status: "success"
    }
    let log_line = json.stringify(log_entry)
    fs.write("deploy.log", log_line)
    say term.blue("  Deployment log written to deploy.log")
    say ""

    term.success("Deploy complete: {app} v{version} → {environment}")
}
```

### Project 3: Backup Automation — Scan, Archive, Rotate

This script scans a directory tree, creates timestamped backups using tar, and
rotates old backups to prevent disk exhaustion.

```forge
// backup.fg — Backup automation with rotation
// Run: forge run backup.fg

say term.banner("Backup Automation")
say ""

// Configuration
let source_dir = "/tmp/forge_backup_test"
let backup_dir = "/tmp/forge_backups"
let max_backups = 3

// Setup: create test data if it doesn't exist
if fs.exists(source_dir) == false {
    say term.blue("Creating test data...")
    fs.mkdir(source_dir)
    fs.write("{source_dir}/config.json", json.pretty({ app: "myservice", port: 8080 }))
    fs.write("{source_dir}/data.csv", "id,name,value\n1,alpha,100\n2,beta,200\n3,gamma,300")
    fs.write("{source_dir}/readme.txt", "This is the project readme file.")
    say "  Created test files in {source_dir}"
    say ""
}

// Create backup directory
if fs.exists(backup_dir) == false {
    fs.mkdir(backup_dir)
    say "  Created backup directory: {backup_dir}"
}

// Step 1: Inventory source files
say term.bold("═══ Source Inventory ═══")
let files = fs.list(source_dir)
let mut file_table = []
for file in files {
    let full_path = "{source_dir}/{file}"
    let fsize = fs.size(full_path)
    let row = { File: file, Size: fsize }
    file_table = append(file_table, row)
}
term.table(file_table)
let file_count = len(files)
say "  Total files: {file_count}"
say ""

// Step 2: Create timestamped backup
say term.bold("═══ Creating Backup ═══")
let timestamp = sh("date +%Y%m%d_%H%M%S")
let backup_name = "backup_{timestamp}.tar.gz"
let backup_path = "{backup_dir}/{backup_name}"

let tar_cmd = "tar -czf {backup_path} -C {source_dir} ."
let result = shell(tar_cmd)
if result.ok {
    let bsize = fs.size(backup_path)
    say term.green("  ✓ Created: {backup_name}")
    say "  Size: {bsize} bytes"
} else {
    let err = result.stderr
    say term.error("  ✗ Backup failed: {err}")
}
say ""

// Step 3: List existing backups
say term.bold("═══ Existing Backups ═══")
let all_files = fs.list(backup_dir)
let mut backups = []
for file in all_files {
    if starts_with(file, "backup_") {
        backups = append(backups, file)
    }
}
let backups = sort(backups)
let mut backup_table = []
for b in backups {
    let bpath = "{backup_dir}/{b}"
    let bsize = fs.size(bpath)
    let row = { Backup: b, Size: bsize }
    backup_table = append(backup_table, row)
}
term.table(backup_table)
let backup_count = len(backups)
say "  Total backups: {backup_count}"
say ""

// Step 4: Rotate — delete oldest backups if over limit
say term.bold("═══ Rotation ═══")
if backup_count > max_backups {
    let to_delete = backup_count - max_backups
    say "  Max backups: {max_backups}"
    say "  Current:     {backup_count}"
    say "  Removing:    {to_delete} oldest"
    say ""

    let mut deleted = 0
    for b in backups {
        if deleted < to_delete {
            let del_path = "{backup_dir}/{b}"
            fs.remove(del_path)
            say term.red("  ✗ Deleted: {b}")
            deleted = deleted + 1
        }
    }
} else {
    let remaining = max_backups - backup_count
    say "  Within limit ({backup_count}/{max_backups}). {remaining} slots remaining."
}
say ""

// Step 5: Generate backup hash for integrity verification
say term.bold("═══ Integrity ═══")
let hash = crypto.sha256(backup_name)
say "  Backup: {backup_name}"
say "  SHA256: {hash}"
say ""

// Summary
let summary = {
    source: source_dir,
    destination: backup_dir,
    backup_file: backup_name,
    files_backed_up: file_count,
    timestamp: sh("date -u +%Y-%m-%dT%H:%M:%SZ"),
    hash: hash
}
fs.write("{backup_dir}/last_backup.json", json.pretty(summary))
say term.blue("  Manifest written to {backup_dir}/last_backup.json")
say ""

term.success("Backup complete!")
```

### Going Further

- **Cron integration.** Schedule your Forge scripts with cron: `*/30 * * * * forge run backup.fg`.
- **Remote execution.** Use `shell("ssh user@host 'command'")` to run commands on remote
  servers.
- **Monitoring loops.** Combine `schedule every 30 seconds { }` with health checks for
  a lightweight monitoring daemon.
- **Configuration management.** Build a Forge script that reads a YAML-like config,
  templates configuration files, and deploys them to the right directories.
- **Alerting.** Pipe health check results to `http.post()` calls to send alerts to
  Slack or PagerDuty webhooks.

---

## Chapter 24: AI Integration

Forge has a built-in connection to large language models through the `ask` keyword. This
isn't a library you install or an API you configure—it's a language-level primitive that
sends a prompt to an LLM and returns the response as a string. Combined with Forge's
file system access, data processing capabilities, and terminal formatting, this turns
Forge into a powerful tool for building AI-augmented scripts and workflows.

### The ask Keyword

The `ask` keyword sends a string prompt to an OpenAI-compatible API and returns the
response:

```forge
let answer = ask "What is the capital of France?"
say answer
```

Under the hood, `ask` makes a POST request to the chat completions API with the prompt
as a user message. The response is extracted from the API response and returned as a
plain string.

### Environment Setup

Before using `ask`, you need to set one of these environment variables:

```bash
# Option 1: Forge-specific key
export FORGE_AI_KEY="sk-your-api-key-here"

# Option 2: Standard OpenAI key
export OPENAI_API_KEY="sk-your-api-key-here"
```

Forge checks `FORGE_AI_KEY` first, then falls back to `OPENAI_API_KEY`. You can also
customize the model and endpoint:

```bash
# Use a different model (default: gpt-4o-mini)
export FORGE_AI_MODEL="gpt-4o"

# Use a different API endpoint (for local models, Azure, etc.)
export FORGE_AI_URL="http://localhost:11434/v1/chat/completions"
```

The `FORGE_AI_URL` variable makes it possible to use Forge with any OpenAI-compatible
API, including local models running through Ollama or LM Studio.

### Prompt Templates

Since `ask` takes a string, you can build prompts dynamically using Forge's string
interpolation:

```forge
let language = "Python"
let topic = "list comprehensions"
let prompt = "Explain {topic} in {language} with 3 examples. Be concise."
let explanation = ask prompt
say explanation
```

For longer prompts, build them up with string concatenation or multi-line construction:

```forge
let code = fs.read("my_script.py")
let prompt = "Review this code for bugs and suggest improvements:\n\n{code}"
let review = ask prompt
say review
```

### Forge Chat Mode

Beyond programmatic use, Forge includes a built-in chat mode for interactive
conversations with an LLM:

```bash
$ forge chat
```

This starts an interactive REPL where you can have a conversation with the configured
LLM. It's useful for quick questions, brainstorming, and exploration without writing
a script.

### Project 1: Code Reviewer — File Analysis with LLM Feedback

This program reads a source file, sends it to an LLM for review, and displays the
feedback with terminal formatting.

```forge
// code_reviewer.fg — AI-powered code review
// Run: FORGE_AI_KEY=sk-... forge run code_reviewer.fg
// Requires: FORGE_AI_KEY or OPENAI_API_KEY environment variable

say term.banner("AI Code Reviewer")
say ""

// Check for API key
let has_key = env.has("FORGE_AI_KEY")
let has_openai = env.has("OPENAI_API_KEY")
if has_key == false {
    if has_openai == false {
        say term.error("No API key found!")
        say "Set FORGE_AI_KEY or OPENAI_API_KEY environment variable."
        say ""
        say "Example:"
        say "  export FORGE_AI_KEY=sk-your-key-here"
        say "  forge run code_reviewer.fg"
    }
}

// Read the target file
let target_file = "examples/hello.fg"
if fs.exists(target_file) == false {
    say term.error("File not found: {target_file}")
} else {
    let code = fs.read(target_file)
    let file_size = fs.size(target_file)
    let line_list = split(code, "\n")
    let line_count = len(line_list)

    say term.bold("File: {target_file}")
    say "  Size:  {file_size} bytes"
    say "  Lines: {line_count}"
    say ""

    say term.blue("Sending to AI for review...")
    say ""

    let prompt = "You are a senior code reviewer. Review the following Forge programming language code. Forge is similar to JavaScript/Python with keywords like 'say' for print, 'let' for variables, and 'fn' for functions. Provide:\n1. A brief summary of what the code does\n2. Code quality assessment (1-10)\n3. Any bugs or issues\n4. Suggestions for improvement\n\nCode:\n\n{code}"

    let review = ask prompt

    say term.bold("═══ AI Review ═══")
    say ""
    say review
    say ""

    // Save the review
    let review_file = "code_review.md"
    let review_content = "# Code Review: {target_file}\n\n{review}"
    fs.write(review_file, review_content)
    say term.blue("Review saved to {review_file}")
    say ""

    term.success("Code review complete!")
}
```

**Walkthrough.** The script first checks that an API key is configured. Then it reads
the target source file, builds a detailed prompt that includes the code and specific
instructions for the review format, sends it to the LLM with `ask`, and displays the
result. The review is also saved to a Markdown file for later reference.

### Project 2: Data Describer — Natural Language Dataset Summary

This program loads a CSV dataset, computes basic statistics, and uses an LLM to
generate a natural language description of the data—useful for reports, documentation,
or quick data exploration.

```forge
// data_describer.fg — AI-powered dataset description
// Run: FORGE_AI_KEY=sk-... forge run data_describer.fg
// Requires: FORGE_AI_KEY or OPENAI_API_KEY environment variable

say term.banner("AI Data Describer")
say ""

// Generate sample data
let csv_file = "sample_data.csv"
if fs.exists(csv_file) == false {
    say term.blue("Creating sample dataset...")
    let data = "name,age,department,salary,performance_score
Alice Chen,32,Engineering,128000,4.5
Bob Martinez,28,Marketing,89000,3.8
Carol Kim,41,Engineering,155000,4.9
David Johnson,35,Sales,95000,4.1
Eva Schmidt,29,Engineering,118000,4.3
Frank Brown,45,Marketing,105000,3.5
Grace Liu,33,Sales,91000,4.7
Henry Wilson,38,Engineering,142000,4.0
Isabel Torres,27,Marketing,82000,4.2
James Park,36,Sales,98000,3.9"
    fs.write(csv_file, data)
    say "  Created {csv_file}"
    say ""
}

// Read and parse
let raw = fs.read(csv_file)
let records = csv.parse(raw)
let record_count = len(records)

say term.bold("Dataset: {csv_file}")
say "  Records: {record_count}"
say ""

// Display the data
say term.bold("═══ Raw Data ═══")
term.table(records)
say ""

// Compute statistics using the database
db.open(":memory:")
db.execute("CREATE TABLE data (name TEXT, age INTEGER, department TEXT, salary REAL, performance_score REAL)")
for row in records {
    let n = row.name
    let a = row.age
    let d = row.department
    let s = row.salary
    let p = row.performance_score
    db.execute("INSERT INTO data VALUES ('{n}', {a}, '{d}', {s}, {p})")
}

let stats = db.query("SELECT COUNT(*) as count, ROUND(AVG(age), 1) as avg_age, ROUND(AVG(salary), 0) as avg_salary, ROUND(AVG(performance_score), 2) as avg_perf, MIN(salary) as min_salary, MAX(salary) as max_salary FROM data")
let dept_stats = db.query("SELECT department, COUNT(*) as headcount, ROUND(AVG(salary), 0) as avg_salary, ROUND(AVG(performance_score), 2) as avg_perf FROM data GROUP BY department ORDER BY avg_salary DESC")

say term.bold("═══ Statistics ═══")
term.table(stats)
say ""
say term.bold("═══ By Department ═══")
term.table(dept_stats)
say ""

db.close()

// Build context for the AI
let stats_str = json.stringify(stats[0])
let dept_str = json.stringify(dept_stats)

let prompt = "You are a data analyst. Given this employee dataset summary, write a 3-4 paragraph natural language description suitable for a business report. Be specific with numbers.\n\nOverall statistics: {stats_str}\n\nDepartment breakdown: {dept_str}\n\nDataset has {record_count} employee records with columns: name, age, department, salary, performance_score."

say term.blue("Generating natural language description...")
say ""

let description = ask prompt

say term.bold("═══ AI-Generated Description ═══")
say ""
say description
say ""

// Save report
let report = "# Dataset Report: {csv_file}\n\nGenerated: {sh("date")}\n\n## Summary\n\n{description}"
fs.write("data_report.md", report)
say term.blue("Report saved to data_report.md")
say ""

// Clean up sample data
fs.remove(csv_file)

term.success("Data description complete!")
```

**The AI integration flow:**

```
  ┌────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
  │  CSV File  │────►│  Statistics  │────►│  AI Prompt   │────►│  Report  │
  │            │     │  (SQLite)    │     │  (ask)       │     │  (.md)   │
  │ raw data   │     │  aggregates  │     │  context +   │     │  human-  │
  │            │     │  dept stats  │     │  instructions│     │  readable│
  └────────────┘     └──────────────┘     └──────────────┘     └──────────┘
```

The power of this approach is the combination: Forge does the data processing (which
it's good at—fast, deterministic, SQL-powered), then hands the structured results to
the LLM for natural language generation (which the LLM is good at). Each tool does
what it does best.

### Going Further

- **Custom models.** Set `FORGE_AI_URL` to point to a local Ollama instance for
  private, offline AI capabilities: `export FORGE_AI_URL=http://localhost:11434/v1/chat/completions`
- **Prompt chaining.** Use the output of one `ask` call as input to the next,
  building multi-step reasoning pipelines.
- **Tool use patterns.** Have the LLM output structured JSON, parse it with
  `json.parse()`, and use the result to drive further program logic—a simple form
  of agentic behavior.
- **Batch processing.** Loop over multiple files or data records, sending each to the
  LLM and collecting responses for bulk analysis.
- **RAG-like patterns.** Read local files for context, include relevant excerpts in the
  prompt, and get answers grounded in your own data.

---

_This concludes Part III: Building Real Things. You've built REST APIs, consumed external
services, processed data pipelines, automated system operations, and integrated AI—all
with a language that compiles to a single binary and requires zero external dependencies.
In Part IV, we'll look at Forge's tooling ecosystem: the formatter, test runner, LSP,
and how to publish Forge packages._
