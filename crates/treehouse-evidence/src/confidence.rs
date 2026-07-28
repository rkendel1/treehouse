use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confidence {
    pub score: f32,
    pub reason: Option<String>,
}

impl Confidence {
    pub fn new(score: f32, reason: impl Into<Option<String>>) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
            reason: reason.into(),
        }
    }

    pub fn combine(left: &Self, right: &Self) -> Self {
        Self {
            score: ((left.score + right.score) / 2.0).clamp(0.0, 1.0),
            reason: left.reason.clone().or_else(|| right.reason.clone()),
        }
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::new(0.8, None)
    }
}
