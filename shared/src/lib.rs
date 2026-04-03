use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Type of damages
#[derive(Debug, Serialize, Deserialize)]
pub enum DamageType {
    Pothole,
    CracksOnRoad,
    WaterLeak,
}

// Represents a GPS location
#[derive(Debug, Serialize, Deserialize)]
pub struct GPSLocation {
    pub latitude: f64,
    pub longitude: f64,
}

// Represents the status of a fix
#[derive(Debug, Serialize, Deserialize)]
pub enum FixStatus {
    Pending,
    InProgress,
    Completed,
}

// Represents a damage report
#[derive(Debug, Serialize, Deserialize)]
pub struct DamageReport {
    pub damage_type: DamageType,
    pub location: GPSLocation,
    pub severity: u8,
    pub description: String,
    pub image: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub status: FixStatus,
}
