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

This tells Cargo what kind of output to produce when building the `frontend` lib:

- **`cdylib`** — "C dynamic library". This is what produces the `.wasm` file. Despite the name being C-related, this is the correct type for WASM output. It means "build a shared library for FFI (foreign function interface)", and WASM is treated as a foreign target.
- **`rlib`** — "Rust library". The normal Rust format, kept so other Rust crates could depend on `frontend` if needed. Mostly here for completeness.

Without `crate-type = ["cdylib"]`, Rust would produce a `.rlib` that Trunk can't turn into WASM.

```toml
[dependencies]
shared = { path = "../shared" }
leptos = { version = "0.8", features = ["csr"] }
leptos-leaflet = "0.10"
wasm-bindgen = "0.2"
```

- **`shared`** — our own domain types. The frontend will use `DamageReport` and `CreateReportRequest` when talking to the backend.
- **`leptos`** with `csr` feature — Leptos is the Rust UI framework. CSR = **client-side rendering**: the entire app runs in the browser. The alternative (SSR) would run on the server and send HTML. We want CSR because map interaction has to happen in the browser. Note: `leptos-leaflet` must use the same version of `leptos` — a version mismatch causes a compile error because the `Component` trait from two different versions is incompatible.
- **`leptos-leaflet`** — Rust components (`MapContainer`, `TileLayer`, `Marker`, etc.) that wrap Leaflet.js via `wasm_bindgen`. You write Rust, Leaflet does the map rendering in JS under the hood.
- **`wasm-bindgen`** — the glue between Rust and the browser's JavaScript environment. Lets Rust call browser APIs (DOM, fetch, events) and lets JS call Rust functions. The `#[wasm_bindgen(start)]` attribute in `lib.rs` comes from this crate.

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

**There is no `<script>` tag for your WASM.** Trunk detects this file, compiles your Rust to WASM, and *injects* the script tag automatically at build time. If you look at the built output (`dist/index.html`) you'll see it added.

**`<body></body>` is empty.** Leptos's `mount_to_body()` call in `lib.rs` inserts the rendered HTML into the body at runtime. The browser starts with an empty body and Rust fills it in.

**Leaflet CSS + JS from CDN.** Leaflet is a JavaScript mapping library. We load it here so it's available when `leptos-leaflet` components initialise. This is the only JavaScript in the project, and we didn't write it.

**The CSS reset** (`margin: 0; padding: 0`) removes the browser's default body margin so the map fills the screen edge to edge.

---

## `src/lib.rs`

This is the entire frontend application. Here's the current file in full:

```rust
use leptos::prelude::*;
use leptos_leaflet::prelude::*;
use wasm_bindgen::prelude::*;

#[component]
fn App() -> impl IntoView {
    let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);

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
        <div style="position: fixed; bottom: 1rem; left: 1rem; background: white; padding: 0.5rem; border-radius: 4px; z-index: 1000;">
            {move || match clicked_pos.get() {
                None => "Click the map to drop a pin".to_string(),
                Some((lat, lng)) => format!("Lat: {:.5}, Lng: {:.5}", lat, lng),
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
use leptos::prelude::*;
use leptos_leaflet::prelude::*;
use wasm_bindgen::prelude::*;
```

The `::prelude::*` pattern brings in each crate's most commonly used items so you don't have to import each one individually.

### `#[component]` and `impl IntoView`

```rust
#[component]
fn App() -> impl IntoView {
```

**`#[component]`** is a Leptos procedural macro that transforms this function into a reusable UI component. Under the hood it generates the wiring Leptos needs to track this component in its reactive system.

**`-> impl IntoView`** — the return type. `IntoView` means "something that can be rendered to the DOM". You don't return a specific named type — you return "whatever the `view!` macro produces". The `impl` keyword means "some concrete type that implements this trait, the compiler figures out which".

### `RwSignal` — reactive state

```rust
let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
```

`RwSignal<T>` is Leptos's reactive state primitive — similar to `useState` in React. It has two sides:
- **Read** — calling `.get()` inside a `move ||` closure re-runs that closure automatically whenever the signal changes
- **Write** — calling `.set(value)` updates the value and triggers any dependent views to re-render

`Option<(f64, f64)>` models "no pin yet" (`None`) or "a pin at these coordinates" (`Some((lat, lng))`).

### The click handler

```rust
let map_events = MapEvents::new().mouse_click(move |e| {
    let latlng = e.lat_lng();
    clicked_pos.set(Some((latlng.lat(), latlng.lng())));
});
```

`MapEvents` is a builder struct — you call `.mouse_click(callback)` to attach a handler, then pass the whole thing to `MapContainer`. When the user clicks the map, Leaflet fires the callback with a `MouseEvent`. `.lat_lng()` extracts the geographic coordinates from that event.

The `move` keyword captures `clicked_pos` by value into the closure. This is required because the closure crosses the WASM boundary — it must own everything it uses since it can't borrow from a scope that might be gone when the callback fires.

### The map

```rust
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
```

**`view!`** is a macro that lets you write HTML-like syntax directly in Rust. It's not real HTML and it's not JSX — it expands into Leptos's internal DOM representation at compile time. String literals inside `view!` must be quoted (`"text"`) because unquoted text would be invalid Rust.

**`MapContainer`** creates the Leaflet map instance and provides a context that child components (like `TileLayer`) use to attach themselves to the map. **`height: 100vh`** is required — without an explicit height, the map div is 0px tall and renders blank. This is a Leaflet requirement, not a Rust one.

**`TileLayer`** fetches and renders the map imagery from OpenStreetMap. The `{s}`, `{z}`, `{x}`, `{y}` placeholders are filled in by Leaflet at runtime.

### Reactive coordinate display

```rust
<div style="... z-index: 1000;">
    {move || match clicked_pos.get() {
        None => "Click the map to drop a pin".to_string(),
        Some((lat, lng)) => format!("Lat: {:.5}, Lng: {:.5}", lat, lng),
    }}
</div>
```

The `move ||` closure is a reactive block — Leptos tracks that it reads `clicked_pos`, so whenever `.set()` is called on the signal, this closure re-runs and the text in the DOM updates automatically. No manual DOM manipulation needed.

**`z-index: 1000`** is required because Leaflet manages its own z-index stack for map layers, controls, and popups. Without it, the div renders behind the map and is invisible even though it's in the DOM.

### Entry point

```rust
#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
```

**`#[wasm_bindgen(start)]`** marks this as the WASM entry point. When the browser loads the `.wasm` module, the JS glue code automatically calls this function. Without it, the function exists in the binary but nothing calls it — the page stays blank.

**`pub`** is required so wasm-bindgen can export it. A private function can't be called from outside the WASM module.

**`mount_to_body(App)`** takes the `App` component (passing the function itself, not calling it — `App` not `App()`) and mounts it into the `<body>` of the HTML document.

---

## Why `src/main.rs` still exists

```rust
fn main() {}
```

An empty stub. The workspace's `cargo build` scans all member crates, and without a `main.rs` certain tooling expects a binary entry point. It does nothing — Trunk ignores it because of `autobins = false`, and the real entry point is in `lib.rs`.

---

## The full picture

```
Cargo.toml
  ├── autobins = false          → ignore src/main.rs as a binary
  ├── crate-type = ["cdylib"]   → produce a .wasm file
  └── leptos + leptos-leaflet   → must share the same leptos version

index.html
  ├── Leaflet CSS + JS from CDN → map rendering
  └── Trunk injects WASM script → your Rust code

src/lib.rs
  ├── RwSignal<Option<(f64,f64)>>  → reactive state for clicked pin
  ├── MapEvents click handler      → updates signal on map click
  ├── MapContainer + TileLayer     → renders the map
  ├── reactive move || closure     → re-renders coordinates on signal change
  └── #[wasm_bindgen(start)] main  → browser entry point
```

| Concept | Where | What it means |
|---|---|---|
| `wasm32-unknown-unknown` | compile target | compile Rust to WebAssembly for the browser |
| `crate-type = ["cdylib"]` | `Cargo.toml` | produce a `.wasm` binary instead of a native binary |
| `#[wasm_bindgen(start)]` | `lib.rs` | marks the WASM entry point the browser calls on load |
| `#[component]` | `lib.rs` | Leptos macro that makes a function into a UI component |
| `view!` | `lib.rs` | macro for writing HTML-like syntax in Rust |
| `mount_to_body` | `lib.rs` | mounts the root component into the HTML `<body>` |
| `RwSignal<T>` | `lib.rs` | reactive state — reads trigger re-renders, writes update the value |
| `Option<T>` in signals | `clicked_pos` | represents "nothing selected yet" vs "something selected" |
| `MapEvents` builder | `lib.rs` | attaches event callbacks to the Leaflet map |
| `move` closures | click handler | closure takes ownership because it may outlive the current scope |
| `move ||` in `view!` | coordinate display | reactive closure — re-runs when signals it reads change |
| `autobins = false` | `Cargo.toml` | stops Cargo treating `main.rs` as a binary target |
| z-index | CSS on div | required to appear above Leaflet's internal layer stack |
