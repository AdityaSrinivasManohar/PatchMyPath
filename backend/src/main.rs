use chrono::Utc;
use shared::{DamageReport, DamageType, FixStatus, GPSLocation};

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
    println!("");

    let json = serde_json::to_string_pretty(&report).unwrap();
    println!("JSON:\n{}", json);
    println!("");

    let deserialized: DamageReport = serde_json::from_str(&json).unwrap();
    println!("Deserialized: {:?}", deserialized);
}
