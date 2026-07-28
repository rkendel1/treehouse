use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use treehouse_application_model::{
    pluralize, to_snake_case, ApiEndpoint, ApplicationInfo, ApplicationModel, Constraint,
    CrudOperation, Entity, Experience, ExperienceScreen, Field, GenerationMetadata, Integration,
    PermissionPolicy, Relationship, RelationshipType, Workflow, WorkflowTransition,
};
use treehouse_graph::{GraphEdgeKind, IdentityKind, UniversalDataGraph, ValueKind};

pub fn infer_application_model(
    graph: &UniversalDataGraph,
    app_name: Option<&str>,
) -> ApplicationModel {
    let mut entities: Vec<Entity> = graph
        .schemas
        .iter()
        .map(|schema| {
            let primary_fields: Vec<String> = schema
                .identities
                .iter()
                .filter(|identity| identity.kind == IdentityKind::Primary)
                .map(|identity| identity.field.clone())
                .collect();
            let fields: Vec<Field> = schema
                .properties
                .iter()
                .map(|field| Field {
                    name: field.name.clone(),
                    field_type: classify_field_type(&field.name, field.kind, field.temporal),
                    required: field.required_ratio >= 0.95,
                    primary: primary_fields
                        .iter()
                        .any(|identity| identity == &field.name)
                        || field.name.eq_ignore_ascii_case("id"),
                    unique: is_unique_candidate(field),
                    confidence: field.confidence,
                })
                .collect();

            let relationships: Vec<Relationship> = graph
                .relationships
                .iter()
                .filter(|relationship| relationship.from == schema.name)
                .map(|relationship| Relationship {
                    name: relationship_name(relationship.kind, &relationship.to),
                    target: relationship.to.clone(),
                    relationship_type: map_relationship_type(relationship.kind),
                    confidence: f32::from(relationship.confidence) / 100.0,
                })
                .collect();

            let mut constraints = Vec::new();
            for field in &fields {
                if field.primary {
                    constraints.push(Constraint {
                        name: format!(
                            "pk_{}_{}",
                            to_snake_case(&schema.name),
                            to_snake_case(&field.name)
                        ),
                        constraint_type: "primary_key".to_string(),
                        fields: vec![field.name.clone()],
                        expression: None,
                    });
                }
                if field.unique {
                    constraints.push(Constraint {
                        name: format!(
                            "uq_{}_{}",
                            to_snake_case(&schema.name),
                            to_snake_case(&field.name)
                        ),
                        constraint_type: "unique".to_string(),
                        fields: vec![field.name.clone()],
                        expression: None,
                    });
                }
            }

            Entity {
                name: schema.name.clone(),
                confidence: schema.confidence,
                fields,
                relationships,
                constraints,
            }
        })
        .collect();

    entities.sort_by(|a, b| a.name.cmp(&b.name));

    let workflows = infer_workflows(&entities);
    let permissions: Vec<PermissionPolicy> = entities
        .iter()
        .map(|entity| PermissionPolicy {
            entity: entity.name.clone(),
            list: true,
            get: true,
            create: true,
            update: true,
        })
        .collect();
    let api = infer_api_surface(&entities);
    let experiences = infer_experiences(&entities);
    let integrations = infer_integrations(graph);

    let source_count = graph
        .intelligence
        .iter()
        .map(|profile| profile.sources.len())
        .sum();
    ApplicationModel {
        application: ApplicationInfo {
            name: app_name
                .unwrap_or("Treehouse Reconstructed Application")
                .to_string(),
            version: "1.0".to_string(),
        },
        entities,
        workflows,
        permissions,
        api,
        experiences,
        integrations,
        metadata: GenerationMetadata {
            generated_by: "treehouse-model-inference".to_string(),
            generated_at_unix: now_unix(),
            source_count,
        },
    }
}

fn classify_field_type(field_name: &str, kind: ValueKind, temporal_hint: bool) -> String {
    let lower = field_name.to_lowercase();
    if lower.contains("email") {
        return "email".to_string();
    }
    if lower.contains("phone") {
        return "phone".to_string();
    }
    if lower.contains("url") || lower.contains("uri") {
        return "url".to_string();
    }
    if lower == "id" || lower.ends_with("_id") || lower.ends_with("id") {
        return "uuid".to_string();
    }
    if lower.contains("status") {
        return "status_enum".to_string();
    }
    if lower.contains("amount") || lower.contains("price") || lower.contains("total") {
        return "money".to_string();
    }
    if temporal_hint {
        return "timestamp".to_string();
    }

    match kind {
        ValueKind::Boolean => "boolean".to_string(),
        ValueKind::Number => "number".to_string(),
        ValueKind::Timestamp => "timestamp".to_string(),
        ValueKind::Array => "array".to_string(),
        ValueKind::Object => "object".to_string(),
        ValueKind::Null | ValueKind::Mixed | ValueKind::String => "string".to_string(),
    }
}

fn is_unique_candidate(field: &treehouse_graph::FieldSchema) -> bool {
    let lower = field.name.to_lowercase();
    lower == "email" || lower.ends_with("_email") || lower == "external_id"
}

fn map_relationship_type(kind: GraphEdgeKind) -> RelationshipType {
    match kind {
        GraphEdgeKind::HasMany => RelationshipType::OneToMany,
        GraphEdgeKind::BelongsTo => RelationshipType::ManyToOne,
        GraphEdgeKind::Related => RelationshipType::OneToOne,
        GraphEdgeKind::DerivedFrom | GraphEdgeKind::HasField => RelationshipType::OneToOne,
    }
}

fn relationship_name(kind: GraphEdgeKind, target: &str) -> String {
    match kind {
        GraphEdgeKind::HasMany => pluralize(target),
        GraphEdgeKind::BelongsTo => target.to_lowercase(),
        GraphEdgeKind::Related | GraphEdgeKind::DerivedFrom | GraphEdgeKind::HasField => {
            target.to_lowercase()
        }
    }
}

fn infer_workflows(entities: &[Entity]) -> Vec<Workflow> {
    entities
        .iter()
        .filter(|entity| {
            let has_status = entity
                .fields
                .iter()
                .any(|field| field.name.eq_ignore_ascii_case("status"));
            let has_temporal = entity.fields.iter().any(|field| {
                let lower = field.name.to_lowercase();
                lower.contains("created")
                    || lower.contains("updated")
                    || lower.contains("completed")
            });
            has_status && has_temporal
        })
        .map(|entity| Workflow {
            entity: entity.name.clone(),
            states: vec![
                "created".to_string(),
                "paid".to_string(),
                "shipped".to_string(),
                "completed".to_string(),
            ],
            transitions: vec![
                WorkflowTransition {
                    from: "created".to_string(),
                    allowed: vec!["paid".to_string()],
                },
                WorkflowTransition {
                    from: "paid".to_string(),
                    allowed: vec!["shipped".to_string()],
                },
                WorkflowTransition {
                    from: "shipped".to_string(),
                    allowed: vec!["completed".to_string()],
                },
            ],
        })
        .collect()
}

fn infer_api_surface(entities: &[Entity]) -> Vec<ApiEndpoint> {
    let mut api = Vec::new();
    for entity in entities {
        let route = pluralize(&entity.name);
        api.push(ApiEndpoint {
            method: "GET".to_string(),
            path: format!("/{route}"),
            operation: CrudOperation::List,
            entity: entity.name.clone(),
        });
        api.push(ApiEndpoint {
            method: "GET".to_string(),
            path: format!("/{route}/:id"),
            operation: CrudOperation::Get,
            entity: entity.name.clone(),
        });
        api.push(ApiEndpoint {
            method: "POST".to_string(),
            path: format!("/{route}"),
            operation: CrudOperation::Create,
            entity: entity.name.clone(),
        });
        api.push(ApiEndpoint {
            method: "PATCH".to_string(),
            path: format!("/{route}/:id"),
            operation: CrudOperation::Update,
            entity: entity.name.clone(),
        });
    }
    api
}

fn infer_experiences(entities: &[Entity]) -> Vec<Experience> {
    let screens = entities
        .iter()
        .flat_map(|entity| {
            let route = pluralize(&entity.name);
            [
                ExperienceScreen {
                    name: format!("{} List", entity.name),
                    route: format!("/{route}"),
                    entities: vec![entity.name.clone()],
                },
                ExperienceScreen {
                    name: format!("{} Detail", entity.name),
                    route: format!("/{route}/:id"),
                    entities: vec![entity.name.clone()],
                },
                ExperienceScreen {
                    name: format!("Create {}", entity.name),
                    route: format!("/{route}/new"),
                    entities: vec![entity.name.clone()],
                },
            ]
        })
        .collect();

    vec![Experience {
        name: "Application Experience".to_string(),
        screens,
    }]
}

fn infer_integrations(graph: &UniversalDataGraph) -> Vec<Integration> {
    let mut sources = BTreeSet::new();
    for profile in &graph.intelligence {
        for source in &profile.sources {
            sources.insert(source.clone());
        }
    }

    sources
        .into_iter()
        .filter_map(|source| {
            let lower = source.to_ascii_lowercase();
            if lower.contains("openapi")
                || lower.contains("swagger")
                || lower.contains("postman")
                || lower.contains("api")
            {
                Some(Integration {
                    name: "API Integration".to_string(),
                    integration_type: "api".to_string(),
                    target: source,
                    confidence: 0.9,
                })
            } else if lower.contains("event") || lower.contains("audit") || lower.contains("log") {
                Some(Integration {
                    name: "Event Integration".to_string(),
                    integration_type: "event_stream".to_string(),
                    target: source,
                    confidence: 0.8,
                })
            } else {
                None
            }
        })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use treehouse_core::Document;
    use treehouse_graph::{GraphSource, UniversalDataGraph};

    use super::*;

    #[test]
    fn infers_application_entities_relationships_and_api() {
        let customers = Document::new(json!([{"id": "c1", "email": "alice@example.com"}]), 0);
        let orders = Document::new(
            json!([{
                "id": "o1",
                "customerId": "c1",
                "status": "pending",
                "created_at": "2026-01-01T00:00:00Z",
                "amount": 9.99
            }]),
            0,
        );

        let graph = UniversalDataGraph::build(&[
            GraphSource {
                name: "customers.json",
                document: &customers,
            },
            GraphSource {
                name: "orders.json",
                document: &orders,
            },
            GraphSource {
                name: "openapi-orders.json",
                document: &orders,
            },
        ]);

        let model = infer_application_model(&graph, Some("Commerce System"));
        assert_eq!(model.application.name, "Commerce System");
        assert!(model
            .entities
            .iter()
            .any(|entity| entity.name == "Customer"));
        assert!(model.entities.iter().any(|entity| entity.name == "Order"));
        assert!(model
            .api
            .iter()
            .any(|endpoint| endpoint.path == "/customers"));
        assert!(model
            .api
            .iter()
            .any(|endpoint| endpoint.path == "/orders/:id"));
        assert!(model
            .workflows
            .iter()
            .any(|workflow| workflow.entity == "Order"
                && workflow.states.iter().any(|state| state == "created")));
        assert!(model
            .experiences
            .iter()
            .flat_map(|experience| experience.screens.iter())
            .any(|screen| screen.name == "Customer Detail"));
        assert!(model
            .integrations
            .iter()
            .any(|integration| integration.integration_type == "api"));
    }
}
