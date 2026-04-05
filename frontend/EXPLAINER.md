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

- **`shared`** — our own domain types (`CreateReportRequest`, `DamageReport`, `DamageType`, `GPSLocation`). Shared between backend and frontend so the same types are used on both sides of the wire.
- **`leptos`** with `csr` feature — the Rust UI framework. CSR = client-side rendering: the entire app runs in the browser. Must match the version used by `leptos-leaflet` — a version mismatch causes a compile error because the `Component` trait from two different versions of `leptos` is incompatible.
- **`leptos-leaflet`** — Rust components (`MapContainer`, `TileLayer`, `Marker`, `Popup`, etc.) wrapping Leaflet.js. You write Rust, Leaflet renders the map in JS under the hood.
- **`wasm-bindgen`** — the glue between Rust and the browser's JS environment. Lets Rust call browser APIs and lets JS call Rust functions. The `#[wasm_bindgen(start)]` attribute comes from this crate.
- **`gloo-net`** — a safe wrapper around the browser's `fetch()` API for making HTTP requests from WASM. Without it you'd have to call raw JS `fetch` through `wasm_bindgen` yourself.
- **`serde_json`** — serializes `CreateReportRequest` into a JSON string to send in the POST body, and deserializes the JSON array of reports returned by the backend.

---

## `Trunk.toml`

```toml
[[proxy]]
rewrite = "/api/"
backend = "http://localhost:3000/api/"
```

The frontend runs on `localhost:8080` and the backend on `localhost:3000`. Without a proxy, any request to `/api/reports` from the browser would go to `localhost:8080/api/reports` (the frontend server), not the backend. This config tells Trunk to forward any request matching `/api/` to `localhost:3000/api/` instead. No CORS issues in development — the browser only ever talks to one origin.

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
use shared::{CreateReportRequest, DamageReport, DamageType, GPSLocation};
use wasm_bindgen::prelude::*;

#[component]
fn App() -> impl IntoView {
    let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
    let damage_type = RwSignal::new("Pothole".to_string());
    let severity = RwSignal::new(5u8);
    let description = RwSignal::new(String::new());
    let reports: RwSignal<Vec<DamageReport>> = RwSignal::new(vec![]);

    spawn_local(async move {
        if let Ok(resp) = Request::get("/api/reports").send().await {
            if let Ok(data) = resp.json::<Vec<DamageReport>>().await {
                reports.set(data);
            }
        }
    });

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
            <For
                each=move || reports.get()
                key=|r| format!("{:.6},{:.6}", r.location.latitude, r.location.longitude)
                children=|r| view! {
                    <Marker position=Position::new(r.location.latitude, r.location.longitude)>
                        <Popup>
                            <p>{format!("{:?} — severity {}", r.damage_type, r.severity)}</p>
                            <p>{r.description.clone()}</p>
                        </Popup>
                    </Marker>
                }
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
                                    if let Ok(r) = Request::get("/api/reports").send().await {
                                        if let Ok(data) = r.json::<Vec<DamageReport>>().await {
                                            reports.set(data);
                                        }
                                    }
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
use shared::{CreateReportRequest, DamageReport, DamageType, GPSLocation};
use wasm_bindgen::prelude::*;
```

`spawn_local` is imported explicitly from `leptos::task` — it's not included in the prelude. `DamageReport` is imported alongside the other shared types so we can deserialize the list of reports returned by the GET endpoint. Everything else comes in via `prelude::*`.

### Reactive state

```rust
let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
let damage_type = RwSignal::new("Pothole".to_string());
let severity = RwSignal::new(5u8);
let description = RwSignal::new(String::new());
let reports: RwSignal<Vec<DamageReport>> = RwSignal::new(vec![]);
```

All five signals are declared at the top of `App`. Every piece of state the UI depends on lives here. `RwSignal<T>` is Leptos's reactive state primitive — calling `.set()` on a signal automatically re-renders any part of the `view!` that reads it via `.get()`.

`clicked_pos` is `Option<(f64, f64)>` — `None` when no pin has been dropped, `Some((lat, lng))` after a click. This drives the conditional rendering: `None` shows the placeholder text, `Some` shows the form.

`reports` holds the live list of all submitted reports. It starts as an empty `Vec`, gets populated immediately on load via a `GET /api/reports` fetch, and is refreshed again after every successful submission so the map stays up to date.

### Loading existing reports on mount

```rust
spawn_local(async move {
    if let Ok(resp) = Request::get("/api/reports").send().await {
        if let Ok(data) = resp.json::<Vec<DamageReport>>().await {
            reports.set(data);
        }
    }
});
```

This runs immediately when the component mounts — not inside any event handler, just in the component body. `spawn_local` schedules the async block on the browser's microtask queue and returns immediately so rendering isn't blocked. When the fetch completes, `reports.set(data)` triggers a re-render of the marker list.

`resp.json::<Vec<DamageReport>>()` deserializes the JSON array the backend returns. The type annotation inside the turbofish (`::<Vec<DamageReport>>`) tells `gloo-net` what to deserialize into. The `DamageReport` type comes from `shared`, so the same struct definition is used on both sides of the wire.

### Click handler

```rust
let map_events = MapEvents::new().mouse_click(move |e| {
    let latlng = e.lat_lng();
    clicked_pos.set(Some((latlng.lat(), latlng.lng())));
});
```

`MapEvents` is a builder — `.mouse_click(callback)` attaches a handler and returns the same `MapEvents` so you can chain more events. The callback receives a `MouseEvent` from Leaflet; `.lat_lng()` extracts the geographic coordinates. The `move` keyword is required because this closure crosses the WASM boundary and must own everything it captures.

### The map and markers

```rust
<MapContainer style="height: 100vh; width: 100%;" center=... zoom=13.0 events=map_events>
    <TileLayer url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png" attribution="..." />
    <For
        each=move || reports.get()
        key=|r| format!("{:.6},{:.6}", r.location.latitude, r.location.longitude)
        children=|r| view! {
            <Marker position=Position::new(r.location.latitude, r.location.longitude)>
                <Popup>
                    <p>{format!("{:?} — severity {}", r.damage_type, r.severity)}</p>
                    <p>{r.description.clone()}</p>
                </Popup>
            </Marker>
        }
    />
</MapContainer>
```

`MapContainer` creates the Leaflet map and provides a context for child components. `TileLayer` attaches itself to that context and loads OpenStreetMap imagery. The `height: 100vh` is required — Leaflet won't render into a zero-height div.

`<For>` is Leptos's reactive list renderer. It's more efficient than a plain `.map()` inside the view — it diffs the list using the `key` field and only re-renders items that actually changed, rather than rebuilding every marker on every update.

- `each=move || reports.get()` — reads the signal reactively. Whenever `reports` is updated (on load or after a submission), this re-evaluates and the marker list re-renders.
- `key=|r| format!(...)` — a unique string per report used for diffing. We use the lat/lng coordinates as the key since each report has a distinct location.
- `children=|r| view! { ... }` — for each report, renders a `<Marker>` at its location with a nested `<Popup>`. Clicking a marker on the map opens the popup showing damage type, severity, and description.

`Position::new(lat, lng)` is required for the `Marker` position prop — it expects a `Position` struct from leptos-leaflet, not a raw tuple.

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
- **`prop:value`** — sets the DOM *property* (not the HTML attribute). For inputs, the property is the live value; the attribute is the initial value. Using `prop:` keeps the input in sync with the signal in both directions.

### The submit handler

```rust
<button on:click=move |_| {
    let req = CreateReportRequest { ... };
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
                if let Ok(r) = Request::get("/api/reports").send().await {
                    if let Ok(data) = r.json::<Vec<DamageReport>>().await {
                        reports.set(data);
                    }
                }
            }
        }
    });
}>"Submit"</button>
```

**Building `CreateReportRequest`** — the `damage_type` signal holds a string (because HTML `<select>` values are strings), so we `match` it back to the `DamageType` enum. `lat` and `lng` come from the enclosing `Some((lat, lng))` pattern. The server fills in `timestamp` and `status` — we don't send those.

**`spawn_local`** — runs an async block from within a sync event handler. WASM is single-threaded, so you can't use `tokio::spawn`. `spawn_local` schedules the future on the same thread using the browser's microtask queue.

**`Request::post(...).body(...).unwrap().send().await`** — `.body()` returns a `Result` (it can fail if the body can't be set), so we `.unwrap()` before calling `.send()`. `.send()` is async and returns a `Result<Response, Error>`.

**`resp.ok()`** — true if the HTTP status is 200–299. On success, the form signals are reset to their defaults (which closes the form and clears the pin), and then a fresh `GET /api/reports` is fired. When that returns, `reports.set(data)` updates the signal, which triggers the `<For>` loop to re-render — the new report's marker appears on the map immediately without a page reload.

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
  ├── 5x RwSignal                → pin position, form fields, and reports list
  ├── spawn_local on mount       → GET /api/reports → populate reports signal
  ├── MapEvents click handler    → sets clicked_pos on map click
  ├── MapContainer + TileLayer   → renders the base map
  ├── <For> over reports         → renders a <Marker> + <Popup> per report
  ├── move || match clicked_pos  → conditional: placeholder or form
  ├── controlled inputs          → on:input + prop:value keep signals in sync
  ├── spawn_local + gloo-net     → async POST to /api/reports
  ├── re-fetch reports on submit → GET /api/reports → update reports signal
  └── #[wasm_bindgen(start)]     → browser entry point
```

| Concept | What it means |
|---|---|
| `RwSignal<T>` | Reactive state — `.set()` triggers re-renders of anything that calls `.get()` |
| `Option<T>` in signals | Models "nothing yet" vs "something" — drives conditional rendering |
| `move` closures | Required when a closure crosses the WASM boundary or outlives its scope |
| `move \|\|` in `view!` | Reactive closure — re-runs automatically when signals it reads change |
| `.into_any()` | Erases concrete view types so different `match` arms are compatible |
| `<For>` | Reactive list renderer — diffs by key, only re-renders changed items |
| `on:input` / `on:change` | Leptos event listeners; `event_target_value(&e)` extracts the value |
| `prop:value` | Sets the live DOM property (not HTML attribute) to keep inputs controlled |
| `spawn_local` | Runs async code from a sync context in single-threaded WASM |
| `resp.json::<T>()` | Deserializes the response body into type `T` using serde |
| `resp.ok()` | True if HTTP status is 200–299 |
| `Position::new(lat, lng)` | Required for Marker position — leptos-leaflet doesn't accept raw tuples |
| z-index: 1000 | Required to render the form panel above Leaflet's internal layer stack |

---

## Known limitations and future improvements

### Re-fetching all reports after submission is inefficient

After a successful POST, the app fires a second `GET /api/reports` to refresh the marker list. This works fine now but doesn't scale — with a large database it downloads every report just to add one new marker.

**Better approach:** The POST response already returns the newly created report (the backend sends it back as `201 Created` with the report body). Instead of re-fetching the whole list, just append that single report to the signal:

```rust
if let Ok(new_report) = resp.json::<DamageReport>().await {
    reports.update(|list| list.push(new_report));
}
```

No second HTTP request needed. Fast regardless of how many reports exist in the database.

**Even better for large datasets:** Only fetch reports within the current map viewport (bounding box query params on the GET endpoint), so the frontend never downloads reports the user can't see.
