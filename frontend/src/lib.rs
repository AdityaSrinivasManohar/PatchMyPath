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
