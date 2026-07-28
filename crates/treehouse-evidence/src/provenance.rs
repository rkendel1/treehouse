use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceKind {
    Git,
    Ast,
    Migration,
    Api,
    Workflow,
    Entity,
    Test,
    Runtime,
    Database,
    SystemDiff,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub source: SourceKind,
    pub location: String,
    pub observer: String,
}

impl Provenance {
    pub fn new(
        source: SourceKind,
        location: impl Into<String>,
        observer: impl Into<String>,
    ) -> Self {
        Self {
            source,
            location: location.into(),
            observer: observer.into(),
        }
    }
}
