use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationTrend {
    Increasing,
    Stable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityObservation {
    pub entity: String,
    pub instances: usize,
    pub sources: BTreeSet<String>,
    pub sample_paths: Vec<String>,
    pub confidence: f32,
    pub observed_count: usize,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub trend: ObservationTrend,
}
