#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub actions: Vec<String>,
}
