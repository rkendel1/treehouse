#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTwin {
    pub entities: Vec<String>,
    pub processes: Vec<String>,
    pub apis: Vec<String>,
    pub permissions: Vec<String>,
    pub events: Vec<String>,
}
