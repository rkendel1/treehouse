use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use treehouse_graph::{EntitySchema, UniversalDataGraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiOperation {
    List,
    GetById,
    Create,
    PatchById,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub entity: String,
    pub method: HttpMethod,
    pub operation: ApiOperation,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApiSurface {
    pub entities: Vec<String>,
    pub endpoints: Vec<ApiEndpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestIntent {
    Create,
    Patch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuiltRequest {
    pub entity: String,
    pub method: HttpMethod,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub required_fields: Vec<String>,
    pub optional_fields: Vec<String>,
    pub body: Value,
}

pub fn generate_api_surface(graph: &UniversalDataGraph) -> ApiSurface {
    let mut entities: Vec<String> = graph.schemas.iter().map(|schema| schema.name.clone()).collect();
    entities.sort();
    entities.dedup();

    let mut endpoints = Vec::new();
    for entity in &entities {
        let plural = plural_route(entity);
        endpoints.push(ApiEndpoint {
            entity: entity.clone(),
            method: HttpMethod::Get,
            operation: ApiOperation::List,
            path: format!("/{plural}"),
        });
        endpoints.push(ApiEndpoint {
            entity: entity.clone(),
            method: HttpMethod::Get,
            operation: ApiOperation::GetById,
            path: format!("/{plural}/{{id}}"),
        });
        endpoints.push(ApiEndpoint {
            entity: entity.clone(),
            method: HttpMethod::Post,
            operation: ApiOperation::Create,
            path: format!("/{plural}"),
        });
        endpoints.push(ApiEndpoint {
            entity: entity.clone(),
            method: HttpMethod::Patch,
            operation: ApiOperation::PatchById,
            path: format!("/{plural}/{{id}}"),
        });
    }

    ApiSurface { entities, endpoints }
}

pub fn build_request(schema: &EntitySchema, intent: RequestIntent) -> BuiltRequest {
    let plural = plural_route(&schema.name);
    let method = match intent {
        RequestIntent::Create => HttpMethod::Post,
        RequestIntent::Patch => HttpMethod::Patch,
    };
    let path = match intent {
        RequestIntent::Create => format!("/{plural}"),
        RequestIntent::Patch => format!("/{plural}/{{id}}"),
    };

    let mut required_fields = Vec::new();
    let mut optional_fields = Vec::new();
    let mut body = Map::new();

    for field in &schema.properties {
        if field.name == "id" || field.name.ends_with("Id") || field.name.ends_with("_id") {
            continue;
        }
        if field.required_ratio >= 0.99 {
            required_fields.push(field.name.clone());
            body.insert(
                field.name.clone(),
                Value::String(format!("<{}>", field.name)),
            );
        } else {
            optional_fields.push(field.name.clone());
        }
    }

    BuiltRequest {
        entity: schema.name.clone(),
        method,
        path,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        required_fields,
        optional_fields,
        body: Value::Object(body),
    }
}

fn plural_route(entity: &str) -> String {
    let base = entity.to_lowercase();
    if base.ends_with('s') {
        base
    } else if base.ends_with('y') && base.len() > 1 {
        format!("{}ies", &base[..base.len() - 1])
    } else {
        format!("{base}s")
    }
}

#[cfg(test)]
mod tests {
    use treehouse_graph::{
        EntityProfile, EntitySchema, FieldSchema, GraphNode, UniversalDataGraph, ValueKind,
    };

    use super::*;

    fn schema(name: &str, fields: Vec<(&str, f32)>) -> EntitySchema {
        EntitySchema {
            name: name.to_string(),
            identities: Vec::new(),
            properties: fields
                .into_iter()
                .map(|(field, required)| FieldSchema {
                    name: field.to_string(),
                    kind: ValueKind::String,
                    required_ratio: required,
                    nullable_ratio: 0.0,
                    confidence: 0.95,
                    pii: false,
                    temporal: false,
                })
                .collect(),
            confidence: 0.9,
        }
    }

    #[test]
    fn generates_expected_endpoints() {
        let graph = UniversalDataGraph {
            nodes: Vec::<GraphNode>::new(),
            edges: Vec::new(),
            schemas: vec![
                schema("Customer", vec![("id", 1.0), ("name", 1.0), ("email", 1.0)]),
                schema("Order", vec![("id", 1.0), ("customerId", 1.0)]),
                schema("Invoice", vec![("id", 1.0), ("orderId", 1.0)]),
            ],
            observations: Vec::new(),
            relationships: Vec::new(),
            intelligence: Vec::<EntityProfile>::new(),
        };

        let surface = generate_api_surface(&graph);
        assert!(surface
            .endpoints
            .iter()
            .any(|endpoint| endpoint.path == "/customers" && endpoint.method == HttpMethod::Get));
        assert!(surface.endpoints.iter().any(|endpoint| {
            endpoint.path == "/customers/{id}" && endpoint.method == HttpMethod::Get
        }));
        assert!(surface
            .endpoints
            .iter()
            .any(|endpoint| endpoint.path == "/customers" && endpoint.method == HttpMethod::Post));
        assert!(surface.endpoints.iter().any(|endpoint| {
            endpoint.path == "/customers/{id}" && endpoint.method == HttpMethod::Patch
        }));
        assert!(surface
            .endpoints
            .iter()
            .any(|endpoint| endpoint.path == "/orders" && endpoint.method == HttpMethod::Get));
        assert!(surface.endpoints.iter().any(|endpoint| {
            endpoint.path == "/orders/{id}" && endpoint.method == HttpMethod::Get
        }));
    }

    #[test]
    fn builds_model_first_request() {
        let customer = schema(
            "Customer",
            vec![
                ("id", 1.0),
                ("name", 1.0),
                ("email", 1.0),
                ("phone", 0.3),
                ("address", 0.2),
            ],
        );

        let request = build_request(&customer, RequestIntent::Create);
        assert_eq!(request.path, "/customers");
        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.required_fields, vec!["name", "email"]);
        assert!(request.optional_fields.iter().any(|field| field == "phone"));
        assert!(request.optional_fields.iter().any(|field| field == "address"));
        assert_eq!(
            request.body.get("name"),
            Some(&Value::String("<name>".to_string()))
        );
    }
}
