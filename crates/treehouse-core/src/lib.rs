use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type NodeId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMeta {
    pub offset: usize,
    pub length: usize,
    pub node_type: NodeType,
    pub child_count: usize,
}

#[derive(Debug, Clone)]
pub struct Document {
    root: Value,
    source_len: usize,
}

impl Document {
    pub fn new(root: Value, source_len: usize) -> Self {
        Self { root, source_len }
    }

    pub fn root(&self) -> &Value {
        &self.root
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn root_meta(&self) -> NodeMeta {
        node_meta(self.root(), 0)
    }
}

pub fn node_type(value: &Value) -> NodeType {
    match value {
        Value::Object(_) => NodeType::Object,
        Value::Array(_) => NodeType::Array,
        Value::String(_) => NodeType::String,
        Value::Number(_) => NodeType::Number,
        Value::Bool(_) => NodeType::Bool,
        Value::Null => NodeType::Null,
    }
}

pub fn node_meta(value: &Value, offset: usize) -> NodeMeta {
    let child_count = match value {
        Value::Object(map) => map.len(),
        Value::Array(items) => items.len(),
        _ => 0,
    };

    let length = match value {
        Value::String(s) => s.len(),
        _ => value.to_string().len(),
    };

    NodeMeta {
        offset,
        length,
        node_type: node_type(value),
        child_count,
    }
}
