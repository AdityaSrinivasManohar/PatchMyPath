# How the backend works

A line-by-line walkthrough of `src/main.rs` for someone new to Rust.

---

## The imports

```rust
use std::sync::{Arc, Mutex};
```

These two types solve a specific problem: **how do multiple requests share the same database connection safely?**

- **`Mutex<T>`** — a "mutual exclusion" lock. Only one thread can access the data inside it at a time. Think of it like a bathroom with one key — you grab the key, do your business, return the key. No one else can enter while you hold it.

- **`Arc<T>`** — "Atomically Reference Counted". This lets multiple parts of your program *share ownership* of the same value. Normally in Rust, one piece of code owns a value and others borrow it. But a web server handles many requests at the same time — you can't have one request "own" the database connection. `Arc` lets everyone have a handle to the same data, and it automatically cleans up when the last handle is dropped.

Together, `Arc<Mutex<T>>` is the standard Rust pattern for **shared mutable state across threads**: Arc for shared ownership, Mutex for safe mutation.

```rust
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
```

Pulling in the pieces of Axum we need:
- `Router` — maps URL paths to handler functions
- `Json` — wraps a value to serialize it as JSON (or deserialize incoming JSON)
- `State` — an *extractor* that pulls the shared `AppState` out and hands it to your handler
- `StatusCode` — HTTP status codes like `200 OK`, `201 Created`
- `get`, `post` — tell the router which HTTP method a route responds to

```rust
use chrono::{DateTime, Utc};
```

`Utc::now()` gets the current timestamp when a report is created. `DateTime` is used when parsing timestamps back out of the database.

```rust
use rusqlite::{Connection, params};
```

`Connection` is the SQLite database handle. `params!` is a macro for passing typed values into SQL queries safely (prevents SQL injection by binding values separately from the query string).

```rust
use shared::{CreateReportRequest, DamageReport, DamageType, FixStatus, GPSLocation};
use tower_http::cors::CorsLayer;
```

Our own domain types from the `shared` crate, and CORS middleware from `tower-http`.

---

## The state type alias

```rust
type AppState = Arc<Mutex<Connection>>;
```

This gives a short name to `Arc<Mutex<Connection>>` so we don't have to write it out everywhere. Reading it inside-out:
- `Connection` — the SQLite database handle (one connection shared by all requests)
- `Mutex<...>` — wrap it so only one request can use it at a time (SQLite handles one writer at a time)
- `Arc<...>` — wrap that so every request handler can hold a reference to the same connection

---

## Setting up the database

```rust
fn init_db() -> Connection {
    let conn = Connection::open("reports.db").unwrap();
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS reports (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            damage_type TEXT    NOT NULL,
            latitude    REAL    NOT NULL,
            longitude   REAL    NOT NULL,
            severity    INTEGER NOT NULL,
            description TEXT    NOT NULL,
            image       TEXT,
            timestamp   TEXT    NOT NULL,
            status      TEXT    NOT NULL
        );
    ").unwrap();
    conn
}
```

`Connection::open("reports.db")` opens the SQLite file at that path — or creates it if it doesn't exist. This is how SQLite works: the whole database is a single file.

`CREATE TABLE IF NOT EXISTS` is safe to run every startup. If the table already exists, it's a no-op. If it's a fresh database, it creates the table.

A few things worth noting about the schema:
- **Enums as TEXT** — SQLite has no enum type. `damage_type` and `status` are stored as their string names (`"Pothole"`, `"Pending"`, etc.) and parsed back into Rust enums when reading.
- **`latitude`/`longitude` as REAL** — SQLite's floating-point type, maps directly to Rust's `f64`.
- **`timestamp` as TEXT** — stored as an RFC3339 string (e.g. `"2024-01-15T10:30:00Z"`), parsed back into `DateTime<Utc>` on read.
- **`image` as nullable TEXT** — `Option<String>` in Rust maps to a nullable column in SQLite.

The function returns the `Connection` so `main` can store it in shared state.

---

## The GET handler

```rust
async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let conn = state.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT damage_type, latitude, longitude, severity, description, image, timestamp, status FROM reports"
    ).unwrap();

    let reports = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, u8>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    }).unwrap()
    .filter_map(|r| r.ok())
    .map(|(damage_type_str, lat, lng, severity, description, image, timestamp_str, status_str)| {
        DamageReport {
            damage_type: match damage_type_str.as_str() {
                "CracksOnRoad" => DamageType::CracksOnRoad,
                "WaterLeak"    => DamageType::WaterLeak,
                _              => DamageType::Pothole,
            },
            location: GPSLocation { latitude: lat, longitude: lng },
            severity,
            description,
            image,
            timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                .unwrap()
                .with_timezone(&Utc),
            status: match status_str.as_str() {
                "InProgress" => FixStatus::InProgress,
                "Completed"  => FixStatus::Completed,
                _            => FixStatus::Pending,
            },
        }
    })
    .collect();

    Json(reports)
}
```

**`state.lock().unwrap()`** — acquires the Mutex lock, giving exclusive access to the `Connection`. The lock is released automatically when `conn` goes out of scope at the end of the function.

**`conn.prepare(...)`** — compiles the SQL query. `prepare` returns a `Statement` that can be executed. It's slightly more efficient than running raw SQL directly for queries you run repeatedly.

**`query_map([], |row| { ... })`** — executes the query (the `[]` means no parameters) and applies a closure to each row, extracting typed values via `row.get::<_, T>(column_index)`. The double-underscore in `get::<_, T>` lets Rust infer the first type argument while we specify `T` explicitly. Each call returns `Result`, and the `?` operator propagates errors.

**`.filter_map(|r| r.ok())`** — skips any rows that failed to parse rather than crashing the whole request.

**`.map(|...| DamageReport { ... })`** — converts each raw tuple of column values back into a `DamageReport`, parsing the string columns back into their proper Rust types: enum variants for `damage_type` and `status`, and `DateTime::parse_from_rfc3339` for the timestamp.

**`.collect()`** — gathers the iterator into a `Vec<DamageReport>`.

---

## The POST handler

```rust
async fn create_report(
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> (StatusCode, Json<DamageReport>) {
```

Two extractors:
- `State(state)` — gives us the shared database connection
- `Json(req): Json<CreateReportRequest>` — Axum reads the request body, parses it as JSON into a `CreateReportRequest`, and hands it to us as `req`. If the body is malformed, Axum automatically returns `400 Bad Request` before your function even runs.

The return type `(StatusCode, Json<DamageReport>)` is a tuple — Axum understands this as "send this status code with this JSON body". This is how you return `201 Created` instead of the default `200 OK`.

```rust
    let report = DamageReport {
        damage_type: req.damage_type,
        location: req.location,
        severity: req.severity,
        description: req.description,
        image: req.image,
        timestamp: Utc::now(),
        status: FixStatus::Pending,
    };
```

Build the full `DamageReport` from the request. The client only sent the user-supplied fields. The server adds `timestamp` and `status` — the client shouldn't be trusted to set these.

```rust
    let conn = state.lock().unwrap();
    conn.execute(
        "INSERT INTO reports (damage_type, latitude, ...) VALUES (?1, ?2, ...)",
        params![
            format!("{:?}", report.damage_type),
            report.location.latitude,
            ...
            report.timestamp.to_rfc3339(),
            format!("{:?}", report.status),
        ],
    ).unwrap();

    (StatusCode::CREATED, Json(report))
```

**`format!("{:?}", report.damage_type)`** — uses the `Debug` representation of the enum to get its string name (`"Pothole"`, `"CracksOnRoad"`, `"WaterLeak"`). This is what gets stored in the TEXT column and matched back on read.

**`params![...]`** — the rusqlite macro for binding values to `?1`, `?2`, etc. placeholders. This is important: values are passed separately from the query string, so user input can never be interpreted as SQL (SQL injection prevention).

**`report.timestamp.to_rfc3339()`** — serializes the timestamp to a standard string like `"2024-01-15T10:30:00Z"` for storage.

---

## The main function

```rust
#[tokio::main]
async fn main() {
    let conn = init_db();
    let state: AppState = Arc::new(Mutex::new(conn));
    ...
}
```

`#[tokio::main]` is a *procedural macro* — it rewrites your `async fn main()` into a regular `fn main()` that sets up the Tokio async runtime and runs your code inside it.

`init_db()` opens (or creates) `reports.db` and returns the connection. That connection is immediately wrapped in `Arc<Mutex<...>>` and registered as the router's state via `.with_state(state)`. Every handler that declares `State(state): State<AppState>` gets an `Arc` clone pointing at the same connection.

```rust
    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .layer(CorsLayer::permissive())
        .with_state(state);
```

- `GET /api/reports` → `list_reports`
- `POST /api/reports` → `create_report`
- `.layer(CorsLayer::permissive())` — CORS middleware so the browser frontend (port 8080) can call this server (port 3000) during development
- `.with_state(state)` — attaches the shared state so Axum knows what to inject

```rust
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
```

Opens a TCP socket on port 3000 (`0.0.0.0` = all network interfaces) and hands it to Axum. `.await` suspends until the server shuts down.

---

## The full picture

```
HTTP request arrives
        │
        ▼
    Router
  (matches path + method)
        │
        ▼
  Handler function
  (Axum injects State<Connection>, Json, etc. automatically)
        │
        ├── locks Mutex to get exclusive access to Connection
        ├── runs SQL query (SELECT or INSERT)
        ├── maps rows ↔ DamageReport structs
        └── returns Json<T> or (StatusCode, Json<T>)
                │
                ▼
        Axum serializes to JSON
        and sends HTTP response
```

| Concept | Where | Why |
|---|---|---|
| `Arc<Mutex<T>>` | `AppState` | Safe shared mutable state across async tasks |
| `rusqlite::Connection` | inside `AppState` | The SQLite database handle |
| `params![...]` | INSERT query | Binds values safely, prevents SQL injection |
| Enums as TEXT | `damage_type`, `status` columns | SQLite has no enum type; store and parse string names |
| RFC3339 strings | `timestamp` column | Standard format for storing datetimes as text |
| Extractors | `State(...)`, `Json(...)` | Axum reads function signatures to inject the right values |
| `async/await` | everywhere | Non-blocking request handling without manual threads |

---

## Deep dive: `Arc<Mutex<T>>`

### The Google Doc analogy

**`Arc` — the shared link to the doc**

`Arc` is like the **URL to a Google Doc**. Everyone gets their own copy of the URL, but they all point to the *same underlying document*. Sharing the URL is cheap — you're not copying the whole doc. When the last person closes their tab, the doc gets cleaned up automatically.

In code terms: cloning an `Arc` just increments a counter. The actual `Connection` inside exists once in memory.

**`Mutex` — the "one editor at a time" rule**

A Google Doc lets multiple people view it, but if two people edit the same sentence simultaneously you get chaos. So imagine a rule: **only one person can type at a time**. You click "Edit", make your changes, click "Done" — and the next person gets in.

That's a `Mutex`. When you call `.lock()`, you grab exclusive access. Everyone else waits. When the lock guard goes out of scope (end of the function), it releases automatically.

**Together:**

```
Arc  = the shared URL (everyone holds a reference to the same thing)
Mutex = the one-editor-at-a-time rule (safe to modify without chaos)

Arc<Mutex<Connection>>
 │       │        └── the SQLite database connection
 │       └── only one request can use it at a time
 └── every request handler shares the same one
```

### Why not just use a reference (`&`) instead?

A reference in Rust has a *lifetime* — it's only valid as long as the thing it points to is alive. In a web server, `main()` creates the connection and then calls `axum::serve(...)` which runs forever, spawning a new async task per request. Each task can outlive the scope where the connection was created — or at least the compiler can't *prove* it won't. So it rejects a `&Connection` here.

`Arc` sidesteps this entirely. The data lives on the heap and is owned jointly by everyone holding an Arc. There's no single parent scope the data is tied to.

**What Arc looks like in memory:**

```
stack (main)          heap
┌──────────┐         ┌───────────────────────┐
│  arc1    ├────────►│  count: 3             │
└──────────┘         │  data: Connection     │
                     └───────────────────────┘
stack (request 1)           ▲
┌──────────┐                │
│  arc2    ├────────────────┘
└──────────┘                │
stack (request 2)           │
┌──────────┐                │
│  arc3    ├────────────────┘
└──────────┘
```

Each `Arc::clone()` just increments the counter — no data is copied. When a task finishes and its Arc is dropped, the counter decrements. When it hits 0, the connection is closed and freed.

| | `&T` | `Arc<T>` |
|---|---|---|
| Where does data live? | somewhere with a known lifetime | heap, owned by the Arc itself |
| How does compiler verify safety? | tracks lifetime statically | reference counting at runtime |
| Cost of sharing | free (just a pointer) | cheap (increment a counter) |
| Works across async tasks? | no — lifetime can't be proven | yes |
| When is data freed? | when the owner is dropped | when the last Arc is dropped |
