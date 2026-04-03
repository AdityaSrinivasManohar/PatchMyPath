use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[component]
fn App() -> impl IntoView {
    view! {
        <h1>"Patch My Path"</h1>
        <p>"Map goes here."</p>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
