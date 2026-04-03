# Patch My Path

A web app for reporting road damage (potholes, cracks, water leaks). Click on a map, fill in the details, submit. Built entirely in Rust.

Im trying to vibe code this project to test out the SOTA models!

## What it does

- Click anywhere on a map to drop a pin
- Categorize the issue (pothole, crack, water leak), set severity, add a description and optional photo
- All reports appear as markers on the map for everyone to see
- Admin panel to update the fix status of each report

## Tech stack

- **Backend:** Axum + Tokio (REST API)
- **Frontend:** Leptos (Rust → WASM, no JavaScript written by hand)
- **Map:** leptos-leaflet (Rust bindings for Leaflet.js)
- **Database:** SQLite via rusqlite
- **Hosting:** Railway

## Running locally

```bash
# One-time setup
rustup target add wasm32-unknown-unknown
cargo install trunk

# Terminal 1 — backend
cargo run -p backend

# Terminal 2 — frontend
cd frontend && trunk serve --port 8080 --proxy-backend http://localhost:3000
```

Open `http://localhost:8080`.

## Build plan

See [PLAN.md](./PLAN.md) for the full step-by-step build plan, including what gets built at each stage and the Rust concepts covered along the way.

### What's been done

- [x] Domain types defined (`DamageReport`, `DamageType`, `GPSLocation`, `FixStatus`)
- [x] JSON serialisation/deserialisation working
- [x] **Step 0** — Cargo workspace (`shared`, `backend`, `frontend` crates)
- [x] **Step 1** — Axum REST API with in-memory storage
- [x] **Step 2** — Leptos frontend skeleton (WASM, renders in browser)

### What's next

- [ ] **Step 3** — Interactive map with click-to-pin
- [ ] **Step 4** — Report submission form (full round-trip to backend)
- [ ] **Step 5** — Show all reports as map markers
- [ ] **Step 6** — SQLite persistence + Railway deployment
- [ ] **Step 7** — Image upload support
- [ ] **Step 8** — Admin panel with status updates
