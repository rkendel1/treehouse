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
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct TreeState {
    expanded: HashSet<String>,
    selected: Option<String>,
}

impl Default for TreeState {
    fn default() -> Self {
        let mut expanded = HashSet::new();
        expanded.insert("$".to_string());
        Self {
            expanded,
            selected: None,
        }
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

    pub fn selected_path(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn select_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        self.expand_path_chain(&path);
        self.selected = Some(path);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn expand_path_chain(&mut self, path: &str) {
        for ancestor in path_ancestors(path) {
            if !self.expanded.insert(ancestor) {
                continue;
            }
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

pub fn path_ancestors(path: &str) -> Vec<String> {
    if path == "$" {
        return vec!["$".to_string()];
    }

    let mut out = vec!["$".to_string()];
    let mut current = String::from("$");
    let mut chars = path.chars().peekable();

    if chars.next() != Some('$') {
        return out;
    }

    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                let mut key = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == '.' || c == '[' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                }
                if !key.is_empty() {
                    current.push('.');
                    current.push_str(&key);
                    out.push(current.clone());
                }
            }
            '[' => {
                let mut index = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == ']' {
                        break;
                    }
                    index.push(c);
                    chars.next();
                }
                if chars.peek() == Some(&']') {
                    chars.next();
                }
                current.push('[');
                current.push_str(&index);
                current.push(']');
                out.push(current.clone());
            }
            _ => {}
        }
    }

    out
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
    let selected = state.selected_path() == Some(path);

    rows.push(TreeRow {
        path: path_owned,
        depth,
        label,
        node_type: node_type(value),
        meta,
        expandable,
        expanded,
        selected,
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

    #[test]
    fn selecting_path_marks_row_selected() {
        let value: Value = serde_json::from_str("{\"users\":[{\"name\":\"a\"}]}").unwrap();
        let doc = Document::new(value, 28);

        let mut state = TreeState::default();
        state.select_path("$.users[0].name");
        let rows = build_rows(&doc, &state);

        assert!(rows
            .iter()
            .any(|row| row.path == "$.users[0].name" && row.selected));
    }

    #[test]
    fn expands_ancestor_chain() {
        let mut state = TreeState::default();
        state.expand_path_chain("$.a.b[2].c");

        assert!(state.is_expanded("$"));
        assert!(state.is_expanded("$.a"));
        assert!(state.is_expanded("$.a.b"));
        assert!(state.is_expanded("$.a.b[2]"));
    }
}
