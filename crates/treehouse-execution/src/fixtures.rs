use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FixtureStore {
    statuses: BTreeMap<String, u16>,
}

impl FixtureStore {
    pub fn set_status(&mut self, method: &str, path: &str, status: u16) {
        self.statuses
            .insert(format!("{} {}", method.to_ascii_uppercase(), path), status);
    }

    pub fn status_for(&self, method: &str, path: &str) -> Option<u16> {
        self.statuses
            .get(&format!("{} {}", method.to_ascii_uppercase(), path))
            .copied()
    }
}
