use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use shared::{CreateReportRequest, DamageReport, FixStatus};
use tower_http::cors::CorsLayer;

type AppState = Arc<Mutex<Vec<DamageReport>>>;

async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let reports = state.lock().unwrap();
    Json(reports.clone())
}

async fn create_report(
    State(state): State<AppState>,
    Json(req): Json<CreateReportRequest>,
) -> (StatusCode, Json<DamageReport>) {
    let report = DamageReport {
        damage_type: req.damage_type,
        location: req.location,
        severity: req.severity,
        description: req.description,
        image: req.image,
        timestamp: Utc::now(),
        status: FixStatus::Pending,
    };

    let mut reports = state.lock().unwrap();
    reports.push(report.clone());

    (StatusCode::CREATED, Json(report))
}

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(Vec::new()));

    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
