use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openapi::import_openapi;
use crate::requests::build_happy_path_body;
use crate::responses::ExpectedResponse;
use crate::schemas::{extract_schema_catalog, first_string_field, required_fields};

const COMMON_MAX_STRING_LENGTH: usize = 256;
const BOUNDARY_STRING_LENGTH: usize = COMMON_MAX_STRING_LENGTH + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioKind {
    HappyPath,
    Validation,
    Boundary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub method: String,
    pub path: String,
    pub kind: ScenarioKind,
    pub body: Value,
    pub expected_status: u16,
}

pub fn generate_test_scenarios(spec: &Value) -> Vec<Scenario> {
    let graph = import_openapi(spec);
    let schemas = extract_schema_catalog(spec);

    let mut scenarios = Vec::new();

    for operation in graph.operations {
        let Some(schema_name) = operation.request_schema.as_ref() else {
            continue;
        };
        let Some(schema) = schemas.get(schema_name) else {
            continue;
        };

        let happy_body = build_happy_path_body(schema);
        scenarios.push(Scenario {
            name: format!("{} {} happy path", operation.method, operation.path),
            method: operation.method.clone(),
            path: operation.path.clone(),
            kind: ScenarioKind::HappyPath,
            body: happy_body,
            expected_status: ExpectedResponse::Success.status_code(),
        });

        for field in required_fields(schema) {
            let mut validation_body = build_happy_path_body(schema);
            if let Some(obj) = validation_body.as_object_mut() {
                obj.insert(field.clone(), Value::Null);
            }
            scenarios.push(Scenario {
                name: format!(
                    "{} {} validation error ({})",
                    operation.method, operation.path, field
                ),
                method: operation.method.clone(),
                path: operation.path.clone(),
                kind: ScenarioKind::Validation,
                body: validation_body,
                expected_status: ExpectedResponse::ValidationError.status_code(),
            });
        }

        let mut boundary_body = build_happy_path_body(schema);
        if let Some(field) = first_string_field(schema) {
            if let Some(obj) = boundary_body.as_object_mut() {
                obj.insert(field, Value::String("x".repeat(BOUNDARY_STRING_LENGTH)));
            }
        }
        scenarios.push(Scenario {
            name: format!("{} {} boundary", operation.method, operation.path),
            method: operation.method,
            path: operation.path,
            kind: ScenarioKind::Boundary,
            body: boundary_body,
            expected_status: ExpectedResponse::ValidationError.status_code(),
        });
    }

    scenarios
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn generates_happy_validation_and_boundary_scenarios() {
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/customers": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/Customer"}
                                }
                            }
                        }
                    }
                }
            },
            "components": {
                "schemas": {
                    "Customer": {
                        "type": "object",
                        "required": ["name", "email"],
                        "properties": {
                            "name": {"type": "string"},
                            "email": {"type": "string"},
                            "status": {"type": "string", "enum": ["active", "inactive"]}
                        }
                    }
                }
            }
        });

        let scenarios = generate_test_scenarios(&spec);
        assert_eq!(scenarios.len(), 4);
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.kind == ScenarioKind::HappyPath));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.kind == ScenarioKind::Validation));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.kind == ScenarioKind::Boundary));
    }
}
