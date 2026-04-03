# How the backend works

A line-by-line walkthrough of `src/main.rs` for someone new to Rust.

---

## The imports

```rust
use std::sync::{Arc, Mutex};
```

These two types solve a specific problem: **how do multiple requests share the same list of reports safely?**

- **`Mutex<T>`** — a "mutual exclusion" lock. Only one thread can access the data inside it at a time. Think of it like a bathroom with one key — you grab the key, do your business, return the key. No one else can enter while you hold it.

- **`Arc<T>`** — "Atomically Reference Counted". This lets multiple parts of your program *share ownership* of the same value. Normally in Rust, one piece of code owns a value and others borrow it. But a web server handles many requests at the same time — you can't have one request "own" the report list. `Arc` lets everyone have a handle to the same data, and it automatically cleans up when the last handle is dropped.

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
use chrono::Utc;
```
For `Utc::now()` — gets the current timestamp when a report is created.

```rust
use shared::{CreateReportRequest, DamageReport, FixStatus};
use tower_http::cors::CorsLayer;
```
Our own domain types from the `shared` crate, and CORS middleware from `tower-http`.

---

## The state type alias

```rust
type AppState = Arc<Mutex<Vec<DamageReport>>>;
```

This just gives a shorter name to `Arc<Mutex<Vec<DamageReport>>>` so we don't have to type that everywhere.

Reading it inside-out:
- `Vec<DamageReport>` — a growable list of reports (this is our "database" for now)
- `Mutex<...>` — wrap it so only one request can modify it at a time
- `Arc<...>` — wrap that so every request handler can hold a reference to the same list

---

## The GET handler

```rust
async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let reports = state.lock().unwrap();
    Json(reports.clone())
}
```

**`async fn`** — this function is asynchronous. It can pause and let other requests run while waiting for I/O (like a database query). Rust doesn't have a built-in async runtime — that's what Tokio provides.

**`State(state): State<AppState>`** — this is an *extractor*. Axum looks at the function's parameters to figure out what to inject. `State<AppState>` tells Axum: "give me the shared state". The `State(state)` part is *destructuring* — it unwraps the `State` wrapper so `state` is the `Arc<Mutex<Vec<DamageReport>>>` directly.

**`state.lock().unwrap()`** — acquires the Mutex lock. This gives you a `MutexGuard`, which acts like a reference to the `Vec` inside. `.unwrap()` handles the error case where the Mutex is "poisoned" (another thread panicked while holding the lock — rare in practice). The lock is automatically released when `reports` goes out of scope at the end of the function.

**`Json(reports.clone())`** — clones the Vec out of the lock (we can't return a reference to data behind a Mutex — the lock would need to stay held forever), then wraps it in `Json` which tells Axum to serialize it to JSON and set `Content-Type: application/json`.

---

## The POST handler

```rust
async fn create_report(
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> (StatusCode, Json<DamageReport>) {
```

Two extractors this time:
- `State(state)` — same as before, gives us the shared report list
- `Json(req): Json<CreateReportRequest>` — Axum reads the request body, parses it as JSON into a `CreateReportRequest`, and hands it to us as `req`. If the body is malformed, Axum automatically returns a `400 Bad Request` before your function even runs.

The return type `(StatusCode, Json<DamageReport>)` is a tuple — Axum understands this as "send this status code with this JSON body". This is how you return `201 Created` instead of the default `200 OK`.

```rust
    let report = DamageReport {
        damage_type: req.damage_type,
        location: req.location,
        severity: req.severity,
        description: req.description,
        image: req.image,
        timestamp: Utc::now(),   // server sets this
        status: FixStatus::Pending,  // server sets this
    };
```

Build the full `DamageReport` from the request. The client only sent the user-supplied fields (`CreateReportRequest`). The server adds `timestamp` and `status` — the client shouldn't be trusted to set these.

```rust
    let mut reports = state.lock().unwrap();
    reports.push(report.clone());

    (StatusCode::CREATED, Json(report))
```

Lock the Mutex (note `mut` — we need a mutable lock guard to push), add the report, then return it. We `.clone()` before pushing because `push` takes ownership of `report`, and we still need `report` to return it. (We could also push first and return the clone, but this reads more naturally.)

---

## The main function

```rust
#[tokio::main]
async fn main() {
```

`#[tokio::main]` is a *procedural macro* — it rewrites your `async fn main()` into a regular `fn main()` that sets up the Tokio runtime and runs your async code inside it. Without this, Rust wouldn't know how to run async code at startup.

```rust
    let state: AppState = Arc::new(Mutex::new(Vec::new()));
```

Create the shared state: an empty Vec, wrapped in a Mutex, wrapped in an Arc. This single value will be cloned (Arc clone = just incrementing a counter, not copying the data) and handed to every request handler.

```rust
    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .layer(CorsLayer::permissive())
        .with_state(state);
```

Build the router:
- `GET /api/reports` → `list_reports`
- `POST /api/reports` → `create_report`
- `.layer(CorsLayer::permissive())` — add CORS middleware so the browser frontend (on port 8080) is allowed to call this server (on port 3000) during development
- `.with_state(state)` — attach the shared state so Axum knows what to inject when a handler uses `State<AppState>`

```rust
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
```

Open a TCP socket on port 3000 (`0.0.0.0` means "all network interfaces"), then hand it to Axum to start accepting connections. `.await` suspends until the server shuts down (which is never, in normal operation).

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
  (Axum injects State, Json, etc. automatically)
        │
        ├── locks Mutex to read/write Vec<DamageReport>
        ├── builds response value
        └── returns Json<T> or (StatusCode, Json<T>)
                │
                ▼
        Axum serializes to JSON
        and sends HTTP response
```

The key Rust ideas at work here:
| Concept | Where | Why |
|---|---|---|
| `Arc<Mutex<T>>` | `AppState` | Safe shared mutable state across async tasks |
| Ownership + Clone | `report.clone()` | Can't return a reference out of a Mutex; must own the data |
| Extractors | `State(...)`, `Json(...)` | Axum reads function signatures to inject the right values |
| `async/await` | everywhere | Non-blocking request handling without manual threads |
| Destructuring | `State(state)`, `Json(req)` | Unwrap a wrapper type and bind the inner value in one step |

---

## Deep dive: `Arc<Mutex<T>>`

### What is it? (The Google Doc analogy)

**`Arc` — the shared link to the doc**

`Arc` is like the **URL to a Google Doc**. Everyone gets their own copy of the URL, but they all point to the *same underlying document*. Sharing the URL is cheap — you're not copying the whole doc. When the last person closes their tab, the doc gets cleaned up automatically.

In code terms: cloning an `Arc` just increments a counter. The actual `Vec<DamageReport>` inside exists once in memory.

**`Mutex` — the "one editor at a time" rule**

A Google Doc lets multiple people view it, but if two people edit the same sentence simultaneously you get chaos. So imagine a rule: **only one person can type at a time**. You click "Edit", make your changes, click "Done" — and the next person gets in.

That's a `Mutex`. When you call `.lock()`, you grab exclusive access. Everyone else waits. When the lock guard goes out of scope (end of the function), it releases automatically.

**Together:**

```
Arc  = the shared URL (everyone holds a reference to the same thing)
Mutex = the one-editor-at-a-time rule (safe to modify without chaos)

Arc<Mutex<Vec<DamageReport>>>
 │       │        └── the actual data (the Google Doc content)
 │       └── only one request can modify it at a time
 └── every request handler shares the same one
```

**Why you need both:**
- `Arc` alone — everyone shares it, but two requests writing at the same time corrupts data. Rust won't even compile this.
- `Mutex` alone — safe access, but only one owner, so you can't share it across requests.
- `Arc<Mutex<T>>` — shared *and* safe. The standard Rust solution.

---

### Why not just use a reference (`&`) instead?

A reference in Rust has a *lifetime* — it's only valid as long as the thing it points to is alive. The compiler enforces this at compile time.

```rust
let reports = Vec::new();
let r = &reports; // only valid while reports is alive
```

In a web server, `main()` creates the Vec and then calls `axum::serve(...)` which runs forever, spawning a new async task per request. Each task can outlive the scope where the Vec was created — or at least the compiler can't *prove* it won't. So it rejects a `&Vec` here.

**What Arc actually does in memory**

Without Arc — one owner, one pointer, freed when the owner is dropped:
```
stack (main)          heap
┌──────────┐         ┌─────────────────────┐
│  reports ├────────►│  Vec<DamageReport>  │
└──────────┘         └─────────────────────┘
```

With Arc — data lives on the heap, shared via a reference count:
```
stack (main)          heap
┌──────────┐         ┌───────────────────────────────┐
│  arc1    ├────────►│  count: 3                     │
└──────────┘         │  data: Vec<DamageReport> [...] │
                     └───────────────────────────────┘
stack (request 1)           ▲
┌──────────┐                │
│  arc2    ├────────────────┘
└──────────┘                │
stack (request 2)           │
┌──────────┐                │
│  arc3    ├────────────────┘
└──────────┘
```

Each `Arc::clone()` just increments the counter — no data is copied. When a task finishes and its Arc is dropped, the counter decrements. When it hits 0, the Vec is freed.

The key difference from `&`: the Arc *owns* the data jointly with everyone else. There's no single parent scope the data is tied to. The compiler doesn't need to reason about lifetimes at all.

| | `&T` | `Arc<T>` |
|---|---|---|
| Where does data live? | somewhere with a known lifetime | heap, owned by the Arc itself |
| How does compiler verify safety? | tracks lifetime statically | reference counting at runtime |
| Cost of sharing | free (just a pointer) | cheap (increment a counter) |
| Works across async tasks? | no — lifetime can't be proven | yes |
| When is data freed? | when the owner is dropped | when the last Arc is dropped |

`&` is a *borrow* — pointing at someone else's data. `Arc` is *shared ownership* — the data belongs to whoever holds an Arc, collectively.
