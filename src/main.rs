use chrono::{DateTime, Utc};

// Type of damages
#[derive(Debug)]
enum DamageType {
    Pothole,
    CracksOnRoad,
    WaterLeak
}

// Represents a GPS location
#[derive(Debug)]
struct GPSLocation {
    latitude: f64,
    longitude: f64,
}

// Represents the status of a fix
#[derive(Debug)]
enum FixStatus {
    Pending,
    InProgress,
    Completed,
}

// Represents a damage report
#[derive(Debug)]
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
        location: GPSLocation { latitude: 40.7128, longitude: -74.0060 },
        severity: 3,
        description: "Pothole near the bridge".to_string(),
        image: None,
        timestamp: Utc::now(),
        status: FixStatus::Pending,
    };

    println!("{:?}", report);
}
