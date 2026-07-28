#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub current: String,
    pub target: String,
    pub steps: Vec<String>,
}

impl MigrationPlan {
    pub fn new(current: impl Into<String>, target: impl Into<String>, steps: Vec<String>) -> Self {
        Self {
            current: current.into(),
            target: target.into(),
            steps,
        }
    }
}
