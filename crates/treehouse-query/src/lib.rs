use anyhow::{anyhow, Result};
use serde_json::Value;
use treehouse_core::Document;

#[derive(Debug, Clone, PartialEq)]
pub struct QueryMatch {
    pub path: String,
    pub value: Value,
}

pub fn value_at_path<'a>(document: &'a Document, path: &str) -> Option<&'a Value> {
    if path == "$" {
        return Some(document.root());
    }

    let mut current = document.root();
    let mut chars = path.chars().peekable();

    if chars.next()? != '$' {
        return None;
    }

    while let Some(ch) = chars.peek().copied() {
        match ch {
            '.' => {
                chars.next();
                let mut key = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == '.' || c == '[' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                }

                if key.is_empty() {
                    return None;
                }

                current = current.as_object()?.get(&key)?;
            }
            '[' => {
                chars.next();
                let mut index_buf = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == ']' {
                        break;
                    }
                    index_buf.push(c);
                    chars.next();
                }
                if chars.next()? != ']' {
                    return None;
                }
                let index = index_buf.parse::<usize>().ok()?;
                current = current.as_array()?.get(index)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

pub fn query_json_path(document: &Document, query: &str) -> Result<Vec<QueryMatch>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if trimmed == "$" {
        return Ok(vec![QueryMatch {
            path: "$".to_string(),
            value: document.root().clone(),
        }]);
    }

    if let Some(stripped) = trimmed.strip_prefix("$..") {
        let key = stripped.trim();
        if key.is_empty() {
            return Err(anyhow!("invalid JSONPath: missing recursive key"));
        }

        let mut out = Vec::new();
        recursive_find(document.root(), "$", key, &mut out);
        return Ok(out);
    }

    if trimmed.contains("[*]") {
        return query_array_wildcard(document, trimmed);
    }

    match value_at_path(document, trimmed) {
        Some(value) => Ok(vec![QueryMatch {
            path: trimmed.to_string(),
            value: value.clone(),
        }]),
        None => Ok(Vec::new()),
    }
}

fn query_array_wildcard(document: &Document, query: &str) -> Result<Vec<QueryMatch>> {
    let parts: Vec<&str> = query.split("[*]").collect();
    if parts.len() != 2 {
        return Err(anyhow!("invalid JSONPath wildcard expression"));
    }

    let prefix = parts[0];
    let suffix = parts[1];

    let array_value = value_at_path(document, prefix)
        .ok_or_else(|| anyhow!("invalid JSONPath array prefix: {}", prefix))?;
    let array = array_value
        .as_array()
        .ok_or_else(|| anyhow!("wildcard prefix is not an array: {}", prefix))?;

    let mut out = Vec::new();
    for (index, item) in array.iter().enumerate() {
        let indexed_path = format!("{}[{}]{}", prefix, index, suffix);
        if suffix.is_empty() {
            out.push(QueryMatch {
                path: format!("{}[{}]", prefix, index),
                value: item.clone(),
            });
        } else if let Some(value) = value_at_path_in_value(item, suffix) {
            out.push(QueryMatch {
                path: indexed_path,
                value: value.clone(),
            });
        }
    }

    Ok(out)
}

fn value_at_path_in_value<'a>(root: &'a Value, suffix: &str) -> Option<&'a Value> {
    if suffix.is_empty() {
        return Some(root);
    }

    let mut current = root;
    let mut chars = suffix.chars().peekable();

    while let Some(ch) = chars.peek().copied() {
        match ch {
            '.' => {
                chars.next();
                let mut key = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == '.' || c == '[' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                }
                if key.is_empty() {
                    return None;
                }
                current = current.as_object()?.get(&key)?;
            }
            '[' => {
                chars.next();
                let mut index_buf = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c == ']' {
                        break;
                    }
                    index_buf.push(c);
                    chars.next();
                }
                if chars.next()? != ']' {
                    return None;
                }
                let index = index_buf.parse::<usize>().ok()?;
                current = current.as_array()?.get(index)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

fn recursive_find(value: &Value, path: &str, key: &str, out: &mut Vec<QueryMatch>) {
    match value {
        Value::Object(map) => {
            for (child_key, child_value) in map {
                let child_path = format!("{}.{}", path, child_key);
                if child_key == key {
                    out.push(QueryMatch {
                        path: child_path.clone(),
                        value: child_value.clone(),
                    });
                }
                recursive_find(child_value, &child_path, key, out);
            }
        }
        Value::Array(items) => {
            for (index, child_value) in items.iter().enumerate() {
                recursive_find(child_value, &format!("{}[{}]", path, index), key, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> Document {
        let value: Value = serde_json::from_str(
            r#"{
                "orders": [{"price": 10, "status": "paid"}, {"price": 20, "status": "pending"}],
                "customers": [{"name":"a"}],
                "meta": {"price": 99}
            }"#,
        )
        .unwrap();
        Document::new(value, 0)
    }

    #[test]
    fn supports_direct_paths() {
        let doc = sample_doc();
        let matches = query_json_path(&doc, "$.customers[0].name").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, Value::String("a".to_string()));
    }

    #[test]
    fn supports_recursive_key_search() {
        let doc = sample_doc();
        let matches = query_json_path(&doc, "$..price").unwrap();
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn supports_array_wildcard() {
        let doc = sample_doc();
        let matches = query_json_path(&doc, "$.orders[*].status").unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, "$.orders[0].status");
    }

    #[test]
    fn resolves_value_by_path() {
        let doc = sample_doc();
        let value = value_at_path(&doc, "$.orders[1].price").unwrap();
        assert_eq!(value, &Value::from(20));
    }
}
