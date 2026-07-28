use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract_definition::{ApiContract, SchemaContract};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractValidationKind {
    RequestSchema,
    ResponseSchema,
    Authorization,
    Lifecycle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractValidationIssue {
    pub kind: ContractValidationKind,
    pub target: String,
    pub message: String,
}

pub fn validate_schema_payload(schema: &SchemaContract, payload: &Value) -> Vec<String> {
    let Some(obj) = payload.as_object() else {
        return vec!["payload is not an object".to_string()];
    };

    schema
        .fields
        .iter()
        .filter_map(|field| {
            let value = obj.get(field.name.as_str());
            if field.required && value.is_none_or(Value::is_null) {
                return Some(format!("{} is required", field.name));
            }

            let Some(value) = value else {
                return None;
            };

            if value.is_null() {
                return None;
            }

            let actual = value_type(value);
            if actual == field.field_type {
                None
            } else {
                Some(format!(
                    "{} expected {}, got {}",
                    field.name, field.field_type, actual
                ))
            }
        })
        .collect()
}

pub fn validate_api_contract(
    api: &ApiContract,
    request: &Value,
    response: &Value,
    authorization_context: Option<&str>,
    lifecycle_events: &[String],
) -> Vec<ContractValidationIssue> {
    let target = format!("{} {}", api.method, api.path);
    let mut issues = Vec::new();

    issues.extend(
        validate_schema_payload(&api.request, request)
            .into_iter()
            .map(|message| ContractValidationIssue {
                kind: ContractValidationKind::RequestSchema,
                target: target.clone(),
                message,
            }),
    );

    issues.extend(
        validate_schema_payload(&api.response, response)
            .into_iter()
            .map(|message| ContractValidationIssue {
                kind: ContractValidationKind::ResponseSchema,
                target: target.clone(),
                message,
            }),
    );

    if api.authorization.as_deref() != authorization_context {
        issues.push(ContractValidationIssue {
            kind: ContractValidationKind::Authorization,
            target: target.clone(),
            message: format!(
                "authorization expected {:?}, got {:?}",
                api.authorization, authorization_context
            ),
        });
    }

    let observed_events: BTreeSet<&str> = lifecycle_events.iter().map(String::as_str).collect();
    for expected in &api.lifecycle {
        if !observed_events.contains(expected.as_str()) {
            issues.push(ContractValidationIssue {
                kind: ContractValidationKind::Lifecycle,
                target: target.clone(),
                message: format!("missing lifecycle event {expected}"),
            });
        }
    }

    issues
}

fn value_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(number) => {
            if number.is_f64() {
                "number".to_string()
            } else {
                "integer".to_string()
            }
        }
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::contract_definition::{ApiContract, FieldContract, SchemaContract};

    use super::*;

    #[test]
    fn validates_request_response_authorization_and_lifecycle() {
        let api = ApiContract {
            method: "POST".to_string(),
            path: "/invoice".to_string(),
            request: SchemaContract {
                fields: vec![FieldContract {
                    name: "amount".to_string(),
                    field_type: "number".to_string(),
                    required: true,
                }],
            },
            response: SchemaContract {
                fields: vec![FieldContract {
                    name: "status".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                }],
            },
            authorization: Some("tenant-context".to_string()),
            lifecycle: vec!["PaymentRequested".to_string()],
        };

        let issues = validate_api_contract(
            &api,
            &json!({"amount": "12"}),
            &json!({"status": {"code":"paid"}}),
            Some("organization-context"),
            &[],
        );

        assert!(issues.iter().any(|issue| {
            issue.kind == ContractValidationKind::RequestSchema
                && issue.message.contains("amount expected number")
        }));
        assert!(issues.iter().any(|issue| {
            issue.kind == ContractValidationKind::ResponseSchema
                && issue.message.contains("status expected string")
        }));
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ContractValidationKind::Authorization));
        assert!(issues
            .iter()
            .any(|issue| issue.kind == ContractValidationKind::Lifecycle));
    }
}
