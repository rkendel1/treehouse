use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEnvironment {
    pub base_url: String,
}

impl Default for ExecutionEnvironment {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:4000".to_string(),
        }
    }
}
