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
