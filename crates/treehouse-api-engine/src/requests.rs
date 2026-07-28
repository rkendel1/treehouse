use serde_json::{Map, Value};

use crate::schemas::{required_fields, sample_value};

pub fn build_happy_path_body(schema: &Value) -> Value {
    let mut body = Map::new();
    let required = required_fields(schema);

    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for field in required {
        if let Some(property) = properties.get(&field) {
            body.insert(field, sample_value(property));
        }
    }

    Value::Object(body)
}
