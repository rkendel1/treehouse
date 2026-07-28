use serde_json::Value;
use treehouse_core::Document;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: String,
    pub snippet: String,
}

pub fn search_document(document: &Document, query: &str) -> Vec<SearchMatch> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    walk(document.root(), "$", &needle, &mut matches);
    matches
}

fn walk(value: &Value, path: &str, needle: &str, matches: &mut Vec<SearchMatch>) {
    if matches_value(value, needle) || path.to_lowercase().contains(needle) {
        matches.push(SearchMatch {
            path: path.to_string(),
            snippet: summarize(value),
        });
    }

    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{}.{}", path, key);
                if key.to_lowercase().contains(needle) {
                    matches.push(SearchMatch {
                        path: child_path.clone(),
                        snippet: format!("key: {}", key),
                    });
                }
                walk(child, &child_path, needle, matches);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_path = format!("{}[{}]", path, index);
                walk(child, &child_path, needle, matches);
            }
        }
        _ => {}
    }
}

fn matches_value(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(v) => v.to_lowercase().contains(needle),
        Value::Number(v) => v.to_string().contains(needle),
        Value::Bool(v) => v.to_string().contains(needle),
        Value::Null => "null".contains(needle),
        _ => false,
    }
}

fn summarize(value: &Value) -> String {
    match value {
        Value::Object(map) => format!("object ({})", map.len()),
        Value::Array(items) => format!("array ({})", items.len()),
        Value::String(v) => format!("\"{}\"", v),
        Value::Number(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use treehouse_core::Document;

    use super::*;

    #[test]
    fn finds_key_and_value_matches() {
        let value: Value = serde_json::from_str(
            "{\"customerId\":\"abc\",\"orders\":[{\"status\":\"paid\"}]}"
        )
        .unwrap();
        let doc = Document::new(value, 54);

        let key_matches = search_document(&doc, "customer");
        assert!(key_matches.iter().any(|m| m.path == "$.customerId"));

        let value_matches = search_document(&doc, "paid");
        assert!(value_matches.iter().any(|m| m.path == "$.orders[0].status"));
    }
}
