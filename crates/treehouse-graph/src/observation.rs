use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationTrendDirection {
    Increasing,
    Stable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationTrend {
    pub direction: ObservationTrendDirection,
    pub transitions: usize,
    pub distinct_markers: usize,
    pub duplicate_markers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationEvidence {
    pub sample_instances: usize,
    pub source_signals: usize,
    pub sample_path_signals: usize,
    pub temporal_markers: usize,
    pub total_signals: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityObservation {
    pub entity: String,
    pub instances: usize,
    pub sources: BTreeSet<String>,
    pub sample_paths: Vec<String>,
    pub confidence: f32,
    pub evidence: ObservationEvidence,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub timeline_markers: Vec<String>,
    pub trend: ObservationTrend,
}
