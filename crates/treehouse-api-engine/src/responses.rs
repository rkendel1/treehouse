use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectedResponse {
    Success,
    ValidationError,
}

impl ExpectedResponse {
    pub fn status_code(self) -> u16 {
        match self {
            Self::Success => 200,
            Self::ValidationError => 400,
        }
    }
}
