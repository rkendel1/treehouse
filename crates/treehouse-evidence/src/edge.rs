use serde::{Deserialize, Serialize};

use crate::{Confidence, EvidenceId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationKind {
    Supports,
    Contradicts,
    DerivedFrom,
    ObservedIn,
    RelatedTo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceEdge {
    pub from: EvidenceId,
    pub to: EvidenceId,
    pub relation: RelationKind,
    pub confidence: Confidence,
    pub observed_at_unix: u64,
}
