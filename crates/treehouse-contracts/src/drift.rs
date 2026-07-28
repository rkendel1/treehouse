use serde_json::Value;

use crate::compatibility::compare_schema;

pub fn detect_contract_drift(previous: &Value, current: &Value) -> Vec<String> {
    compare_schema(previous, current)
        .into_iter()
        .map(|change| {
            format!(
                "{} changed from {} to {}",
                change.field, change.previous_type, change.current_type
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{detect_contract_drift, validate_required_fields};

    #[test]
    fn validates_required_fields_and_detects_drift() {
        let previous = json!({
            "required": ["email"],
            "properties": {
                "email": {"type": "string"}
            }
        });
        let current = json!({
            "required": ["email"],
            "properties": {
                "email": {"type": "null"}
            }
        });

        let errors = validate_required_fields(&previous, &json!({"email": null}));
        assert_eq!(errors, vec!["email is required"]);

        let drift = detect_contract_drift(&previous, &current);
        assert_eq!(drift, vec!["email changed from string to null"]);
    }
}
