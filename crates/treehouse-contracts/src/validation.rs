use serde_json::Value;

pub fn validate_required_fields(schema: &Value, payload: &Value) -> Vec<String> {
    let Some(required) = schema.get("required").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(obj) = payload.as_object() else {
        return vec!["payload is not an object".to_string()];
    };

    required
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|field| match obj.get(field) {
            Some(Value::Null) | None => Some(format!("{field} is required")),
            _ => None,
        })
        .collect()
}
