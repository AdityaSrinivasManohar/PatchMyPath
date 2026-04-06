# How the app works

A complete walkthrough of the Patch My Path codebase — frontend, backend, shared types, and deployment — for someone new to Rust web development.

---

## The big picture

The project is a Cargo **workspace** with three crates:

```
patch_my_path/
├── shared/     ← domain types used by both backend and frontend
├── backend/    ← Axum HTTP server (native binary, runs on the server)
└── frontend/   ← Leptos app (compiles to WebAssembly, runs in the browser)
```

There is no JavaScript written by hand. The entire browser UI is written in Rust, compiled to **WebAssembly (WASM)** — a binary format that browsers can execute natively.

The request flow in production:

```
Browser
  │
  ├── GET /          → backend serves frontend/dist/index.html
  ├── GET /*.wasm    → backend serves the compiled WASM bundle
  └── POST /api/reports → backend handles the API request, reads/writes SQLite
```

In development, Trunk runs a local server on port 8080 and **proxies** `/api/` calls to the backend on port 3000, so you get live-reloading without CORS issues.

---

## `shared/` — the glue between backend and frontend

Both the backend (native Rust) and the frontend (WASM Rust) import the same `shared` crate. This guarantees that the types the backend serializes are exactly the types the frontend deserializes — no mismatches possible.

Key types:

- **`DamageReport`** — a full report as stored in the database, including `id: i64` and `status: FixStatus`.
- **`CreateReportRequest`** — what the frontend sends in a POST body (no `id` or `status` — the backend fills those in).
- **`UpdateStatusRequest`** — what the frontend sends in a PATCH body to change a report's status.
- **`DamageType`** — `Pothole`, `CracksOnRoad`, `WaterLeak`.
- **`FixStatus`** — `Pending`, `InProgress`, `Completed`.
- **`GPSLocation`** — `{ latitude: f64, longitude: f64 }`.

All types derive `Serialize` and `Deserialize` from serde, so they work with JSON automatically.

---

## `backend/` — the Axum server

### What it does

The backend is a single binary that:

1. Opens (or creates) a SQLite database file.
2. Exposes a REST API for reports.
3. **In production**, also serves the compiled frontend files as static assets.

### Routes

| Method | Path | Auth required | What it does |
|---|---|---|---|
| `GET` | `/api/reports` | No | Returns all reports as JSON |
| `POST` | `/api/reports` | No | Creates a new report, returns it as `201 Created` |
| `PATCH` | `/api/reports/{id}` | Yes | Updates a report's status |
| `DELETE` | `/api/reports/{id}` | Yes | Deletes a report |
| `GET` | `/api/admin/ping` | Yes | Returns 200 if the password is correct, 401 if not |
| `*` | `/*` | No | Serves the frontend static files (WASM, JS, CSS, HTML) |

### Authentication

The admin password is stored in the `ADMIN_PASSWORD` environment variable (defaults to `"admin"` for local dev). It is **never sent to the browser** — the WASM bundle can be inspected by anyone, so keeping secrets in it is insecure.

Instead, the frontend sends the password with every protected request as an HTTP header:

```
Authorization: Bearer <password>
```

The backend checks this on every protected route with a helper function:

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

The `/api/admin/ping` endpoint exists purely as a login check — the frontend hits it with the entered password; a 200 means "password correct, proceed to admin panel", a 401 means "wrong password".

### AppState

All route handlers share state via Axum's `State` extractor. The state type is:

```rust
struct AppData {
    db: Mutex<Connection>,      // the SQLite connection
    admin_password: String,     // read from ADMIN_PASSWORD env var
}
type AppState = Arc<AppData>;
```

`Arc` (atomic reference count) lets the state be shared across async tasks. `Mutex` ensures only one handler accesses the database at a time.

### Static file serving

In production the backend also serves the compiled frontend. `tower-http`'s `ServeDir` points at the `frontend/dist/` folder:

```rust
let serve_dir = ServeDir::new(&static_dir)
    .not_found_service(ServeFile::new(format!("{}/index.html", &static_dir)));
```

The `.not_found_service` fallback means any route that isn't a real file (like `/admin`) gets served `index.html` — which is correct because Leptos's client-side router handles those routes in the browser.

### Configurable via environment variables

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `3000` | Port to listen on (Railway sets this automatically) |
| `DB_PATH` | `reports.db` | Path to the SQLite file |
| `STATIC_DIR` | `frontend/dist` | Where the compiled frontend lives |
| `ADMIN_PASSWORD` | `admin` | Admin panel password |

---

## `frontend/` — the Leptos WASM app

### How Rust becomes browser code

```
src/lib.rs  (Rust)
     │
     ▼
trunk build
     │
     ├── compiles Rust → .wasm  (targeting wasm32-unknown-unknown)
     ├── runs wasm-bindgen to generate JS glue code
     ├── injects <script> tag into index.html
     └── outputs everything into frontend/dist/

Browser loads index.html
     │
     ├── downloads leaflet.css + leaflet.js from CDN
     ├── downloads styles.css
     ├── downloads frontend.wasm
     └── JS glue calls #[wasm_bindgen(start)]
              │
              ▼
         mount_to_body(App) runs
              │
              ▼
         Leptos renders the component tree into <body>
```

### `Cargo.toml`

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

- **`cdylib`** — produces the `.wasm` file. This is the correct type for WASM output — it tells Cargo to build a shared library for a foreign target.
- **`rlib`** — the normal Rust library format, kept so the workspace can reference the crate if needed.

`autobins = false` — prevents Cargo from treating `src/main.rs` (an empty stub) as a binary target.

Key dependencies:

| Crate | What it does |
|---|---|
| `shared` | Domain types — same structs used on both sides of the wire |
| `leptos` (csr) | The UI framework. CSR = client-side rendering, entire app runs in browser |
| `leptos_router` | Client-side routing — maps URLs to components without page reloads |
| `leptos-leaflet` | Rust components wrapping the Leaflet.js map library |
| `gloo-net` | Safe wrapper around the browser's `fetch()` API |
| `gloo-timers` (futures) | `TimeoutFuture` for async sleep without blocking the browser |
| `wasm-bindgen` | Glue between Rust and the browser's JS environment |
| `web-sys` | Rust bindings for browser APIs (Geolocation, etc.) |
| `serde_json` | Serializes request structs to JSON strings |

### `Trunk.toml`

```toml
[[proxy]]
rewrite = "/api/"
backend = "http://localhost:3000/api/"
```

In development, Trunk runs on port 8080. Without the proxy, any `/api/` request would go to the wrong server. This config forwards all `/api/` traffic to the backend on port 3000. The browser only ever talks to one origin, so there are no CORS issues locally.

In production, the proxy isn't used — the backend serves both the frontend files and the API from the same origin.

### `index.html`

```html
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<link data-trunk rel="css" href="styles.css" />
```

- Leaflet CSS and JS come from a CDN — the only JavaScript in the project. `leptos-leaflet` wraps it, but the underlying map rendering is still Leaflet.js.
- **`data-trunk rel="css"`** tells Trunk to copy `styles.css` into the build output and inject it. Without this attribute Trunk ignores the file.
- **No `<script>` for WASM** — Trunk injects it automatically.
- **Empty `<body>`** — Leptos fills it at runtime.

---

## `src/lib.rs` — component by component

The entire frontend application lives in this one file. It defines five components and a WASM entry point.

### Component tree

```
App (router wrapper)
 ├── Route "/"      → MapPage
 │    ├── MapContainer
 │    │    ├── TileLayer
 │    │    ├── FlyToHandler
 │    │    ├── <For> over reports → Marker + Popup per report
 │    │    └── user location Marker (conditional)
 │    ├── LocationButton (outside MapContainer)
 │    └── .panel div (form, outside MapContainer)
 │
 └── Route "/admin" → AdminPage
      ├── (password not entered) → login form
      └── (password entered)    → AdminPanel
           └── <For> over reports → table row per report
```

### `App` — the router

```rust
#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| "Page not found.">
                <Route path=path!("/") view=MapPage />
                <Route path=path!("/admin") view=AdminPage />
            </Routes>
        </Router>
    }
}
```

`leptos_router` handles client-side navigation. When the user visits `/admin`, the browser doesn't make a new HTTP request — Leptos swaps the component. This is why the backend's static file fallback must serve `index.html` for unknown paths: any deep-link needs to load the app first, then Leptos handles the route.

---

### `MapPage` — the public map

This is the main screen. It holds all the reporting state and renders the map, the location button, and the submission form.

**Signals (reactive state):**

| Signal | Type | What it tracks |
|---|---|---|
| `clicked_pos` | `RwSignal<Option<(f64,f64)>>` | Where the user last clicked; `None` = no pin |
| `damage_type` | `RwSignal<String>` | The selected damage type |
| `severity` | `RwSignal<u8>` | Slider value 1–10 |
| `description` | `RwSignal<String>` | Text area content |
| `reports` | `RwSignal<Vec<DamageReport>>` | All reports, drives marker rendering |
| `submitting` | `RwSignal<bool>` | Disables submit button while POST is in flight |
| `fly_to` | `RwSignal<Option<(f64,f64)>>` | Bridge signal to animate the map |
| `user_location` | `RwSignal<Option<(f64,f64)>>` | Current GPS position |

`RwSignal<T>` is Leptos's reactive primitive. Any `view!` closure that calls `.get()` on a signal automatically re-runs when that signal changes — this is how the UI stays in sync without manual DOM manipulation.

**On startup:**

1. `spawn_local` fires a `GET /api/reports` to populate the marker list immediately.
2. The browser's Geolocation API is called; if permission is granted, the map flies to the user's position and a blue dot appears.
3. A global `keydown` listener is registered so pressing Escape closes the form panel.

**The form panel:**

The panel sits outside `MapContainer` as a sibling `<div>`. This is required because Leaflet's layer panes use CSS `transform`, which breaks `position: fixed` positioning for any element inside them.

```rust
{move || match clicked_pos.get() {
    None => view! { <p class="panel-hint">"Click the map to drop a pin"</p> }.into_any(),
    Some((lat, lng)) => view! { /* form */ }.into_any(),
}}
```

`.into_any()` erases the concrete view type so both match arms are compatible. The `move ||` closure is reactive — Leptos re-runs it when `clicked_pos` changes.

**Submit flow:**

1. Builds a `CreateReportRequest` from current signal values.
2. Sets `submitting = true` — disables the button.
3. POSTs to `/api/reports` with a JSON body.
4. On success: resets all form signals, then re-fetches `/api/reports` to update the markers.
5. Sets `submitting = false`.

---

### `FlyToHandler` — the map bridge

This component renders nothing visible. Its only job is to bridge the `LocationButton` (outside the map) with the Leaflet map instance (only accessible from inside the map).

```rust
#[component]
fn FlyToHandler(fly_to: RwSignal<Option<(f64, f64)>>) -> impl IntoView {
    let ctx = use_leaflet_context();
    Effect::new(move |_| {
        if let Some((lat, lng)) = fly_to.get() {
            // call map.flyTo(...)
        }
    });
    view! {}
}
```

`use_leaflet_context()` only works inside children of `MapContainer`. So `FlyToHandler` lives inside the map to access the context, while `LocationButton` lives outside to render correctly as a fixed button. The `fly_to` signal connects them: `LocationButton` writes to it, `FlyToHandler` reads it via `Effect::new` and calls the Leaflet API.

**`Effect::new`** — a reactive side effect that re-runs whenever signals it reads change. Using the tracked `ctx.map()` (not `map_untracked`) means the effect also fires when the map finishes initializing, so a geolocation result that arrives before the map is ready still triggers the animation.

**`web_sys::js_sys::Object` / `Reflect::set`** — Leaflet's `flyTo` accepts `{ duration: N }` in seconds. There's no Rust struct for this; we build a raw JavaScript object. This is the WASM equivalent of `{ duration: 0.1 }` in JS.

---

### `LocationButton` — geolocation

```rust
#[component]
fn LocationButton(
    fly_to: RwSignal<Option<(f64, f64)>>,
    user_location: RwSignal<Option<(f64, f64)>>,
) -> impl IntoView { ... }
```

On click: calls the browser's `getCurrentPosition` API. On success: sets `user_location` (shows the blue dot) and sets `fly_to` (animates the map there).

**`Closure::once`** — wraps a Rust `FnOnce` into a form JavaScript can call as a callback. The `move` keyword transfers ownership of the signals into the closure.

**`success.forget()`** — by default a `Closure` is freed when it goes out of scope. But the geolocation callback fires asynchronously, after the click handler returns. `.forget()` intentionally leaks the closure to keep it alive until JS calls it.

---

### `AdminPage` — the password gate

`AdminPage` uses a single signal to decide what to render:

```rust
let auth_token: RwSignal<Option<String>> = RwSignal::new(None);
```

- `None` → show the login form.
- `Some(token)` → show `<AdminPanel token=token />`.

On form submit, the entered password is sent to `GET /api/admin/ping` with an `Authorization: Bearer <password>` header. If the response is `200 OK`, the password is stored in `auth_token` and the panel appears. If `401`, an error message is shown. The password never needs to be stored anywhere else — every subsequent request sends it fresh.

---

### `AdminPanel` — the live report table

Once authenticated, `AdminPanel` shows a table of all reports and polls for updates every 5 seconds.

**The polling loop:**

```rust
spawn_local(async move {
    loop {
        if !active.get_untracked() { break; }
        // fetch /api/reports and update signal
        if !active.get_untracked() { break; }
        TimeoutFuture::new(5_000).await;
    }
});
on_cleanup(move || active.set(false));
```

`TimeoutFuture` from `gloo-timers` is an async sleep that doesn't block the browser's event loop. The `active` signal is set to `false` by `on_cleanup` when the component unmounts — this breaks the loop cleanly without leaking the task.

**Why not `Interval`?** `gloo_timers::callback::Interval` is not `Send + Sync`, but Leptos's `on_cleanup` requires `Send + Sync + 'static`. The async loop with `TimeoutFuture` has none of those constraints.

**Status updates:** Each row has a `<select>` dropdown. On change, it fires a `PATCH /api/reports/{id}` with `Authorization: Bearer <token>` and a JSON body of `{ "status": "InProgress" }`.

**Delete:** Each row has a Delete button. On click, it fires `DELETE /api/reports/{id}`. On `204 No Content` (success), the report is removed from the local signal immediately — no re-fetch needed:

```rust
reports.update(|rs| rs.retain(|r| r.id != report_id));
```

---

## Deployment with Docker + Railway

### Why a Dockerfile is needed

Railway's default auto-detector (Railpack) only knows how to build native Rust binaries. It has no idea how to:
- Install the WASM target (`wasm32-unknown-unknown`)
- Install Trunk
- Run `trunk build --release` to compile the frontend

So we use a custom Dockerfile that handles all of this.

### The Dockerfile — two stages

**Stage 1: builder** — everything needed to compile both crates:

```dockerfile
FROM rust:1.94.1-slim AS builder

RUN apt-get install -y pkg-config libssl-dev build-essential
RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --locked   # --locked pins trunk's deps to its tested lockfile

COPY . .
RUN cargo build -p backend --release      # compiles the Axum binary
RUN cd frontend && trunk build --release  # compiles the WASM bundle → frontend/dist/
```

**Stage 2: final image** — only the runtime artifacts, no compiler toolchain:

```dockerfile
FROM debian:bookworm-slim

COPY --from=builder /app/target/release/backend /app/backend
COPY --from=builder /app/frontend/dist          /app/frontend/dist

ENV STATIC_DIR=/app/frontend/dist
ENV DB_PATH=/data/reports.db
CMD ["/app/backend"]
```

The final image is much smaller because it doesn't include Rust, Trunk, or any build tools.

### Railway setup

1. **Source** — connect your GitHub repo; set Builder to **Dockerfile** (not Railpack).
2. **Volume** — add a volume mounted at `/data` for SQLite persistence. Without it the database resets on every deploy.
3. **Variables** — set `ADMIN_PASSWORD` and `DB_PATH=/data/reports.db`. Railway sets `PORT` automatically; the backend reads it.
4. **Networking** — set the public domain to route to port `3000` (or set `PORT=3000` in Variables so it's always consistent).

---

## Concepts reference

| Concept | What it means |
|---|---|
| `RwSignal<T>` | Reactive state — `.set()` triggers re-renders of anything that calls `.get()` |
| `Effect::new` | Reactive side effect — re-runs whenever signals it reads change |
| `Option<T>` in signals | Models "nothing yet" vs "something" — drives conditional rendering |
| `move` closures | Required when a closure crosses the WASM boundary or outlives its scope |
| `move \|\|` in `view!` | Reactive closure — Leptos re-runs it when signals it reads change |
| `.into_any()` | Erases concrete view types so different `match` arms are compatible |
| `<For>` | Reactive list renderer — diffs by key, only re-renders changed items |
| `spawn_local` | Runs async code from a sync context in single-threaded WASM |
| `on_cleanup` | Runs when a component unmounts — used to cancel the polling loop |
| `TimeoutFuture::new(ms)` | Async sleep in WASM — doesn't block the browser event loop |
| `Closure::once` | Wraps a Rust `FnOnce` so JavaScript can call it as a callback |
| `success.forget()` | Keeps a Closure alive past its Rust scope so JS can call it asynchronously |
| `Reflect::set` | Sets a property on a raw JS object from Rust |
| `on:input` / `on:change` | Leptos event listeners; `event_target_value(&e)` extracts the string value |
| `prop:value` | Sets the live DOM property (not HTML attribute) to keep inputs controlled |
| `resp.json::<T>()` | Deserializes the response body into type `T` using serde |
| `resp.ok()` | True if HTTP status is 200–299 |
| `use_leaflet_context()` | Retrieves the Leaflet map instance — only works inside `MapContainer` |
| `icon_class=` | Uses a CSS div as a Leaflet marker instead of the default image |
| `Arc<AppData>` | Shared state across async Axum handlers — reference-counted, thread-safe |
| `Mutex<Connection>` | Ensures only one handler reads/writes SQLite at a time |
| Multi-stage Docker build | Stage 1 compiles everything; stage 2 copies only the output — smaller image |
