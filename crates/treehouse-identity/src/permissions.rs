#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub resource: String,
    pub action: String,
    pub roles: Vec<String>,
}
