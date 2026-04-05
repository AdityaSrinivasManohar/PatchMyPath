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
     ├── downloads styles.css
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
leaflet = "0.5"
web-sys = { version = "0.3", features = ["Window", "Navigator", "Geolocation", "Position", "Coordinates"] }
```

- **`shared`** — our own domain types (`CreateReportRequest`, `DamageReport`, `DamageType`, `GPSLocation`). Shared between backend and frontend so the same types are used on both sides of the wire.
- **`leptos`** with `csr` feature — the Rust UI framework. CSR = client-side rendering: the entire app runs in the browser. Must match the version used by `leptos-leaflet` — a version mismatch causes a compile error because the `Component` trait from two different versions of `leptos` is incompatible.
- **`leptos-leaflet`** — Rust components (`MapContainer`, `TileLayer`, `Marker`, `Popup`, etc.) wrapping Leaflet.js. You write Rust, Leaflet renders the map in JS under the hood.
- **`wasm-bindgen`** — the glue between Rust and the browser's JS environment. Lets Rust call browser APIs and lets JS call Rust functions. The `#[wasm_bindgen(start)]` attribute and `Closure` type come from this crate.
- **`gloo-net`** — a safe wrapper around the browser's `fetch()` API for making HTTP requests from WASM.
- **`serde_json`** — serializes `CreateReportRequest` into a JSON string to send in the POST body.
- **`leaflet`** — the typed Rust bindings for Leaflet's JavaScript classes (`LatLng`, `Map`). Used directly when calling imperative map methods like `fly_to_with_zoom_and_options`.
- **`web-sys`** — Rust bindings for browser Web APIs. Each API you use must be listed as a feature flag in Cargo.toml — here we enable `Geolocation`, `Position`, `Coordinates`, etc. Without listing them, the types don't exist at compile time.

---

## `Trunk.toml`

```toml
[[proxy]]
rewrite = "/api/"
backend = "http://localhost:3000/api/"
```

The frontend runs on `localhost:8080` and the backend on `localhost:3000`. Without a proxy, any request to `/api/reports` would go to the frontend server. This config tells Trunk to forward all `/api/` traffic to the backend. No CORS issues in development — the browser only ever talks to one origin.

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
    <link data-trunk rel="css" href="styles.css" />
  </head>
  <body></body>
</html>
```

**`data-trunk rel="css"`** — tells Trunk to copy `styles.css` into the build output and inject it. Without the `data-trunk` attribute, Trunk ignores the file and the browser never receives it.

**No `<script>` tag for WASM** — Trunk injects it automatically at build time.

**Empty `<body>`** — Leptos's `mount_to_body()` fills it in at runtime.

**Leaflet from CDN** — the only JavaScript in the project. Required by `leptos-leaflet` to do the actual map rendering.

---

## `styles.css`

All visual styles live here. No inline `style=` attributes appear in `lib.rs` — every element uses a CSS class instead. The key classes:

- **`.panel`** — the fixed bottom-left card: white background, rounded corners, drop shadow.
- **`.input`** — shared by `<select>` and `<textarea>`: border, padding, focus ring in indigo.
- **`.range`** — the severity slider with `accent-color: #6366f1` to tint the thumb and track indigo.
- **`.btn`** — indigo submit button with hover (darker indigo) and disabled (faded indigo) states.
- **`.loc-btn`** — the fixed bottom-right location button: small white square, indigo icon, hover tint.
- **`.marker-dot`** — custom report marker: small indigo circle with a white border.
- **`.user-dot`** — current location marker: blue circle with a white border and a blue outer ring, visually distinct from report markers.
- **`.leaflet-popup-content`** — overrides Leaflet's default popup typography with `system-ui` font and cleaner spacing.

---

## `src/lib.rs`

This is the entire frontend application. It defines three components: `FlyToHandler`, `LocationButton`, and `App`.

### Imports

```rust
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_leaflet::prelude::*;
use shared::{CreateReportRequest, DamageReport, DamageType, GPSLocation};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
```

`spawn_local` is imported explicitly from `leptos::task` — it's not in the prelude. `Closure` is from `wasm_bindgen::closure` and is used to pass Rust callbacks to JavaScript APIs. `JsCast` provides the `.unchecked_ref::<T>()` method for casting JS values to specific types.

---

### `FlyToHandler` component

```rust
#[component]
fn FlyToHandler(fly_to: RwSignal<Option<(f64, f64)>>) -> impl IntoView {
    let ctx = use_leaflet_context();
    Effect::new(move |_| {
        if let Some((lat, lng)) = fly_to.get() {
            if let Some(ref ctx) = ctx {
                if let Some(map) = ctx.map() {
                    let latlng = leaflet::LatLng::new(lat, lng);
                    let options = web_sys::js_sys::Object::new();
                    let _ = web_sys::js_sys::Reflect::set(&options, &"duration".into(), &JsValue::from_f64(3.0));
                    leaflet::Map::fly_to_with_zoom_and_options(&map, &latlng, 15.0, &options);
                }
            }
        }
    });
    view! {}
}
```

This component renders nothing visible (`view! {}`). Its only job is to bridge the location button (which lives outside the map) with the Leaflet map object (which is only accessible from inside the map).

**Why it needs to be inside `MapContainer`:** `use_leaflet_context()` retrieves the `LeafletMapContext` that `MapContainer` places into the Leptos context system. Only components rendered as children of `MapContainer` in the Leptos component tree have access to it. The `LocationButton`, which renders as a sibling of `MapContainer`, cannot call `use_leaflet_context()` — so `FlyToHandler` acts as a bridge.

**Why the `LocationButton` can't be inside `MapContainer`:** Leaflet's layer panes use CSS `transform`. Any `position: fixed` element inside a transformed ancestor positions itself relative to that ancestor instead of the viewport — meaning the button would render off-screen inside the map tile panes. So `LocationButton` must live outside `MapContainer` in the DOM.

**The signal-based bridge:** `fly_to: RwSignal<Option<(f64, f64)>>` is defined in `App` and passed to both components. `LocationButton` writes to it; `FlyToHandler` reads it via `Effect::new`. When the signal changes, the effect re-runs and calls `fly_to_with_zoom_and_options` on the Leaflet map.

**`ctx.map()` (tracked) vs `ctx.map_untracked()`:** Using the tracked version means the effect also re-runs when the map finishes initializing. This matters on startup: if geolocation resolves before the Leaflet map is ready, the fly-to still fires once the map becomes available.

**`web_sys::js_sys::Object` and `Reflect::set`:** Leaflet's `flyTo` accepts an options object `{ duration: N }` where `duration` is in seconds. There's no Rust struct for this — we have to build a raw JavaScript object. `js_sys::Object::new()` creates an empty `{}`, and `Reflect::set` sets a key on it. This is the WASM equivalent of writing `{ duration: 3.0 }` in JavaScript.

---

### `LocationButton` component

```rust
#[component]
fn LocationButton(
    fly_to: RwSignal<Option<(f64, f64)>>,
    user_location: RwSignal<Option<(f64, f64)>>,
) -> impl IntoView {
    let on_click = move |_| {
        let window = web_sys::window().unwrap();
        let geo = window.navigator().geolocation().unwrap();

        let success = Closure::once(move |pos: web_sys::Position| {
            let coords = pos.coords();
            let lat = coords.latitude();
            let lng = coords.longitude();
            user_location.set(Some((lat, lng)));
            fly_to.set(Some((lat, lng)));
        });

        let _ = geo.get_current_position(success.as_ref().unchecked_ref());
        success.forget();
    };

    view! {
        <button class="loc-btn" title="Go to my location" on:click=on_click>
            "◎"
        </button>
    }
}
```

**`web_sys::window().navigator().geolocation()`** — accesses the browser's Geolocation API through the chain of `window → navigator → geolocation`. Each step returns a `Result` or `Option`; we `.unwrap()` here for simplicity.

**`Closure::once`** — creates a Rust closure that can be called exactly once from JavaScript. The browser's `getCurrentPosition` takes a JS function as its success callback; `Closure::once` wraps a Rust `FnOnce` into a form JS can call. The `move` keyword transfers ownership of `user_location` and `fly_to` into the closure.

**`success.as_ref().unchecked_ref::<js_sys::Function>()`** — `Closure` is a WASM value, not a Rust reference. `.as_ref()` gives a `&JsValue`, and `.unchecked_ref()` casts it to `&Function` without a runtime check. This is the standard pattern for passing Rust closures to Web APIs.

**`success.forget()`** — by default, when a `Closure` is dropped (goes out of scope), its memory is freed. But the browser holds the callback and calls it asynchronously — after `on_click` has returned and `success` would normally drop. `.forget()` leaks the closure intentionally, keeping it alive until JS calls it. Without this the callback fires on freed memory and crashes.

On success, two things happen: `user_location.set(...)` makes a blue dot marker appear at the location, and `fly_to.set(...)` triggers `FlyToHandler` to animate the map there.

---

### `App` component — signals

```rust
let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
let damage_type = RwSignal::new("Pothole".to_string());
let severity = RwSignal::new(5u8);
let description = RwSignal::new(String::new());
let reports: RwSignal<Vec<DamageReport>> = RwSignal::new(vec![]);
let submitting = RwSignal::new(false);
let fly_to: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
let user_location: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
```

All reactive state is declared at the top of `App`. `RwSignal<T>` is Leptos's reactive primitive — `.set()` triggers re-renders of any part of the view that reads it via `.get()`.

- `clicked_pos` — `None` shows the placeholder text, `Some((lat, lng))` shows the form.
- `reports` — the live list fetched from the backend; drives the `<For>` marker loop.
- `submitting` — disables the button and swaps its label while the POST is in flight.
- `fly_to` — the bridge signal between `LocationButton` and `FlyToHandler`.
- `user_location` — when `Some`, renders a blue dot marker at the current location.

---

### `App` component — startup effects

```rust
spawn_local(async move {
    if let Ok(resp) = Request::get("/api/reports").send().await {
        if let Ok(data) = resp.json::<Vec<DamageReport>>().await {
            reports.set(data);
        }
    }
});
```

Fires immediately on mount. Fetches all existing reports and populates the `reports` signal so markers appear without a user action.

```rust
if let Some(window) = web_sys::window() {
    if let Ok(geo) = window.navigator().geolocation() {
        let success = Closure::once(move |pos: web_sys::Position| {
            let coords = pos.coords();
            user_location.set(Some((lat, lng)));
            fly_to.set(Some((lat, lng)));
        });
        let _ = geo.get_current_position(success.as_ref().unchecked_ref());
        success.forget();
    }
}
```

Also fires immediately on mount. Requests geolocation permission and, if granted, places the blue dot and flies the map to the user's location automatically. The map's default center is the US at zoom 4 — if geolocation is denied or unavailable, that's what the user sees.

```rust
window_event_listener(leptos::ev::keydown, move |e| {
    if e.key() == "Escape" {
        clicked_pos.set(None);
    }
});
```

Registers a global `keydown` listener that resets `clicked_pos` to `None` on Escape, closing the form panel. `window_event_listener` ties the listener's lifetime to the component — it's automatically removed when `App` unmounts.

---

### `App` component — the map

```rust
<MapContainer
    style="height: 100vh; width: 100%;"
    center=Position::new(39.5, -98.35)
    zoom=4.0
    events=map_events
>
    <TileLayer url="https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png" ... />
    <FlyToHandler fly_to=fly_to />
    <For each=move || reports.get() key=... children=|r| view! {
        <Marker position=... icon_class="marker-dot">
            <Popup>...</Popup>
        </Marker>
    } />
    {move || user_location.get().map(|(lat, lng)| view! {
        <Marker position=Position::new(lat, lng) icon_class="user-dot" />
    })}
</MapContainer>
```

**CartoDB Voyager tiles** — a modern, colorful map style. Free, no API key. The `{r}` in the URL is a Leaflet placeholder for retina/HiDPI support.

**`style=` on `MapContainer`** — unlike regular HTML elements, `MapContainer` is a leptos-leaflet component that doesn't forward a `class=` prop to its inner div. The dimensions must be set via inline `style=`.

**`<FlyToHandler>`** — renders nothing visible, only its reactive effect runs. Placed inside `MapContainer` to have access to the map context.

**`<For>`** — Leptos's reactive list renderer. More efficient than `.map()` — it diffs by key and only re-renders changed items. `key=|r| format!("{:.6},{:.6}", ...)` uses the lat/lng as a unique key.

**`icon_class="marker-dot"`** — uses a CSS div-based icon instead of Leaflet's default blue teardrop. The class `.marker-dot` is defined in `styles.css`.

**User location marker** — rendered conditionally from `user_location`. `move || user_location.get().map(...)` is a reactive closure: it re-runs when the signal changes. When `user_location` is `None` it renders nothing; when `Some` it renders a `<Marker>` with the `.user-dot` class (a blue ring that's visually distinct from report markers).

---

### `App` component — the panel

The form panel sits outside `MapContainer` as a sibling `<div>`. It's rendered over the map via `position: fixed; z-index: 1000` in CSS.

**Why outside `MapContainer`:** Leaflet's layer panes use CSS `transform`, which turns `position: fixed` children into `position: fixed` relative to the pane rather than the viewport. Elements placed inside `MapContainer` can't reliably use fixed positioning.

```rust
{move || match clicked_pos.get() {
    None => view! { <p class="panel-hint">"Click the map to drop a pin"</p> }.into_any(),
    Some((lat, lng)) => view! { ... }.into_any(),
}}
```

The `move ||` closure is a reactive block — Leptos re-runs it when `clicked_pos` changes. `.into_any()` erases the concrete view type so both match arms are compatible.

### Controlled inputs

```rust
prop:value=move || severity.get().to_string()
on:input=move |e| { severity.set(event_target_value(&e).parse::<u8>().unwrap_or(5)); }
```

`prop:value` sets the live DOM property (not the HTML attribute) to keep the input in sync with the signal. `on:input` reads back the new value on each keystroke. `event_target_value(&e)` is a Leptos helper that extracts the string value from the event's target element.

### The submit handler

The submit button's click handler:
1. Builds a `CreateReportRequest` from the current signal values
2. Sets `submitting = true` — disables the button and shows "Submitting..."
3. POSTs to `/api/reports` via `gloo-net`
4. On `resp.ok()` (2xx): resets all form signals, then re-fetches `/api/reports` to refresh the marker list
5. Sets `submitting = false` regardless of success or failure

---

### Entry point

```rust
#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
```

`#[wasm_bindgen(start)]` marks this as the WASM entry point. The browser calls it automatically when the `.wasm` module loads. `mount_to_body(App)` renders the component into `<body>`.

---

## Why `src/main.rs` still exists

```rust
fn main() {}
```

An empty stub. Without it, certain Cargo tooling expects a binary entry point. Trunk ignores it because of `autobins = false`.

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
  ├── data-trunk rel="css"       → tells Trunk to bundle styles.css
  └── Trunk injects WASM script  → your Rust code

styles.css
  └── all visual styles (panel, inputs, button, markers, popups)

src/lib.rs
  ├── FlyToHandler               → inside MapContainer, bridges fly_to signal → map.flyTo()
  ├── LocationButton             → outside MapContainer, fixed bottom-right button
  ├── 8x RwSignal                → all reactive state
  ├── spawn_local on mount       → GET /api/reports → populate reports signal
  ├── geolocation on mount       → auto fly to user's location on startup
  ├── window_event_listener      → Escape key closes the form panel
  ├── MapEvents click handler    → sets clicked_pos on map click
  ├── MapContainer + TileLayer   → Voyager tile style map
  ├── <For> over reports         → renders a <Marker> + <Popup> per report
  ├── user location marker       → blue dot at current location (when available)
  ├── move || match clicked_pos  → conditional: placeholder or form
  ├── controlled inputs          → on:input + prop:value keep signals in sync
  ├── spawn_local + gloo-net     → async POST to /api/reports
  ├── re-fetch reports on submit → GET /api/reports → update reports signal
  └── #[wasm_bindgen(start)]     → browser entry point
```

| Concept | What it means |
|---|---|
| `RwSignal<T>` | Reactive state — `.set()` triggers re-renders of anything that calls `.get()` |
| `Effect::new` | Reactive side effect — re-runs whenever signals it reads change |
| `Option<T>` in signals | Models "nothing yet" vs "something" — drives conditional rendering |
| `move` closures | Required when a closure crosses the WASM boundary or outlives its scope |
| `move \|\|` in `view!` | Reactive closure — re-runs automatically when signals it reads change |
| `.into_any()` | Erases concrete view types so different `match` arms are compatible |
| `<For>` | Reactive list renderer — diffs by key, only re-renders changed items |
| `Closure::once` | Wraps a Rust `FnOnce` so JavaScript can call it as a callback |
| `success.forget()` | Keeps the closure alive past its Rust scope so JS can still call it |
| `web_sys::js_sys::Reflect::set` | Sets a property on a raw JS object from Rust |
| `on:input` / `on:change` | Leptos event listeners; `event_target_value(&e)` extracts the string value |
| `prop:value` | Sets the live DOM property (not HTML attribute) to keep inputs controlled |
| `spawn_local` | Runs async code from a sync context in single-threaded WASM |
| `resp.json::<T>()` | Deserializes the response body into type `T` using serde |
| `resp.ok()` | True if HTTP status is 200–299 |
| `Position::new(lat, lng)` | Required for Marker position — leptos-leaflet doesn't accept raw tuples |
| `icon_class=` | Uses a CSS div as a Leaflet marker icon instead of the default image |
| `use_leaflet_context()` | Retrieves the map instance — only works in children of `MapContainer` |
| z-index: 1000 | Required to render the panel and buttons above Leaflet's layer stack |

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
