use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiOperation {
    pub method: String,
    pub path: String,
    pub request_schema: Option<String>,
    pub response_schema: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApiGraph {
    pub operations: Vec<ApiOperation>,
}

pub fn import_openapi(spec: &Value) -> ApiGraph {
    let mut operations = Vec::new();
    let methods = ["get", "post", "put", "patch", "delete", "options", "head"];

    let Some(paths) = spec.get("paths").and_then(Value::as_object) else {
        return ApiGraph::default();
    };

    for (path, path_item) in paths {
        let Some(path_item_obj) = path_item.as_object() else {
            continue;
        };

        for method in methods {
            let Some(operation) = path_item_obj.get(method).and_then(Value::as_object) else {
                continue;
            };

            let request_schema = operation
                .get("requestBody")
                .and_then(|v| v.get("content"))
                .and_then(|v| v.get("application/json"))
                .and_then(|v| v.get("schema"))
                .and_then(schema_name_from_ref_or_inline);

            let response_schema = operation
                .get("responses")
                .and_then(Value::as_object)
                .and_then(|responses| {
                    responses
                        .get("200")
                        .or_else(|| responses.values().next())
                        .and_then(|resp| resp.get("content"))
                        .and_then(|v| v.get("application/json"))
                        .and_then(|v| v.get("schema"))
                        .and_then(schema_name_from_ref_or_inline)
                });

            operations.push(ApiOperation {
                method: method.to_ascii_uppercase(),
                path: path.clone(),
                request_schema,
                response_schema,
            });
        }
    }

    operations.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
    ApiGraph { operations }
}

fn schema_name_from_ref_or_inline(schema: &Value) -> Option<String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().map(str::to_string);
    }

    schema
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn imports_openapi_operations() {
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
                        },
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": {"$ref": "#/components/schemas/Customer"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let graph = import_openapi(&spec);
        assert_eq!(graph.operations.len(), 1);
        assert_eq!(graph.operations[0].method, "POST");
        assert_eq!(graph.operations[0].path, "/customers");
        assert_eq!(
            graph.operations[0].request_schema.as_deref(),
            Some("Customer")
        );
    }
}
