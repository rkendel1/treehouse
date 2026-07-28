use std::collections::BTreeMap;

use serde_json::Value;

pub type SchemaCatalog = BTreeMap<String, Value>;

pub fn extract_schema_catalog(spec: &Value) -> SchemaCatalog {
    spec.get("components")
        .and_then(|v| v.get("schemas"))
        .and_then(Value::as_object)
        .map(|schemas| {
            schemas
                .iter()
                .map(|(name, schema)| (name.clone(), schema.clone()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn required_fields(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn first_string_field(schema: &Value) -> Option<String> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| {
            properties.iter().find_map(|(name, property)| {
                property
                    .get("type")
                    .and_then(Value::as_str)
                    .filter(|kind| *kind == "string")
                    .map(|_| name.clone())
            })
        })
}

pub fn sample_value(property: &Value) -> Value {
    if let Some(enum_values) = property.get("enum").and_then(Value::as_array) {
        if let Some(first) = enum_values.first() {
            return first.clone();
        }
    }

    match property.get("type").and_then(Value::as_str) {
        Some("string") => Value::String("example".to_string()),
        Some("integer") => Value::Number(1.into()),
        Some("number") => serde_json::json!(1.0),
        Some("boolean") => Value::Bool(true),
        Some("array") => Value::Array(Vec::new()),
        Some("object") => Value::Object(Default::default()),
        _ => Value::Null,
    }
}
