# How the backend works

A walkthrough of `backend/src/main.rs` for someone new to Rust web servers.

---

## What the backend does

The backend is a single binary that:

1. Opens (or creates) a SQLite database file.
2. Serves a REST API for road damage reports.
3. In production, also serves the compiled frontend (HTML, WASM, CSS) as static files.

Everything is configured via environment variables so the same binary works in both local dev and production.

---

## Imports

```rust
use std::sync::{Arc, Mutex};
```

Two types that solve the same problem: **how do multiple concurrent requests safely share one database connection?**

- **`Mutex<T>`** — a mutual exclusion lock. Only one thread can access the data inside it at a time. Everyone else waits. The lock releases automatically when the lock guard goes out of scope.
- **`Arc<T>`** — "Atomically Reference Counted". Lets multiple parts of your program share ownership of the same value. Normally in Rust one piece of code owns a value. But a web server handles many requests at once — you can't have one request "own" the database. `Arc` lets everyone hold a handle to the same data, and the data is freed only when the last handle is dropped.

Together, `Arc<Mutex<T>>` is the standard Rust pattern for shared mutable state across threads.

```rust
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, patch, post},
};
```

- **`Router`** — maps URL paths to handler functions.
- **`Json`** — wraps a value to serialize it as JSON, or deserializes incoming JSON from a request body.
- **`State`** — an extractor that pulls the shared `AppState` out of the router and injects it into a handler.
- **`Path`** — extracts a path segment like `{id}` from the URL.
- **`HeaderMap`** — gives access to request headers (used for reading the `Authorization` header).
- **`StatusCode`** — HTTP status codes: `200 OK`, `201 Created`, `401 Unauthorized`, etc.
- **`get`, `post`, `patch`, `delete`** — tell the router which HTTP method a route responds to.

---

## `AppData` — shared state

```rust
struct AppData {
    db: Mutex<Connection>,
    admin_password: String,
}
type AppState = Arc<AppData>;
```

All route handlers share one `AppData` via Axum's `State` extractor. Reading inside-out:

- **`Connection`** — the rusqlite database handle (one connection for the whole server).
- **`Mutex<Connection>`** — wraps it so only one request can run a query at a time. SQLite supports one writer at a time, so this is correct.
- **`admin_password`** — the admin password, read from the `ADMIN_PASSWORD` environment variable at startup. Stored here so every handler can check it without re-reading the env var.
- **`Arc<AppData>`** — wraps the whole struct so every request handler holds a reference to the same data. Cheap to clone (just increments a counter).

---

## `check_auth` — authentication helper

```rust
fn check_auth(headers: &HeaderMap, admin_password: &str) -> bool {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == admin_password)
        .unwrap_or(false)
}
```

The admin frontend sends the password as an HTTP header on every protected request:

```
Authorization: Bearer <password>
```

This function extracts the token from that header and compares it to the stored password. Returns `true` if they match, `false` for anything else (missing header, wrong password, malformed header).

**Why check on every request?** The frontend is WASM running in the browser — any user can inspect it. Secrets cannot be stored there. The backend must verify the password on every protected request, not just at login time. The `/api/admin/ping` endpoint exists purely as a login check: the frontend hits it to verify a password before showing the admin panel, without actually touching any data.

---

## Database setup

```rust
fn init_db() -> Connection {
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "reports.db".to_string());
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS reports (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            damage_type TEXT    NOT NULL,
            ...
        );
    ").unwrap();
    conn
}
```

`Connection::open` opens the SQLite file at `DB_PATH` — or creates it if it doesn't exist. This is how SQLite works: the whole database is a single file.

`CREATE TABLE IF NOT EXISTS` is safe to run every startup. If the table already exists it's a no-op. If it's a fresh database it creates the table.

**Schema notes:**

| Column | SQLite type | Rust type | Why |
|---|---|---|---|
| `id` | `INTEGER PRIMARY KEY AUTOINCREMENT` | `i64` | Auto-assigned unique ID per row |
| `damage_type` | `TEXT` | `DamageType` enum | SQLite has no enum type; stored as `"Pothole"`, `"CracksOnRoad"`, etc. |
| `status` | `TEXT` | `FixStatus` enum | Same — stored as `"Pending"`, `"InProgress"`, `"Completed"` |
| `latitude`/`longitude` | `REAL` | `f64` | SQLite floating-point maps directly to Rust `f64` |
| `timestamp` | `TEXT` | `DateTime<Utc>` | Stored as RFC3339 string, e.g. `"2024-01-15T10:30:00Z"` |
| `image` | `TEXT` (nullable) | `Option<String>` | `NULL` in SQLite maps to `None` in Rust |

---

## Routes

### `GET /api/reports` — list all reports

```rust
async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, damage_type, ... FROM reports").unwrap();
    ...
    Json(reports)
}
```

No auth required — reports are public.

**`state.db.lock().unwrap()`** — acquires the Mutex lock, giving exclusive access to the `Connection`. The lock releases automatically when `conn` drops at end of function.

**`query_map([], |row| { ... })`** — executes the query (empty `[]` = no parameters) and applies a closure to each row. `row.get::<_, T>(index)` extracts a typed value from a column. The `?` propagates errors per row.

**`.filter_map(|r| r.ok())`** — skips malformed rows rather than crashing the whole request.

**`.map(|...| DamageReport { ... })`** — parses each raw tuple back into a `DamageReport`: string columns back to enums, the RFC3339 string back to `DateTime<Utc>`.

---

### `POST /api/reports` — create a report

```rust
async fn create_report(
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> (StatusCode, Json<DamageReport>) {
```

No auth required — anyone can submit a report.

Two extractors:
- **`State(state)`** — shared database connection.
- **`Json(req)`** — Axum reads the request body, parses it as JSON into a `CreateReportRequest`, and hands it to us. If the body is malformed, Axum returns `400 Bad Request` before your function even runs.

The return type `(StatusCode, Json<DamageReport>)` is a tuple — Axum serializes this as a response with the given status code and JSON body. This is how we return `201 Created` instead of the default `200 OK`.

**`format!("{:?}", req.damage_type)`** — uses the `Debug` representation of the enum to get its string name for storage. The same string is matched back to an enum variant when reading.

**`params![...]`** — rusqlite's macro for binding values to `?1`, `?2`, ... placeholders. Values are passed separately from the query string — user input can never be interpreted as SQL (prevents SQL injection).

**`conn.last_insert_rowid()`** — gets the auto-generated `id` for the row just inserted. This is sent back to the frontend in the response so it can reference the report later (e.g. in the admin panel).

---

### `GET /api/admin/ping` — password check

```rust
async fn admin_ping(State(state): State<AppState>, headers: HeaderMap) -> StatusCode {
    if check_auth(&headers, &state.admin_password) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}
```

This endpoint exists purely for the login flow. The frontend sends the entered password here before showing the admin panel. A `200` means "proceed"; a `401` means "wrong password". No data is read or written.

---

### `PATCH /api/reports/{id}` — update status

```rust
async fn update_report_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(req): Json<UpdateStatusRequest>,
) -> StatusCode {
    if !check_auth(&headers, &state.admin_password) {
        return StatusCode::UNAUTHORIZED;
    }
    ...
    conn.execute("UPDATE reports SET status = ?1 WHERE id = ?2", params![...]);
    StatusCode::OK
}
```

Auth required. **`Path(id): Path<i64>`** extracts the `{id}` segment from the URL as an `i64`. **Note:** Axum 0.8 uses `{id}` in route strings, not `:id` — using `:id` causes a runtime panic at startup.

---

### `DELETE /api/reports/{id}` — delete a report

```rust
async fn delete_report(...) -> StatusCode {
    if !check_auth(&headers, &state.admin_password) {
        return StatusCode::UNAUTHORIZED;
    }
    conn.execute("DELETE FROM reports WHERE id = ?1", params![id]);
    StatusCode::NO_CONTENT
}
```

Auth required. Returns `204 No Content` on success — the standard HTTP response for a successful deletion with no body.

---

## `main()` — wiring it all together

```rust
#[tokio::main]
async fn main() {
    let admin_password = std::env::var("ADMIN_PASSWORD")
        .unwrap_or_else(|_| "admin".to_string());

    let state: AppState = Arc::new(AppData {
        db: Mutex::new(init_db()),
        admin_password,
    });

    let static_dir = std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| "frontend/dist".to_string());
    let serve_dir = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", &static_dir)));

    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .route("/api/reports/{id}", patch(update_report_status))
        .route("/api/reports/{id}", delete(delete_report))
        .route("/api/admin/ping", get(admin_ping))
        .layer(CorsLayer::permissive())
        .with_state(state)
        .fallback_service(serve_dir);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**`#[tokio::main]`** — a procedural macro that rewrites `async fn main()` into a regular `fn main()` that sets up the Tokio async runtime and runs your code inside it.

**`ServeDir` + `.fallback_service`** — in production, any request that doesn't match an API route is served from the `frontend/dist/` folder. The `.not_found_service(ServeFile::new("index.html"))` fallback handles SPA routes like `/admin` that don't correspond to a real file — the browser receives `index.html` and Leptos's client-side router takes over.

**`CorsLayer::permissive()`** — allows cross-origin requests. Needed in development when the frontend runs on port 8080 and the backend on port 3000. In production both are on the same origin so this is a no-op, but it's harmless to keep.

**Environment variables:**

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `3000` | Port to bind to (Railway sets this automatically) |
| `DB_PATH` | `reports.db` | Path to the SQLite file |
| `STATIC_DIR` | `frontend/dist` | Where the compiled frontend lives |
| `ADMIN_PASSWORD` | `admin` | Admin panel password |

---

## The full request flow

```
HTTP request arrives
        │
        ▼
    CorsLayer (middleware)
        │
        ▼
    Router (matches path + method)
        │
        ├── /api/* route found
        │       │
        │       ▼
        │   Handler function
        │   Axum injects: State<AppData>, Path<i64>, Json<T>, HeaderMap
        │       │
        │       ├── check_auth (if protected route)
        │       ├── state.db.lock() → exclusive SQLite access
        │       ├── run SQL query (SELECT / INSERT / UPDATE / DELETE)
        │       ├── map rows ↔ structs
        │       └── return StatusCode or Json<T>
        │
        └── no /api/* match
                │
                ▼
            ServeDir (static files from frontend/dist/)
                │
                ├── file exists → serve it (WASM, JS, CSS, images)
                └── file missing → serve index.html (SPA fallback)
```

---

## Deep dive: `Arc<AppData>`

### The Google Doc analogy

**`Arc` — the shared link to the doc**

`Arc` is like the URL to a Google Doc. Everyone gets their own copy of the URL, but they all point to the same underlying document. Sharing the URL is cheap — you're not copying the doc. When the last person closes their tab, the doc is cleaned up automatically.

In code terms: cloning an `Arc` just increments a counter. The actual `AppData` (with the database connection inside) exists once in memory.

**`Mutex` — one editor at a time**

Imagine a rule: only one person can edit the document at once. You click "Edit", make your changes, click "Done" — the next person gets in.

That's `Mutex`. When you call `.lock()`, you get exclusive access. Everyone else blocks. The lock releases automatically when the guard goes out of scope.

**Together:**

```
Arc      = the shared URL  (everyone holds a reference to the same AppData)
Mutex    = one editor at a time  (safe to mutate without concurrent chaos)

Arc<AppData>
 │   └── AppData
 │        ├── Mutex<Connection>  ← only one query runs at a time
 │        └── admin_password     ← read-only, no lock needed
 └── every request handler gets a clone of this Arc
```

### Why not just use a reference (`&`) instead?

A Rust reference has a *lifetime* — it's only valid as long as the thing it points to is alive. In a web server, `main()` creates `AppData` and then calls `axum::serve(...)` which runs forever, spawning an async task per request. Each task could outlive the scope where `AppData` was created — or at least the compiler can't *prove* it won't. So it rejects `&AppData` here.

`Arc` sidesteps this entirely. The data lives on the heap and is jointly owned by all Arc holders. There's no single parent scope it's tied to.

```
stack (main)          heap
┌──────────┐         ┌──────────────────────┐
│  arc1    ├────────►│  count: 3            │
└──────────┘         │  data: AppData       │
                     └──────────────────────┘
stack (request 1)           ▲
┌──────────┐                │
│  arc2    ├────────────────┘
└──────────┘                │
stack (request 2)           │
┌──────────┐                │
│  arc3    ├────────────────┘
└──────────┘
```

Each `Arc::clone()` just increments the counter — no data is copied. When a task finishes, its Arc is dropped and the counter decrements. When it hits 0, `AppData` is freed.

| | `&T` | `Arc<T>` |
|---|---|---|
| Where does data live? | somewhere with a known lifetime | heap, owned by the Arc |
| How is safety verified? | statically, by the compiler | reference count at runtime |
| Cost of sharing | free (just a pointer) | cheap (increment a counter) |
| Works across async tasks? | no | yes |
| When is data freed? | when the owner is dropped | when the last Arc is dropped |
