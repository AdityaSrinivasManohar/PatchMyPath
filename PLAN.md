# Full-Stack Rust Road Damage Reporter — Build Plan

## Stack

| Layer | Choice | Why |
|---|---|---|
| Backend | **Axum** + Tokio | Async-native, no magic macros, huge ecosystem |
| Frontend | **Leptos 0.7 (CSR)** | Fine-grained signals map well to Rust ownership; best learner docs |
| Map | **leptos-leaflet** | Rust components wrapping Leaflet.js; zero JS written by you |
| HTTP client (WASM) | **gloo-net** | Safe `fetch()` wrapper for WASM |
| Build (frontend) | **Trunk** | Handles WASM compilation + dev server proxy |
| Storage (phase 1) | `Arc<Mutex<Vec<DamageReport>>>` | Teaches concurrency primitives first |
| Storage (phase 2) | **rusqlite** (bundled) | Synchronous, no extra server, natural upgrade |

**On "pure Rust":** Leaflet.js loads from a CDN `<script>` tag in `index.html` — a library
asset, not authored JS. All map interaction is Rust code via `leptos-leaflet` (wasm_bindgen
bindings). You write zero `.js` files.

---

## Final Project Structure

```
patch_my_path/
├── Cargo.toml               ← [workspace]
├── shared/src/lib.rs        ← all domain types — single source of truth
├── backend/src/
│   ├── main.rs              ← Router, AppState, tokio::main
│   ├── handlers.rs          ← list/create/update/upload handlers
│   ├── db.rs                ← rusqlite helpers (Step 6)
│   └── error.rs             ← AppError + IntoResponse (Step 6)
└── frontend/src/
    ├── main.rs              ← mount_to_body(App)
    ├── app.rs               ← App + Router
    ├── pages/map_page.rs    ← map, click handler, form panel, markers
    ├── pages/admin_page.rs  ← report table + status dropdown (Step 8)
    ├── components/          ← ReportForm, ReportMarker
    └── api.rs               ← gloo-net fetch functions
```

---

## Steps

### Step 0 — Cargo Workspace
Convert the single-crate project into a workspace with three crates: `shared`, `backend`, `frontend`.

- Move `DamageReport`, `DamageType`, `GPSLocation`, `FixStatus` to `shared/src/lib.rs` (add `pub`)
- Move `main()` to `backend/src/main.rs`
- Both crates depend on `shared = { path = "../shared" }`

**Rust concepts:** workspaces, `pub` visibility, path dependencies, crate boundaries — the compiler will flag every place you forget `use shared::DamageReport`.

**Verify:** `cargo run -p backend` prints the same JSON as before.

---

### Step 1 — Axum REST API (in-memory storage)
Build a real HTTP server with two endpoints.

- `AppState = Arc<Mutex<Vec<DamageReport>>>`
- `GET  /api/reports` → `Json<Vec<DamageReport>>`
- `POST /api/reports` → accepts `Json<CreateReportRequest>` (user-supplied fields only); server fills in `timestamp` and `status: FixStatus::Pending`
- Add `tower-http` `CorsLayer::permissive()` for dev

`CreateReportRequest` lives in `shared` — this teaches why you don't send the full struct over the wire (the server owns `timestamp` and initial `status`).

**Rust concepts:** `async/await`, `tokio::main`, `Arc<Mutex<T>>`, Axum `State<T>` extractor, `Json<T>`, why `Vec<DamageReport>` needs `Clone` when returned from a locked handler.

**Verify:** `curl localhost:3000/api/reports` → `[]`. POST a report → appears in the list.

---

### Step 2 — Leptos Frontend Skeleton
One-time tooling setup, then a minimal WASM page.

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

- `frontend/Cargo.toml` with `crate-type = ["cdylib", "rlib"]` and `leptos = { features = ["csr"] }`
- `frontend/index.html` — Trunk entry point + Leaflet CDN `<link>` and `<script>` tags
- `frontend/src/main.rs` — a single `#[component]` rendering a title

**Rust concepts:** `crate-type = ["cdylib"]` (what a WASM binary is), Leptos `#[component]` and `view!` macro, `Signal<T>`, `trunk serve` workflow.

**Verify:** `trunk serve` → page loads in browser with a title.

---

### Step 3 — Interactive Map + Click Handler
Render a Leaflet map and capture click coordinates.

- `<MapContainer>` + `<TileLayer url="openstreetmap">` via `leptos-leaflet`
- `RwSignal<Option<(f64, f64)>>` stores the clicked location
- Map click event → updates the signal → coordinates shown below the map

**Rust concepts:** `RwSignal<T>`, `create_effect`, `wasm_bindgen::Closure` (why closures crossing the WASM boundary must be `move` and `'static`), `Option<T>` in views.

**Verify:** Clicking the map shows lat/lng below it.

---

### Step 4 — Report Submission Form
Full round-trip: click map → fill form → POST to backend.

- Form with `DamageType` select, severity slider (1–10), description textarea
- `spawn_local(async { gloo_net::http::Request::post("/api/reports")... })` on submit
- On success: clear form, re-fetch report list

**Rust concepts:** `spawn_local` (async inside sync components), `gloo-net`, `serde_json::to_string` in WASM, controlled inputs with `on:input=move |e| set_description(event_target_value(&e))`.

**Verify:** Click map → fill form → submit → `curl /api/reports` shows the report.

---

### Step 5 — Show Reports as Map Markers
Fetch all reports on load and render each as a pin with a popup.

- `create_resource(|| (), fetch_reports)` + `<Suspense fallback=|| "Loading...">`
- Each report → `<Marker position=(lat, lng)><Popup>damage type, severity, description</Popup></Marker>`
- `impl Display for DamageType` to use in popup text

**Rust concepts:** `create_resource`, `<Suspense>`, `collect_view()`, `impl Display for` an enum.

**Verify:** Submitted reports appear as map pins; clicking a pin shows the details.

---

### Step 6 — SQLite Persistence
Replace in-memory Vec with a SQLite database. No frontend changes needed.

Add `rusqlite = { version = "0.32", features = ["bundled"] }` to backend.

- `backend/src/db.rs`: `open_db()`, `insert_report()`, `list_reports()`
- `AppState` changes from `Arc<Mutex<Vec<DamageReport>>>` to `Arc<Mutex<rusqlite::Connection>>` — same pattern, persistent data
- `backend/src/error.rs`: `AppError` enum + `impl From<rusqlite::Error> for AppError` → enables `?` in handlers

**Rust concepts:** `From` trait, `?` operator across error types, why `rusqlite::Connection` is `!Send` and why `Mutex<Connection>` fixes that.

**Verify:** Reports survive restarting the backend. `damage_reports.db` appears on disk.

---

### Step 7 — Image Upload
Optional photo attachment on a report.

- `POST /api/reports/upload` — multipart form data, saves file to `./uploads/{uuid}.jpg`, returns the path
- `frontend`: `<input type="file">` → upload first, include the returned path in the report body
- `DamageReport.image: Option<String>` is already in the struct — just needs to be wired up

**Rust concepts:** `tokio::fs::write` (async file I/O), `uuid::Uuid::new_v4()`, multipart vs JSON Axum extractors, `Option<String>` in practice.

**Verify:** Submitted report with photo → path in DB → file in `uploads/`.

---

### Step 8 — Admin Panel + Status Updates
A second page for viewing all reports and updating their `FixStatus`.

- Add `id: i64` to `DamageReport` (SQLite ROWID) — the compiler will flag every place that constructs a `DamageReport` without the new field
- `PATCH /api/reports/{id}` endpoint
- New `/admin` route: table of all reports with a `<select>` for `FixStatus`
- Add `leptos_router` for client-side navigation between map and admin pages

**Rust concepts:** `leptos_router`, HTTP `PATCH` semantics, `#[derive(PartialEq)]` on enums for view comparison, how struct evolution forces you to fix every construction site at compile time.

**Verify:** Change status in admin panel → updated in DB → reflected on map page after refresh.

---

## Development Workflow

```bash
# Terminal 1 — backend
cargo run -p backend

# Terminal 2 — frontend (proxies /api/* to localhost:3000)
cd frontend && trunk serve --port 8080 --proxy-backend http://localhost:3000
```

The `--proxy-backend` flag means no CORS config needed in development.

---

## Deployment: Railway

**Cost:** $5/month (hobby plan). Persistent volumes supported — SQLite file survives deploys.

### How it works

Railway builds from a `Dockerfile` in the repo root. The build has two stages:

1. **Build stage:** Compile the WASM frontend (`trunk build --release`) then compile the Axum backend binary
2. **Runtime stage:** Minimal image containing just the binary, the `dist/` folder, and a mount point for the SQLite volume

Axum serves the WASM static files directly using `tower-http::services::ServeDir`, so there is no separate frontend hosting — one Railway service runs everything.

### What to add to the project (done at Step 6 alongside SQLite)

- **`Dockerfile`** (multi-stage) at the repo root
- **`railway.toml`** — tells Railway which port to expose and where to mount the persistent volume
- **Environment variable** `DATABASE_PATH` — backend reads the SQLite path from this so it points to the Railway volume in production and `./damage_reports.db` locally

### Deploy steps (one-time setup)

```bash
# Install Railway CLI
npm install -g @railway/cli   # or brew install railway

# Login and link
railway login
railway init                  # creates a new project
railway volume add            # attach a persistent volume (for SQLite)

# Deploy
railway up
```

After the first deploy, subsequent deploys are just `railway up` (or auto-deploy on git push if you link the GitHub repo in the Railway dashboard).

### Environment variables to set in Railway dashboard

| Variable | Value |
|---|---|
| `DATABASE_PATH` | `/data/damage_reports.db` (path on the mounted volume) |
| `PORT` | `3000` (Railway injects this automatically) |

### Image upload caveat

Uploaded images saved to `./uploads/` will also need to be on the persistent volume (`/data/uploads/`) in production, or moved to an object store (e.g. Cloudflare R2, free tier) if you want them to survive re-deploys. This is a Step 7 decision.

---

## Rust Concepts by Step

| Step | Concepts |
|---|---|
| 0 | Workspaces, `pub`, path dependencies |
| 1 | `async/await`, `Arc<Mutex<T>>`, Axum extractors |
| 2 | `cdylib`, `#[component]`, `view!`, `Signal<T>`, Trunk |
| 3 | `RwSignal`, `create_effect`, `wasm_bindgen::Closure`, `move` closures |
| 4 | `spawn_local`, `gloo-net`, serde over the wire |
| 5 | `create_resource`, `Suspense`, `impl Display` |
| 6 | `From` trait, `?` operator, `!Send`, SQLite |
| 7 | Async file I/O, `uuid`, multipart, `Option` in practice |
| 8 | Client routing, `PATCH`, struct evolution, compiler guidance |
