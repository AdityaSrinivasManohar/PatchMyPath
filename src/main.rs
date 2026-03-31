use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Type of damages
#[derive(Debug, Serialize, Deserialize)]
enum DamageType {
    Pothole,
    CracksOnRoad,
    WaterLeak,
}

// Represents a GPS location
#[derive(Debug, Serialize, Deserialize)]
struct GPSLocation {
    latitude: f64,
    longitude: f64,
}

// Represents the status of a fix
#[derive(Debug, Serialize, Deserialize)]
enum FixStatus {
    Pending,
    InProgress,
    Completed,
}

// Represents a damage report
#[derive(Debug, Serialize, Deserialize)]
struct DamageReport {
    damage_type: DamageType,
    location: GPSLocation,
    severity: u8,
    description: String,
    image: Option<String>,
    timestamp: DateTime<Utc>,
    status: FixStatus,
}

fn main() {
    let report = DamageReport {
        damage_type: DamageType::Pothole,
        location: GPSLocation {
            latitude: 40.7128,
            longitude: -74.0060,
        },
        severity: 3,
        description: "Pothole near the bridge".to_string(),
        image: None,
        timestamp: Utc::now(),
        status: FixStatus::Pending,
    };

    println!("Inital report\n{:?}", report);

    let json = serde_json::to_string(&report).unwrap();
    println!("JSON: {}", json);

    let deserialized: DamageReport = serde_json::from_str(&json).unwrap();
    println!("Deserialized: {:?}", deserialized);
}
