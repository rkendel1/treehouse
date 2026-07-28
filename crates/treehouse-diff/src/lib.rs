use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use treehouse_core::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Added,
    Removed,
    Changed,
    TypeChanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffEntry {
    pub path: String,
    pub kind: DiffKind,
    pub left: Option<Value>,
    pub right: Option<Value>,
}

pub fn diff_documents(left: &Document, right: &Document) -> Vec<DiffEntry> {
    let mut entries = Vec::new();
    diff_values(left.root(), right.root(), "$", &mut entries);
    entries
}

fn diff_values(left: &Value, right: &Value, path: &str, out: &mut Vec<DiffEntry>) {
    if value_tag(left) != value_tag(right) {
        out.push(DiffEntry {
            path: path.to_string(),
            kind: DiffKind::TypeChanged,
            left: Some(left.clone()),
            right: Some(right.clone()),
        });
        return;
    }

    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            let keys: BTreeSet<_> = left_map.keys().chain(right_map.keys()).collect();
            for key in keys {
                let child_path = format!("{path}.{key}");
                match (left_map.get(key), right_map.get(key)) {
                    (Some(left_child), Some(right_child)) => {
                        diff_values(left_child, right_child, &child_path, out);
                    }
                    (Some(left_child), None) => out.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Removed,
                        left: Some(left_child.clone()),
                        right: None,
                    }),
                    (None, Some(right_child)) => out.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Added,
                        left: None,
                        right: Some(right_child.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(left_items), Value::Array(right_items)) => {
            let max = left_items.len().max(right_items.len());
            for index in 0..max {
                let child_path = format!("{path}[{index}]");
                match (left_items.get(index), right_items.get(index)) {
                    (Some(left_child), Some(right_child)) => {
                        diff_values(left_child, right_child, &child_path, out);
                    }
                    (Some(left_child), None) => out.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Removed,
                        left: Some(left_child.clone()),
                        right: None,
                    }),
                    (None, Some(right_child)) => out.push(DiffEntry {
                        path: child_path,
                        kind: DiffKind::Added,
                        left: None,
                        right: Some(right_child.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => {
            if left != right {
                out.push(DiffEntry {
                    path: path.to_string(),
                    kind: DiffKind::Changed,
                    left: Some(left.clone()),
                    right: Some(right.clone()),
                });
            }
        }
    }
}

fn value_tag(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reports_added_removed_and_changed_fields() {
        let left = Document::new(
            json!({
                "id": 1,
                "name": "Alice",
                "tags": ["a", "b"],
                "legacy": true
            }),
            0,
        );
        let right = Document::new(
            json!({
                "id": 1,
                "name": "Alicia",
                "tags": ["a", "c", "d"],
                "status": "active"
            }),
            0,
        );

        let diff = diff_documents(&left, &right);
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.name" && entry.kind == DiffKind::Changed));
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.legacy" && entry.kind == DiffKind::Removed));
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.status" && entry.kind == DiffKind::Added));
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.tags[1]" && entry.kind == DiffKind::Changed));
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.tags[2]" && entry.kind == DiffKind::Added));
    }

    #[test]
    fn reports_type_changes() {
        let left = Document::new(json!({"count": 10}), 0);
        let right = Document::new(json!({"count": {"value": 10}}), 0);

        let diff = diff_documents(&left, &right);
        assert!(diff
            .iter()
            .any(|entry| entry.path == "$.count" && entry.kind == DiffKind::TypeChanged));
    }
}
