# How the frontend works

A file-by-file walkthrough of the frontend crate for someone new to Rust and WebAssembly.

---

## The big picture first

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
leptos = { version = "0.7", features = ["csr"] }
wasm-bindgen = "0.2"
```

- **`shared`** — our own domain types. The frontend will eventually use `DamageReport` and `CreateReportRequest` when talking to the backend.
- **`leptos`** with `csr` feature — Leptos is the Rust UI framework. CSR = **client-side rendering**: the entire app runs in the browser. The alternative (SSR) would run on the server and send HTML. We want CSR because the map interaction has to happen in the browser.
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
    <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
    <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
  </head>
  <body></body>
</html>
```

This is Trunk's entry point. A few things worth noting:

**There is no `<script>` tag for your WASM.** Trunk detects this file, compiles your Rust to WASM, and *injects* the script tag automatically at build time. If you look at the built output (`dist/index.html`) you'll see it added.

**`<body></body>` is empty.** Leptos's `mount_to_body()` call in `lib.rs` will insert the rendered HTML into the body at runtime. The browser starts with an empty body and Rust fills it in.

**Leaflet CSS + JS from CDN.** Leaflet is a JavaScript mapping library. We load it here so it's available when we wire up the map in Step 3. Nothing uses it yet — it's just being loaded ahead of time. This is the only JavaScript in the project, and we didn't write it.

---

## `src/lib.rs`

```rust
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
```

Import everything we need from Leptos and wasm-bindgen. The `::prelude::*` pattern brings in the most commonly used items so you don't have to import each one individually.

```rust
#[component]
fn App() -> impl IntoView {
    view! {
        <h1>"Patch My Path"</h1>
        <p>"Map goes here."</p>
    }
}
```

**`#[component]`** — a Leptos procedural macro that transforms this function into a reusable UI component. Under the hood it generates the wiring Leptos needs to track this component in its reactive system.

**`-> impl IntoView`** — the return type. `IntoView` means "something that can be rendered to the DOM". You don't return a specific type — you return "whatever the `view!` macro produces", which implements `IntoView`. The `impl` keyword means "some concrete type that implements this trait, I don't care which".

**`view!`** — a macro that lets you write HTML-like syntax directly in Rust. It's not real HTML and it's not JSX — it's a Rust macro that expands into Leptos's internal DOM representation at compile time. Note that string literals inside `view!` must be quoted (`"Patch My Path"` not just `Patch My Path`) because unquoted text would be invalid Rust.

```rust
#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
```

**`#[wasm_bindgen(start)]`** — this attribute marks `main` as the WASM entry point. When the browser loads the `.wasm` module, the JS glue code automatically calls this function. Without it, the function exists in the WASM binary but nothing calls it — the page stays blank (as we discovered).

**`pub fn main()`** — has to be `pub` so wasm-bindgen can export it and the JS glue can call it. A private function can't be called from outside the WASM module.

**`mount_to_body(App)`** — takes the `App` component (note: passing the function itself, not calling it — `App` not `App()`) and mounts it into the `<body>` element of the HTML document. This is the moment Rust takes over the page.

---

## Why `src/main.rs` still exists

```rust
fn main() {}
```

This is an empty stub. The workspace's `cargo build` command scans all member crates, and without this file, certain tooling can get confused expecting a binary entry point. It does nothing — Trunk ignores it because of `autobins = false`, and the real entry point is in `lib.rs`.

---

## The full picture

```
Cargo.toml
  └── [lib] crate-type = ["cdylib"]  → tells Rust to produce a .wasm file

index.html
  └── Trunk entry point              → Trunk injects the WASM script tag here

src/lib.rs
  ├── #[component] App               → defines what to render
  └── #[wasm_bindgen(start)] main    → browser calls this on load
           └── mount_to_body(App)   → Rust renders into <body>
```

Key concepts introduced in this step:

| Concept | Where | What it means |
|---|---|---|
| `wasm32-unknown-unknown` | compile target | "compile Rust to WebAssembly for the browser" |
| `crate-type = ["cdylib"]` | `Cargo.toml` | produce a `.wasm` binary instead of a native binary |
| `#[wasm_bindgen(start)]` | `lib.rs` | marks the WASM entry point the browser calls on load |
| `#[component]` | `lib.rs` | Leptos macro that makes a function into a UI component |
| `view!` | `lib.rs` | macro for writing HTML-like syntax in Rust |
| `mount_to_body` | `lib.rs` | mounts the root component into the HTML `<body>` |
| `autobins = false` | `Cargo.toml` | stops Cargo treating `main.rs` as a binary target |
