use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use treehouse_core::Document;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentStats {
    pub objects: usize,
    pub arrays: usize,
    pub values: usize,
    pub max_depth: usize,
    pub largest_array: usize,
    pub null_count: usize,
    pub most_common_keys: Vec<(String, usize)>,
}

pub fn analyze(document: &Document) -> DocumentStats {
    let mut stats = DocumentStats::default();
    let mut key_counts = HashMap::new();
    walk(document.root(), 0, &mut stats, &mut key_counts);

    let mut keys: Vec<_> = key_counts.into_iter().collect();
    keys.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    stats.most_common_keys = keys.into_iter().take(5).collect();
    stats
}

fn walk(value: &Value, depth: usize, stats: &mut DocumentStats, key_counts: &mut HashMap<String, usize>) {
    stats.max_depth = stats.max_depth.max(depth);

    match value {
        Value::Object(map) => {
            stats.objects += 1;
            for (key, child) in map {
                *key_counts.entry(key.clone()).or_insert(0) += 1;
                walk(child, depth + 1, stats, key_counts);
            }
        }
        Value::Array(items) => {
            stats.arrays += 1;
            stats.largest_array = stats.largest_array.max(items.len());
            for child in items {
                walk(child, depth + 1, stats, key_counts);
            }
        }
        Value::Null => {
            stats.values += 1;
            stats.null_count += 1;
        }
        _ => {
            stats.values += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use treehouse_core::Document;

    use super::*;

    #[test]
    fn computes_basic_statistics() {
        let value: Value = serde_json::from_str(
            "{\"users\":[{\"id\":1,\"name\":null},{\"id\":2}],\"active\":true}"
        )
        .unwrap();
        let doc = Document::new(value, 60);

        let stats = analyze(&doc);
        assert_eq!(stats.objects, 3);
        assert_eq!(stats.arrays, 1);
        assert_eq!(stats.largest_array, 2);
        assert_eq!(stats.null_count, 1);
        assert!(stats.max_depth >= 2);
        assert!(stats.most_common_keys.iter().any(|(k, v)| k == "id" && *v == 2));
    }
}
