use crate::EvidenceNode;

#[derive(Debug, Clone, Default)]
pub struct EvidenceQuery {
    kind: Option<String>,
    subsystem: Option<String>,
    since_unix: Option<u64>,
}

impl EvidenceQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(normalize(&kind.into()));
        self
    }

    pub fn subsystem(mut self, subsystem: impl Into<String>) -> Self {
        self.subsystem = Some(subsystem.into());
        self
    }

    pub fn since_unix(mut self, since_unix: u64) -> Self {
        self.since_unix = Some(since_unix);
        self
    }

    pub fn matches(&self, node: &EvidenceNode) -> bool {
        if let Some(kind) = &self.kind {
            if normalize(node.kind_name()) != *kind {
                return false;
            }
        }

        if let Some(subsystem) = &self.subsystem {
            if node.subsystem.as_deref() != Some(subsystem.as_str()) {
                return false;
            }
        }

        if let Some(since) = self.since_unix {
            if node.observed_at_unix < since {
                return false;
            }
        }

        true
    }
}

fn normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| *ch != '-' && *ch != '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Confidence, EvidenceKind, EvidenceNode, Provenance, SourceKind};

    #[test]
    fn filters_by_kind_and_since() {
        let node = EvidenceNode::new(
            EvidenceKind::Entity {
                name: "Invoice".to_string(),
            },
            20,
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "x", "observer"),
            Some("Billing".to_string()),
            serde_json::Value::Null,
        );

        assert!(EvidenceQuery::new()
            .kind("entity")
            .since_unix(19)
            .matches(&node));
        assert!(!EvidenceQuery::new().kind("api").matches(&node));
        assert!(!EvidenceQuery::new().since_unix(21).matches(&node));
    }
}
