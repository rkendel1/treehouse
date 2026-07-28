use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityObservation {
    pub entity: String,
    pub instances: usize,
    pub sources: BTreeSet<String>,
    pub sample_paths: Vec<String>,
}
