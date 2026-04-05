use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use shared::{CreateReportRequest, DamageReport, DamageType, FixStatus, GPSLocation};
use tower_http::cors::CorsLayer;

type AppState = Arc<Mutex<Connection>>;

fn init_db() -> Connection {
    let conn = Connection::open("reports.db").unwrap();
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS reports (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            damage_type TEXT    NOT NULL,
            latitude    REAL    NOT NULL,
            longitude   REAL    NOT NULL,
            severity    INTEGER NOT NULL,
            description TEXT    NOT NULL,
            image       TEXT,
            timestamp   TEXT    NOT NULL,
            status      TEXT    NOT NULL
        );
    ").unwrap();
    conn
}

async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let conn = state.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT damage_type, latitude, longitude, severity, description, image, timestamp, status FROM reports"
    ).unwrap();

    let reports = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, u8>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    }).unwrap()
    .filter_map(|r| r.ok())
    .map(|(damage_type_str, lat, lng, severity, description, image, timestamp_str, status_str)| {
        DamageReport {
            damage_type: match damage_type_str.as_str() {
                "CracksOnRoad" => DamageType::CracksOnRoad,
                "WaterLeak"    => DamageType::WaterLeak,
                _              => DamageType::Pothole,
            },
            location: GPSLocation { latitude: lat, longitude: lng },
            severity,
            description,
            image,
            timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                .unwrap()
                .with_timezone(&Utc),
            status: match status_str.as_str() {
                "InProgress" => FixStatus::InProgress,
                "Completed"  => FixStatus::Completed,
                _            => FixStatus::Pending,
            },
        }
    })
    .collect();

    Json(reports)
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

    let conn = state.lock().unwrap();
    conn.execute(
        "INSERT INTO reports (damage_type, latitude, longitude, severity, description, image, timestamp, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            format!("{:?}", report.damage_type),
            report.location.latitude,
            report.location.longitude,
            report.severity,
            report.description,
            report.image,
            report.timestamp.to_rfc3339(),
            format!("{:?}", report.status),
        ],
    ).unwrap();

    (StatusCode::CREATED, Json(report))
}

#[tokio::main]
async fn main() {
    let conn = init_db();
    let state: AppState = Arc::new(Mutex::new(conn));

    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
