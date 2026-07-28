use std::collections::HashSet;

use serde_json::Value;
use treehouse_core::{node_meta, node_type, Document, NodeMeta, NodeType};

#[derive(Debug, Clone)]
pub struct TreeRow {
    pub path: String,
    pub depth: usize,
    pub label: String,
    pub node_type: NodeType,
    pub meta: NodeMeta,
    pub expandable: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct TreeState {
    expanded: HashSet<String>,
}

impl Default for TreeState {
    fn default() -> Self {
        let mut expanded = HashSet::new();
        expanded.insert("$".to_string());
        Self { expanded }
    }
}

impl TreeState {
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }

    pub fn toggle(&mut self, path: &str) {
        if !self.expanded.insert(path.to_string()) {
            self.expanded.remove(path);
        }
    }
}

pub fn build_rows(document: &Document, state: &TreeState) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    let mut offset = 0;
    push_rows(
        document.root(),
        "$",
        "root",
        0,
        state,
        &mut rows,
        &mut offset,
    );
    rows
}

fn push_rows(
    value: &Value,
    path: &str,
    key_label: &str,
    depth: usize,
    state: &TreeState,
    rows: &mut Vec<TreeRow>,
    offset: &mut usize,
) {
    let path_owned = path.to_string();
    let meta = node_meta(value, *offset);
    *offset += 1;

    let expandable = matches!(value, Value::Object(map) if !map.is_empty())
        || matches!(value, Value::Array(items) if !items.is_empty());
    let expanded = state.is_expanded(path);
    let label = format_label(key_label, value);

    rows.push(TreeRow {
        path: path_owned,
        depth,
        label,
        node_type: node_type(value),
        meta,
        expandable,
        expanded,
    });

    if !expandable || !expanded {
        return;
    }

    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{}.{}", path, key);
                push_rows(child, &child_path, key, depth + 1, state, rows, offset);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_path = format!("{}[{}]", path, index);
                let label = index.to_string();
                push_rows(child, &child_path, &label, depth + 1, state, rows, offset);
            }
        }
        _ => {}
    }
}

fn format_label(key_label: &str, value: &Value) -> String {
    match value {
        Value::Object(map) => format!("{}: object ({})", key_label, map.len()),
        Value::Array(items) => format!("{}: array ({})", key_label, items.len()),
        Value::String(s) => format!("{}: \"{}\"", key_label, s),
        Value::Number(n) => format!("{}: {}", key_label, n),
        Value::Bool(b) => format!("{}: {}", key_label, b),
        Value::Null => format!("{}: null", key_label),
    }
}

#[cfg(test)]
mod tests {
    use treehouse_core::Document;

    use super::*;

    #[test]
    fn tree_respects_expansion_state() {
        let value: Value =
            serde_json::from_str("{\"users\":[{\"name\":\"a\"},{\"name\":\"b\"}]}").unwrap();
        let doc = Document::new(value, 36);

        let mut state = TreeState::default();
        let collapsed = build_rows(&doc, &state);
        assert_eq!(collapsed.len(), 2);

        state.toggle("$.users");
        let expanded = build_rows(&doc, &state);
        assert!(expanded.len() > collapsed.len());
        assert!(expanded.iter().any(|row| row.path == "$.users[0]"));
    }
}
