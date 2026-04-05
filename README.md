<p align="center">
  <img src="patchmypath.svg" alt="Patch My Path" width="500" />
</p>

<p align="center">
  A community road-damage reporting tool. Click the map, describe the issue, submit. Built entirely in Rust.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/frontend-WASM%20%2F%20Leptos-blueviolet?style=flat-square" alt="Leptos" />
  <img src="https://img.shields.io/badge/database-SQLite-blue?style=flat-square" alt="SQLite" />
  <img src="https://img.shields.io/badge/hosting-Railway-black?style=flat-square" alt="Railway" />
</p>

---

## What it does

- Click anywhere on a map to drop a pin
- Categorize the issue (pothole, crack, water leak), set severity, add a description
- All reports appear as markers on the map for everyone to see
- Admin panel to update fix status or delete reports

## Tech stack

| Layer | Technology |
|---|---|
| Backend | Axum + Tokio (REST API) |
| Frontend | Leptos (Rust → WASM, zero hand-written JS) |
| Map | leptos-leaflet (Rust bindings for Leaflet.js) |
| Database | SQLite via rusqlite (bundled, no system install needed) |
| Hosting | Railway |

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

Open `http://localhost:8080`. Admin panel at `http://localhost:8080/admin` (default password: `admin`).

## Build plan

See [PLAN.md](./PLAN.md) for the full step-by-step build plan.

### Done

- [x] Domain types (`DamageReport`, `DamageType`, `GPSLocation`, `FixStatus`)
- [x] **Step 0** — Cargo workspace (`shared`, `backend`, `frontend` crates)
- [x] **Step 1** — Axum REST API with in-memory storage
- [x] **Step 2** — Leptos frontend skeleton (WASM, renders in browser)
- [x] **Step 3** — Interactive map with click-to-pin
- [x] **Step 4** — Report submission form (full round-trip to backend)
- [x] **Step 5** — Show all reports as map markers
- [x] **Step 6** — SQLite persistence
- [x] **Step 7** — GUI polish (custom markers, location button, styled panel, escape to close)
- [x] **Step 8** — Admin panel (password gate, report table, status updates, delete)

### Next

- [ ] **Step 9** — Image upload support
- [ ] **Step 10** — Railway deployment
