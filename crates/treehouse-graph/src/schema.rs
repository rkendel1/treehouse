use serde::{Deserialize, Serialize};

use crate::identity::Identity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValueKind {
    String,
    Number,
    Boolean,
    Timestamp,
    Object,
    Array,
    Null,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub kind: ValueKind,
    pub required_ratio: f32,
    pub nullable_ratio: f32,
    pub confidence: f32,
    pub pii: bool,
    pub temporal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntitySchema {
    pub name: String,
    pub identities: Vec<Identity>,
    pub properties: Vec<FieldSchema>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityProfile {
    pub name: String,
    pub instances: usize,
    pub fields: usize,
    pub primary_identifier: Option<String>,
    pub required_ratio: f32,
    pub nullable_ratio: f32,
    pub related: Vec<String>,
    pub detected_pii: Vec<String>,
    pub sources: Vec<String>,
    pub confidence: f32,
}
