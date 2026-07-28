use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub previous_type: String,
    pub current_type: String,
    pub breaking: bool,
}

pub fn compare_schema(previous: &Value, current: &Value) -> Vec<FieldChange> {
    let previous_properties = previous
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let current_properties = current
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    previous_properties
        .into_iter()
        .filter_map(|(field, old_schema)| {
            let new_schema = current_properties.get(&field)?;
            let previous_type = old_schema
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let current_type = new_schema
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            if previous_type == current_type {
                return None;
            }

            Some(FieldChange {
                field,
                previous_type,
                current_type,
                breaking: true,
            })
        })
        .collect()
}
