#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormDefinition {
    pub name: String,
    pub entity: String,
    pub fields: Vec<String>,
}
