use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use shared::{CreateReportRequest, DamageReport, DamageType, FixStatus, GPSLocation, UpdateStatusRequest};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

struct AppData {
    db: Mutex<Connection>,
    admin_password: String,
}

type AppState = Arc<AppData>;

fn init_db() -> Connection {
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "reports.db".to_string());
    let conn = Connection::open(&db_path).unwrap();
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

fn check_auth(headers: &HeaderMap, admin_password: &str) -> bool {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == admin_password)
        .unwrap_or(false)
}

async fn list_reports(State(state): State<AppState>) -> Json<Vec<DamageReport>> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, damage_type, latitude, longitude, severity, description, image, timestamp, status FROM reports"
    ).unwrap();

    let reports = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, u8>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    }).unwrap()
    .filter_map(|r| r.ok())
    .map(|(id, damage_type_str, lat, lng, severity, description, image, timestamp_str, status_str)| {
        DamageReport {
            id,
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
    let conn = state.db.lock().unwrap();
    let timestamp = Utc::now();
    conn.execute(
        "INSERT INTO reports (damage_type, latitude, longitude, severity, description, image, timestamp, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            format!("{:?}", req.damage_type),
            req.location.latitude,
            req.location.longitude,
            req.severity,
            req.description,
            req.image,
            timestamp.to_rfc3339(),
            format!("{:?}", FixStatus::Pending),
        ],
    ).unwrap();

    let id = conn.last_insert_rowid();
    let report = DamageReport {
        id,
        damage_type: req.damage_type,
        location: req.location,
        severity: req.severity,
        description: req.description,
        image: req.image,
        timestamp,
        status: FixStatus::Pending,
    };

    (StatusCode::CREATED, Json(report))
}

async fn admin_ping(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> StatusCode {
    if check_auth(&headers, &state.admin_password) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

async fn update_report_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(req): Json<UpdateStatusRequest>,
) -> StatusCode {
    if !check_auth(&headers, &state.admin_password) {
        return StatusCode::UNAUTHORIZED;
    }
    let conn = state.db.lock().unwrap();
    conn.execute(
        "UPDATE reports SET status = ?1 WHERE id = ?2",
        params![format!("{:?}", req.status), id],
    ).unwrap();
    StatusCode::OK
}

async fn delete_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> StatusCode {
    if !check_auth(&headers, &state.admin_password) {
        return StatusCode::UNAUTHORIZED;
    }
    let conn = state.db.lock().unwrap();
    conn.execute("DELETE FROM reports WHERE id = ?1", params![id]).unwrap();
    StatusCode::NO_CONTENT
}

#[tokio::main]
async fn main() {
    let admin_password = std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());
    let state: AppState = Arc::new(AppData {
        db: Mutex::new(init_db()),
        admin_password,
    });

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let serve_dir = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{}/index.html", &static_dir)));

    let app = Router::new()
        .route("/api/reports", get(list_reports))
        .route("/api/reports", post(create_report))
        .route("/api/reports/{id}", patch(update_report_status))
        .route("/api/reports/{id}", delete(delete_report))
        .route("/api/admin/ping", get(admin_ping))
        .layer(CorsLayer::permissive())
        .with_state(state)
        .fallback_service(serve_dir);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}
