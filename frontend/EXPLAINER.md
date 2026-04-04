# How the frontend works

A file-by-file walkthrough of the frontend crate for someone new to Rust and WebAssembly.

---

## The big picture

The backend is a native binary that runs on your machine and speaks HTTP. The frontend is something completely different — it compiles to **WebAssembly (WASM)**, a binary format that browsers can execute. There is no JavaScript written by hand. Rust becomes the browser code.

The flow from code to browser:

```
src/lib.rs  (Rust)
     │
     ▼
trunk serve
     │
     ├── compiles Rust → .wasm via cargo (targeting wasm32-unknown-unknown)
     ├── runs wasm-bindgen to generate JS glue code
     ├── injects <script> tag into index.html
     └── serves everything on localhost:8080

Browser loads index.html
     │
     ├── downloads leaflet.css + leaflet.js from CDN
     ├── downloads your .wasm file
     └── JS glue calls your #[wasm_bindgen(start)] function
              │
              ▼
         mount_to_body(App) runs
              │
              ▼
         Leptos renders your component into <body>
```

---

## `Cargo.toml`

```toml
[package]
name = "frontend"
version = "0.1.0"
edition = "2024"
autobins = false
```

**`autobins = false`** — by default, Cargo treats any file at `src/main.rs` as a binary target. We have a `src/main.rs` stub (kept for workspace compatibility) but we don't want Cargo to build it as a binary. This flag disables that auto-detection so only the `[lib]` gets built.

```toml
[lib]
crate-type = ["cdylib", "rlib"]
```

- **`cdylib`** — produces the `.wasm` file. Despite the C-related name, this is the correct type for WASM output — it means "build a shared library for FFI", and WASM is treated as a foreign target.
- **`rlib`** — the normal Rust library format, kept so other Rust crates could depend on `frontend` if needed.

```toml
[dependencies]
shared = { path = "../shared" }
leptos = { version = "0.8", features = ["csr"] }
leptos-leaflet = "0.10"
wasm-bindgen = "0.2"
gloo-net = "0.6"
serde_json = "1.0.149"
```

- **`shared`** — our own domain types (`CreateReportRequest`, `DamageType`, `GPSLocation`). Shared between backend and frontend so the same types are used on both sides of the wire.
- **`leptos`** with `csr` feature — the Rust UI framework. CSR = client-side rendering: the entire app runs in the browser. Must match the version used by `leptos-leaflet` — a version mismatch causes a compile error because the `Component` trait from two different versions of `leptos` is incompatible.
- **`leptos-leaflet`** — Rust components (`MapContainer`, `TileLayer`, etc.) wrapping Leaflet.js. You write Rust, Leaflet renders the map in JS under the hood.
- **`wasm-bindgen`** — the glue between Rust and the browser's JS environment. Lets Rust call browser APIs and lets JS call Rust functions. The `#[wasm_bindgen(start)]` attribute comes from this crate.
- **`gloo-net`** — a safe wrapper around the browser's `fetch()` API for making HTTP requests from WASM. Without it you'd have to call raw JS `fetch` through `wasm_bindgen` yourself.
- **`serde_json`** — serializes `CreateReportRequest` into a JSON string to send in the POST body.

---

## `Trunk.toml`

```toml
[[proxy]]
rewrite = "/api/"
backend = "http://localhost:3000/api/"
```

The frontend runs on `localhost:8080` and the backend on `localhost:3000`. Without a proxy, a `POST /api/reports` from the browser would go to `localhost:8080/api/reports` (the frontend server), not the backend. This config tells Trunk to forward any request matching `/api/` to `localhost:3000/api/` instead. No CORS issues in development — the browser only ever talks to one origin.

---

## `index.html`

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Patch My Path</title>
    <style>* { margin: 0; padding: 0; box-sizing: border-box; }</style>
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
  </head>
  <body></body>
</html>
```

**No `<script>` tag for WASM** — Trunk injects it automatically at build time.

**Empty `<body>`** — Leptos's `mount_to_body()` fills it in at runtime.

**Leaflet from CDN** — the only JavaScript in the project. Required by `leptos-leaflet` to do the actual map rendering.

**CSS reset** — removes browser default margins so the map fills the screen edge to edge.

---

## `src/lib.rs`

This is the entire frontend application. Here's the full file:

```rust
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_leaflet::prelude::*;
use shared::{CreateReportRequest, DamageType, GPSLocation};
use wasm_bindgen::prelude::*;

#[component]
fn App() -> impl IntoView {
    let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
    let damage_type = RwSignal::new("Pothole".to_string());
    let severity = RwSignal::new(5u8);
    let description = RwSignal::new(String::new());

    let map_events = MapEvents::new().mouse_click(move |e| {
        let latlng = e.lat_lng();
        clicked_pos.set(Some((latlng.lat(), latlng.lng())));
    });

    view! {
        <MapContainer
            style="height: 100vh; width: 100%;"
            center=Position::new(51.505, -0.09)
            zoom=13.0
            events=map_events
        >
            <TileLayer
                url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
                attribution="&copy; OpenStreetMap contributors"
            />
        </MapContainer>

        <div style="position: fixed; bottom: 1rem; left: 1rem; background: white; padding: 1rem; border-radius: 4px; z-index: 1000; min-width: 260px;">
            {move || match clicked_pos.get() {
                None => view! {
                    <p>"Click the map to drop a pin"</p>
                }.into_any(),
                Some((lat, lng)) => view! {
                    <p style="margin-bottom: 0.5rem;">{format!("Lat: {:.5}, Lng: {:.5}", lat, lng)}</p>

                    <label>"Type"</label>
                    <select
                        style="display: block; width: 100%; margin-bottom: 0.5rem;"
                        on:change=move |e| damage_type.set(event_target_value(&e))
                    >
                        <option value="Pothole">"Pothole"</option>
                        <option value="CracksOnRoad">"Cracks on Road"</option>
                        <option value="WaterLeak">"Water Leak"</option>
                    </select>

                    <label>{move || format!("Severity: {}", severity.get())}</label>
                    <input
                        type="range" min="1" max="10"
                        style="display: block; width: 100%; margin-bottom: 0.5rem;"
                        prop:value=move || severity.get().to_string()
                        on:input=move |e| {
                            if let Ok(v) = event_target_value(&e).parse::<u8>() {
                                severity.set(v);
                            }
                        }
                    />

                    <label>"Description"</label>
                    <textarea
                        style="display: block; width: 100%; margin-bottom: 0.5rem;"
                        on:input=move |e| description.set(event_target_value(&e))
                        prop:value=move || description.get()
                    />

                    <button on:click=move |_| {
                        let req = CreateReportRequest {
                            damage_type: match damage_type.get().as_str() {
                                "CracksOnRoad" => DamageType::CracksOnRoad,
                                "WaterLeak" => DamageType::WaterLeak,
                                _ => DamageType::Pothole,
                            },
                            location: GPSLocation { latitude: lat, longitude: lng },
                            severity: severity.get(),
                            description: description.get(),
                            image: None,
                        };
                        spawn_local(async move {
                            let result = Request::post("/api/reports")
                                .header("Content-Type", "application/json")
                                .body(serde_json::to_string(&req).unwrap())
                                .unwrap()
                                .send()
                                .await;
                            if let Ok(resp) = result {
                                if resp.ok() {
                                    clicked_pos.set(None);
                                    damage_type.set("Pothole".to_string());
                                    severity.set(5);
                                    description.set(String::new());
                                }
                            }
                        });
                    }>"Submit"</button>
                }.into_any(),
            }}
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
```

### Imports

```rust
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_leaflet::prelude::*;
use shared::{CreateReportRequest, DamageType, GPSLocation};
use wasm_bindgen::prelude::*;
```

`spawn_local` is imported explicitly from `leptos::task` — it's not included in the prelude. Everything else comes in via `prelude::*`.

### Reactive state

```rust
let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
let damage_type = RwSignal::new("Pothole".to_string());
let severity = RwSignal::new(5u8);
let description = RwSignal::new(String::new());
```

All four signals are declared at the top of `App`. Every piece of state the UI depends on lives here. `RwSignal<T>` is Leptos's reactive state primitive — calling `.set()` on a signal automatically re-renders any part of the `view!` that reads it via `.get()`.

`clicked_pos` is `Option<(f64, f64)>` — `None` when no pin has been dropped, `Some((lat, lng))` after a click. This drives the conditional rendering: `None` shows the placeholder text, `Some` shows the form.

### Click handler

```rust
let map_events = MapEvents::new().mouse_click(move |e| {
    let latlng = e.lat_lng();
    clicked_pos.set(Some((latlng.lat(), latlng.lng())));
});
```

`MapEvents` is a builder — `.mouse_click(callback)` attaches a handler and returns the same `MapEvents` so you can chain more events. The callback receives a `MouseEvent` from Leaflet; `.lat_lng()` extracts the geographic coordinates. The `move` keyword is required because this closure crosses the WASM boundary and must own everything it captures.

### The map

```rust
<MapContainer style="height: 100vh; width: 100%;" center=... zoom=13.0 events=map_events>
    <TileLayer url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png" attribution="..." />
</MapContainer>
```

`MapContainer` creates the Leaflet map and provides a context for child components. `TileLayer` attaches itself to that context and loads OpenStreetMap imagery. The `height: 100vh` is required — Leaflet won't render into a zero-height div.

### Conditional form rendering

```rust
{move || match clicked_pos.get() {
    None => view! { <p>"Click the map to drop a pin"</p> }.into_any(),
    Some((lat, lng)) => view! { ... }.into_any(),
}}
```

The `move ||` closure is a reactive block — Leptos re-runs it whenever `clicked_pos` changes. The two `match` arms return different view types, so `.into_any()` erases the concrete type to make them compatible. `lat` and `lng` are captured from the `Some` pattern and used directly inside the form (in the submit handler's `GPSLocation` construction).

### Controlled inputs

```rust
on:change=move |e| damage_type.set(event_target_value(&e))
```

```rust
prop:value=move || severity.get().to_string()
on:input=move |e| {
    if let Ok(v) = event_target_value(&e).parse::<u8>() {
        severity.set(v);
    }
}
```

- **`on:change` / `on:input`** — Leptos event listeners. `event_target_value(&e)` is a Leptos helper that pulls the current string value out of the event target element.
- **`prop:value`** — sets the DOM *property* (not the HTML attribute). For inputs, `property` is the live value; `attribute` is the initial value. Using `prop:` keeps the input in sync with the signal in both directions.

### The submit handler

```rust
<button on:click=move |_| {
    let req = CreateReportRequest {
        damage_type: match damage_type.get().as_str() {
            "CracksOnRoad" => DamageType::CracksOnRoad,
            "WaterLeak" => DamageType::WaterLeak,
            _ => DamageType::Pothole,
        },
        location: GPSLocation { latitude: lat, longitude: lng },
        severity: severity.get(),
        description: description.get(),
        image: None,
    };
    spawn_local(async move {
        let result = Request::post("/api/reports")
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&req).unwrap())
            .unwrap()
            .send()
            .await;
        if let Ok(resp) = result {
            if resp.ok() {
                clicked_pos.set(None);
                damage_type.set("Pothole".to_string());
                severity.set(5);
                description.set(String::new());
            }
        }
    });
}>"Submit"</button>
```

**Building `CreateReportRequest`** — the `damage_type` signal holds a string (because HTML `<select>` values are strings), so we `match` it back to the `DamageType` enum. `lat` and `lng` come from the enclosing `Some((lat, lng))` pattern. The server fills in `timestamp` and `status` — we don't send those.

**`spawn_local`** — runs an async block from within a sync event handler. WASM is single-threaded, so you can't use `tokio::spawn`. `spawn_local` schedules the future on the same thread using the browser's microtask queue.

**`Request::post(...).body(...).unwrap().send().await`** — `.body()` returns a `Result` (it can fail if the body can't be set), so we `.unwrap()` before calling `.send()`. `.send()` is async and returns a `Result<Response, Error>`.

**`resp.ok()`** — true if the HTTP status is 200–299. On success, all four signals are reset to their defaults, which triggers Leptos to re-render: `clicked_pos` becomes `None`, the form disappears, and the placeholder text returns.

### Entry point

```rust
#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
```

`#[wasm_bindgen(start)]` marks this as the WASM entry point — the browser calls it automatically when the `.wasm` module loads. `pub` is required so wasm-bindgen can export it. `mount_to_body(App)` mounts the component into `<body>`.

---

## Why `src/main.rs` still exists

```rust
fn main() {}
```

An empty stub. Without it, certain Cargo tooling expects a binary entry point. Trunk ignores it because of `autobins = false` — the real entry point is in `lib.rs`.

---

## The full picture

```
Cargo.toml
  ├── autobins = false           → ignore src/main.rs as a binary
  ├── crate-type = ["cdylib"]    → produce a .wasm file
  └── leptos + leptos-leaflet    → must share the same leptos version

Trunk.toml
  └── proxy /api/ → localhost:3000  → forward API calls to the backend

index.html
  ├── Leaflet CSS + JS from CDN  → map rendering
  └── Trunk injects WASM script  → your Rust code

src/lib.rs
  ├── 4x RwSignal                → all reactive state (pin, form fields)
  ├── MapEvents click handler    → sets clicked_pos on map click
  ├── MapContainer + TileLayer   → renders the map
  ├── move || match clicked_pos  → conditional: placeholder or form
  ├── controlled inputs          → on:input + prop:value keep signals in sync
  ├── spawn_local + gloo-net     → async POST to /api/reports
  ├── reset signals on success   → clears form and pin after submit
  └── #[wasm_bindgen(start)]     → browser entry point
```

| Concept | What it means |
|---|---|
| `RwSignal<T>` | Reactive state — `.set()` triggers re-renders of anything that calls `.get()` |
| `Option<T>` in signals | Models "nothing yet" vs "something" — drives conditional rendering |
| `move` closures | Required when a closure crosses the WASM boundary or outlives its scope |
| `move \|\|` in `view!` | Reactive closure — re-runs automatically when signals it reads change |
| `.into_any()` | Erases concrete view types so different `match` arms are compatible |
| `on:input` / `on:change` | Leptos event listeners; `event_target_value(&e)` extracts the value |
| `prop:value` | Sets the live DOM property (not HTML attribute) to keep inputs controlled |
| `spawn_local` | Runs async code from a sync context in single-threaded WASM |
| `resp.ok()` | True if HTTP status is 200–299 |
| z-index: 1000 | Required to render above Leaflet's internal layer stack |
