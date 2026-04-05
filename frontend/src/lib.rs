use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_leaflet::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use shared::{CreateReportRequest, DamageReport, DamageType, FixStatus, GPSLocation, UpdateStatusRequest};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Sits inside MapContainer (has context access). Watches the fly_to signal
/// and calls map.fly_to_with_zoom when it's set.
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

/// Sits outside MapContainer (renders correctly as a fixed button).
/// On click: gets geolocation, sets user_location marker and triggers fly_to.
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

#[component]
fn AdminPanel(token: String) -> impl IntoView {
    let reports: RwSignal<Vec<DamageReport>> = RwSignal::new(vec![]);

    spawn_local(async move {
        if let Ok(resp) = Request::get("/api/reports").send().await {
            if let Ok(data) = resp.json::<Vec<DamageReport>>().await {
                reports.set(data);
            }
        }
    });

    view! {
        <div class="admin-panel">
            <div class="admin-panel-header">
                <h1 class="admin-title">"Reports"</h1>
                <a href="/" class="admin-back">"← Back to map"</a>
            </div>
            <table class="admin-table">
                <thead>
                    <tr>
                        <th>"Type"</th>
                        <th>"Location"</th>
                        <th>"Severity"</th>
                        <th>"Description"</th>
                        <th>"Reported"</th>
                        <th>"Status"</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || reports.get()
                        key=|r| r.id
                        children=move |r| {
                            let token_s = token.clone();
                            let token_d = token.clone();
                            let report_id = r.id;
                            let current_status = match r.status {
                                FixStatus::InProgress => "InProgress",
                                FixStatus::Completed  => "Completed",
                                FixStatus::Pending    => "Pending",
                            };
                            view! {
                                <tr>
                                    <td>{format!("{:?}", r.damage_type)}</td>
                                    <td class="coords">
                                        {format!("{:.4}, {:.4}", r.location.latitude, r.location.longitude)}
                                    </td>
                                    <td>{r.severity}</td>
                                    <td>{r.description.clone()}</td>
                                    <td class="coords">{r.timestamp.format("%Y-%m-%d").to_string()}</td>
                                    <td>
                                        <select
                                            class="input"
                                            prop:value=current_status
                                            on:change=move |e| {
                                                let t = token_s.clone();
                                                let status_str = event_target_value(&e);
                                                spawn_local(async move {
                                                    let status = match status_str.as_str() {
                                                        "InProgress" => FixStatus::InProgress,
                                                        "Completed"  => FixStatus::Completed,
                                                        _            => FixStatus::Pending,
                                                    };
                                                    let body = serde_json::to_string(
                                                        &UpdateStatusRequest { status }
                                                    ).unwrap();
                                                    let _ = Request::patch(
                                                        &format!("/api/reports/{}", report_id)
                                                    )
                                                    .header("Authorization", &format!("Bearer {}", t))
                                                    .header("Content-Type", "application/json")
                                                    .body(body)
                                                    .unwrap()
                                                    .send()
                                                    .await;
                                                });
                                            }
                                        >
                                            <option value="Pending">"Pending"</option>
                                            <option value="InProgress">"In Progress"</option>
                                            <option value="Completed">"Completed"</option>
                                        </select>
                                    </td>
                                    <td>
                                        <button
                                            class="btn btn-danger"
                                            on:click=move |_| {
                                                let t = token_d.clone();
                                                spawn_local(async move {
                                                    let result = Request::delete(
                                                        &format!("/api/reports/{}", report_id)
                                                    )
                                                    .header("Authorization", &format!("Bearer {}", t))
                                                    .send()
                                                    .await;
                                                    if let Ok(resp) = result {
                                                        if resp.ok() || resp.status() == 204 {
                                                            reports.update(|rs| {
                                                                rs.retain(|r| r.id != report_id)
                                                            });
                                                        }
                                                    }
                                                });
                                            }
                                        >
                                            "Delete"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }
                    />
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn AdminPage() -> impl IntoView {
    let auth_token: RwSignal<Option<String>> = RwSignal::new(None);
    let password_input = RwSignal::new(String::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let checking = RwSignal::new(false);

    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let pw = password_input.get();
        spawn_local(async move {
            checking.set(true);
            error.set(None);
            let result = Request::get("/api/admin/ping")
                .header("Authorization", &format!("Bearer {}", pw))
                .send()
                .await;
            match result {
                Ok(resp) if resp.ok() => auth_token.set(Some(pw)),
                Ok(_) => error.set(Some("Wrong password.".to_string())),
                Err(_) => error.set(Some("Could not reach server.".to_string())),
            }
            checking.set(false);
        });
    };

    view! {
        {move || match auth_token.get() {
            None => view! {
                <div class="admin-login">
                    <h1 class="admin-title">"Admin Panel"</h1>
                    <p class="admin-subtitle">"Enter your password to continue."</p>
                    <form on:submit=submit>
                        <input
                            type="password"
                            class="input"
                            placeholder="Password"
                            prop:value=move || password_input.get()
                            on:input=move |e| password_input.set(event_target_value(&e))
                        />
                        {move || error.get().map(|msg| view! {
                            <p class="admin-error">{msg}</p>
                        })}
                        <button type="submit" class="btn" prop:disabled=move || checking.get()>
                            {move || if checking.get() { "Checking..." } else { "Login" }}
                        </button>
                    </form>
                    <a href="/" class="admin-back">"← Back to map"</a>
                </div>
            }.into_any(),
            Some(token) => view! {
                <AdminPanel token=token />
            }.into_any(),
        }}
    }
}

#[component]
fn MapPage() -> impl IntoView {
    let clicked_pos: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
    let damage_type = RwSignal::new("Pothole".to_string());
    let severity = RwSignal::new(5u8);
    let description = RwSignal::new(String::new());
    let reports: RwSignal<Vec<DamageReport>> = RwSignal::new(vec![]);
    let submitting = RwSignal::new(false);
    let fly_to: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
    let user_location: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);

    spawn_local(async move {
        if let Ok(resp) = Request::get("/api/reports").send().await {
            if let Ok(data) = resp.json::<Vec<DamageReport>>().await {
                reports.set(data);
            }
        }
    });

    // Auto-locate on startup
    if let Some(window) = web_sys::window() {
        if let Ok(geo) = window.navigator().geolocation() {
            let success = Closure::once(move |pos: web_sys::Position| {
                let coords = pos.coords();
                let lat = coords.latitude();
                let lng = coords.longitude();
                user_location.set(Some((lat, lng)));
                fly_to.set(Some((lat, lng)));
            });
            let _ = geo.get_current_position(success.as_ref().unchecked_ref());
            success.forget();
        }
    }

    window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "Escape" {
            clicked_pos.set(None);
        }
    });

    let map_events = MapEvents::new().mouse_click(move |e| {
        let latlng = e.lat_lng();
        clicked_pos.set(Some((latlng.lat(), latlng.lng())));
    });

    view! {
        <MapContainer
            style="height: 100vh; width: 100%;"
            center=Position::new(39.5, -98.35)
            zoom=4.0
            events=map_events
        >
            <TileLayer
                url="https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png"
                attribution="&copy; OpenStreetMap contributors &copy; CARTO"
            />
            <FlyToHandler fly_to=fly_to />
            <For
                each=move || reports.get()
                key=|r| format!("{:.6},{:.6}", r.location.latitude, r.location.longitude)
                children=|r| view! {
                    <Marker position=Position::new(r.location.latitude, r.location.longitude) icon_class="marker-dot">
                        <Popup>
                            <p>{format!("{:?} — severity {}", r.damage_type, r.severity)}</p>
                            <p>{r.description.clone()}</p>
                        </Popup>
                    </Marker>
                }
            />
            {move || user_location.get().map(|(lat, lng)| view! {
                <Marker position=Position::new(lat, lng) icon_class="user-dot" />
            })}
        </MapContainer>

        <LocationButton fly_to=fly_to user_location=user_location />

        <div class="panel">
            {move || match clicked_pos.get() {
                None => view! {
                    <p class="panel-hint">"Click the map to drop a pin"</p>
                }.into_any(),
                Some((lat, lng)) => view! {
                    <p class="coords">{format!("Lat: {:.5}, Lng: {:.5}", lat, lng)}</p>

                    <label class="label">"Type"</label>
                    <select
                        class="input"
                        on:change=move |e| damage_type.set(event_target_value(&e))
                    >
                        <option value="Pothole">"Pothole"</option>
                        <option value="CracksOnRoad">"Cracks on Road"</option>
                        <option value="WaterLeak">"Water Leak"</option>
                    </select>

                    <label class="label">{move || format!("Severity: {}", severity.get())}</label>
                    <input
                        type="range" min="1" max="10"
                        class="range"
                        prop:value=move || severity.get().to_string()
                        on:input=move |e| {
                            if let Ok(v) = event_target_value(&e).parse::<u8>() {
                                severity.set(v);
                            }
                        }
                    />

                    <label class="label">"Description"</label>
                    <textarea
                        class="input"
                        on:input=move |e| description.set(event_target_value(&e))
                        prop:value=move || description.get()
                    />

                    <button
                        class="btn"
                        prop:disabled=move || submitting.get()
                        on:click=move |_| {
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
                                submitting.set(true);
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
                                submitting.set(false);
                            });
                        }
                    >
                        {move || if submitting.get() { "Submitting..." } else { "Submit" }}
                    </button>
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| "Page not found.">
                <Route path=path!("/") view=MapPage />
                <Route path=path!("/admin") view=AdminPage />
            </Routes>
        </Router>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    mount_to_body(App);
}
