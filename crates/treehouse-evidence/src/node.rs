use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::{Confidence, Provenance};

pub type EvidenceId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvidenceKind {
    GitDelta {
        file: String,
        status: String,
    },
    Symbol {
        language: String,
        kind: String,
        name: String,
    },
    Migration {
        table: String,
        operation: String,
    },
    ApiSurface {
        method: String,
        path: String,
    },
    Workflow {
        name: String,
    },
    Entity {
        name: String,
    },
    TestResult {
        name: String,
        status: String,
    },
    RuntimeEvent {
        event: String,
    },
    DbSignal {
        signal: String,
    },
    SystemDiffFinding {
        finding: String,
    },
    ScanBaseline {
        summary: String,
    },
    LlmInferredTarget {
        summary: String,
    },
    GapRecommendation {
        recommendation: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceNode {
    pub id: EvidenceId,
    pub kind: EvidenceKind,
    pub observed_at_unix: u64,
    pub confidence: Confidence,
    pub provenance: Provenance,
    pub subsystem: Option<String>,
    pub attributes: serde_json::Value,
}

impl EvidenceNode {
    pub fn new(
        kind: EvidenceKind,
        observed_at_unix: u64,
        confidence: Confidence,
        provenance: Provenance,
        subsystem: Option<String>,
        attributes: serde_json::Value,
    ) -> Self {
        let id = make_id(&kind, observed_at_unix, &provenance.location);
        Self {
            id,
            kind,
            observed_at_unix,
            confidence,
            provenance,
            subsystem,
            attributes,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            EvidenceKind::GitDelta { .. } => "gitdelta",
            EvidenceKind::Symbol { .. } => "symbol",
            EvidenceKind::Migration { .. } => "migration",
            EvidenceKind::ApiSurface { .. } => "apisurface",
            EvidenceKind::Workflow { .. } => "workflow",
            EvidenceKind::Entity { .. } => "entity",
            EvidenceKind::TestResult { .. } => "testresult",
            EvidenceKind::RuntimeEvent { .. } => "runtimeevent",
            EvidenceKind::DbSignal { .. } => "dbsignal",
            EvidenceKind::SystemDiffFinding { .. } => "systemdifffinding",
            EvidenceKind::ScanBaseline { .. } => "scanbaseline",
            EvidenceKind::LlmInferredTarget { .. } => "llminferredtarget",
            EvidenceKind::GapRecommendation { .. } => "gaprecommendation",
        }
    }
}

fn make_id(kind: &EvidenceKind, observed_at_unix: u64, location: &str) -> EvidenceId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    observed_at_unix.hash(&mut hasher);
    location.hash(&mut hasher);
    format!("ev-{:016x}", hasher.finish())
}

impl Hash for EvidenceKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            EvidenceKind::GitDelta { file, status } => {
                file.hash(state);
                status.hash(state);
            }
            EvidenceKind::Symbol {
                language,
                kind,
                name,
            } => {
                language.hash(state);
                kind.hash(state);
                name.hash(state);
            }
            EvidenceKind::Migration { table, operation } => {
                table.hash(state);
                operation.hash(state);
            }
            EvidenceKind::ApiSurface { method, path } => {
                method.hash(state);
                path.hash(state);
            }
            EvidenceKind::Workflow { name } => name.hash(state),
            EvidenceKind::Entity { name } => name.hash(state),
            EvidenceKind::TestResult { name, status } => {
                name.hash(state);
                status.hash(state);
            }
            EvidenceKind::RuntimeEvent { event } => event.hash(state),
            EvidenceKind::DbSignal { signal } => signal.hash(state),
            EvidenceKind::SystemDiffFinding { finding } => finding.hash(state),
            EvidenceKind::ScanBaseline { summary } => summary.hash(state),
            EvidenceKind::LlmInferredTarget { summary } => summary.hash(state),
            EvidenceKind::GapRecommendation { recommendation } => recommendation.hash(state),
        }
    }
}
