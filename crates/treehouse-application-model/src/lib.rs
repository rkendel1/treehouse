use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipType {
    #[serde(rename = "one_to_one")]
    OneToOne,
    #[serde(rename = "one_to_many")]
    OneToMany,
    #[serde(rename = "many_to_one")]
    ManyToOne,
    #[serde(rename = "many_to_many")]
    ManyToMany,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    pub name: String,
    pub target: String,
    #[serde(rename = "type")]
    pub relationship_type: RelationshipType,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub name: String,
    #[serde(rename = "type")]
    pub constraint_type: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowTransition {
    pub from: String,
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    pub entity: String,
    pub states: Vec<String>,
    pub transitions: Vec<WorkflowTransition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrudOperation {
    #[serde(rename = "list")]
    List,
    #[serde(rename = "get")]
    Get,
    #[serde(rename = "create")]
    Create,
    #[serde(rename = "update")]
    Update,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    pub operation: CrudOperation,
    pub entity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionPolicy {
    pub entity: String,
    #[serde(default)]
    pub list: bool,
    #[serde(default)]
    pub get: bool,
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub update: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub confidence: f32,
    pub fields: Vec<Field>,
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationMetadata {
    pub generated_by: String,
    pub generated_at_unix: u64,
    pub source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplicationModel {
    pub application: ApplicationInfo,
    pub entities: Vec<Entity>,
    pub workflows: Vec<Workflow>,
    pub permissions: Vec<PermissionPolicy>,
    pub api: Vec<ApiEndpoint>,
    pub metadata: GenerationMetadata,
}

pub fn pluralize(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with('s') {
        lower
    } else if lower.ends_with('y') && lower.len() > 1 {
        format!("{}ies", &lower[..lower.len() - 1])
    } else {
        format!("{lower}s")
    }
}

pub fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}
