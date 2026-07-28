use serde::{Deserialize, Serialize};

use crate::{EvidenceEdge, EvidenceNode};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct EvidenceSnapshot {
    pub observed_through_unix: u64,
    pub nodes: Vec<EvidenceNode>,
    pub edges: Vec<EvidenceEdge>,
}

impl EvidenceSnapshot {
    pub fn newest_node(&self) -> Option<&EvidenceNode> {
        self.nodes.last()
    }
}
