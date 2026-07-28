use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use treehouse_core::Document;

use crate::{
    edge::{GraphEdge, GraphEdgeKind},
    identity::{Identity, IdentityKind},
    node::{GraphNode, GraphNodeKind},
    observation::EntityObservation,
    schema::{EntityProfile, EntitySchema, FieldSchema, ValueKind},
};

#[derive(Debug, Clone, Copy)]
pub struct GraphSource<'a> {
    pub name: &'a str,
    pub document: &'a Document,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub from: String,
    pub to: String,
    pub kind: GraphEdgeKind,
    pub confidence: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UniversalDataGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub schemas: Vec<EntitySchema>,
    pub observations: Vec<EntityObservation>,
    pub relationships: Vec<Relationship>,
    pub intelligence: Vec<EntityProfile>,
}

impl UniversalDataGraph {
    pub fn build(sources: &[GraphSource<'_>]) -> Self {
        let mut collector = EntityCollector::default();

        for source in sources {
            collector.collect_source(source.name, source.document.root());
        }

        collector.finish()
    }
}

#[derive(Default)]
struct EntityCollector {
    entities: BTreeMap<String, EntityAggregate>,
    relationships: BTreeMap<(String, String, GraphEdgeKind), u8>,
}

#[derive(Default)]
struct EntityAggregate {
    samples: Vec<Map<String, Value>>,
    sources: BTreeSet<String>,
    sample_paths: Vec<String>,
}

#[derive(Default)]
struct FieldAggregate {
    present: usize,
    nulls: usize,
    type_counts: BTreeMap<ValueKind, usize>,
    timestamp_hits: usize,
    pii_hits: usize,
}

impl EntityCollector {
    fn collect_source(&mut self, source: &str, root: &Value) {
        match root {
            Value::Array(items) => {
                let samples = collect_object_items(items);
                if !samples.is_empty() {
                    let entity_name = singularize(normalize_name(source_stem(source)));
                    self.add_entity_samples(&entity_name, source, "$", samples);
                }
            }
            Value::Object(map) => {
                let mut added = false;
                for (key, value) in map {
                    match value {
                        Value::Array(items) => {
                            let samples = collect_object_items(items);
                            if !samples.is_empty() {
                                let entity_name = singularize(normalize_name(key));
                                let path = format!("$.{key}");
                                self.add_entity_samples(&entity_name, source, &path, samples);
                                added = true;
                            }
                        }
                        Value::Object(object) => {
                            let entity_name = singularize(normalize_name(key));
                            let path = format!("$.{key}");
                            self.add_entity_samples(
                                &entity_name,
                                source,
                                &path,
                                vec![object.clone()],
                            );
                            added = true;
                        }
                        _ => {}
                    }
                }

                if !added {
                    let entity_name = singularize(normalize_name(source_stem(source)));
                    self.add_entity_samples(&entity_name, source, "$", vec![map.clone()]);
                }
            }
            _ => {}
        }
    }

    fn add_entity_samples(
        &mut self,
        entity_name: &str,
        source: &str,
        path: &str,
        samples: Vec<Map<String, Value>>,
    ) {
        {
            let entry = self.entities.entry(entity_name.to_string()).or_default();
            entry.sources.insert(source.to_string());
            entry.sample_paths.push(path.to_string());
        }

        let mut prepared_samples = Vec::new();
        for sample in samples {
            self.inspect_nested(entity_name, &sample);
            prepared_samples.push(sample);
        }

        let entry = self.entities.entry(entity_name.to_string()).or_default();
        entry.samples.extend(prepared_samples);
    }

    fn inspect_nested(&mut self, parent_entity: &str, sample: &Map<String, Value>) {
        for (field, value) in sample {
            match value {
                Value::Array(items) => {
                    let nested_samples = collect_object_items(items);
                    if nested_samples.is_empty() {
                        continue;
                    }
                    let child_entity = singularize(normalize_name(field));
                    self.relationships.insert(
                        (
                            parent_entity.to_string(),
                            child_entity.clone(),
                            GraphEdgeKind::HasMany,
                        ),
                        97,
                    );
                    let nested = self.entities.entry(child_entity).or_default();
                    nested.samples.extend(nested_samples);
                }
                Value::Object(object) => {
                    let child_entity = singularize(normalize_name(field));
                    self.relationships.insert(
                        (
                            parent_entity.to_string(),
                            child_entity.clone(),
                            GraphEdgeKind::Related,
                        ),
                        90,
                    );
                    let nested = self.entities.entry(child_entity).or_default();
                    nested.samples.push(object.clone());
                }
                _ => {}
            }
        }
    }

    fn finish(self) -> UniversalDataGraph {
        let mut graph = UniversalDataGraph::default();
        let mut entity_names: Vec<_> = self.entities.keys().cloned().collect();
        entity_names.sort();

        for entity_name in &entity_names {
            graph.nodes.push(GraphNode {
                id: format!("entity:{entity_name}"),
                label: entity_name.clone(),
                kind: GraphNodeKind::Entity,
                properties: BTreeMap::new(),
            });
        }

        let mut relationships = self.relationships;

        for entity_name in &entity_names {
            let Some(aggregate) = self.entities.get(entity_name) else {
                continue;
            };

            let (schema, observation, profile, foreign_relations) =
                infer_entity(entity_name, aggregate, &entity_names);

            for relationship in foreign_relations {
                relationships.insert(
                    (
                        relationship.from.clone(),
                        relationship.to.clone(),
                        relationship.kind,
                    ),
                    relationship.confidence,
                );
            }

            for field in &schema.properties {
                graph.nodes.push(GraphNode {
                    id: format!("field:{entity_name}:{}", field.name),
                    label: field.name.clone(),
                    kind: GraphNodeKind::Field,
                    properties: BTreeMap::from([
                        ("type".to_string(), format!("{:?}", field.kind)),
                        (
                            "confidence".to_string(),
                            format!("{:.0}", field.confidence * 100.0),
                        ),
                    ]),
                });
                graph.edges.push(GraphEdge {
                    from: format!("entity:{entity_name}"),
                    to: format!("field:{entity_name}:{}", field.name),
                    kind: GraphEdgeKind::HasField,
                    label: "has field".to_string(),
                    confidence: field.confidence,
                });
            }

            for source in &profile.sources {
                let source_id = format!("source:{source}");
                if !graph.nodes.iter().any(|node| node.id == source_id) {
                    graph.nodes.push(GraphNode {
                        id: source_id.clone(),
                        label: source.clone(),
                        kind: GraphNodeKind::Source,
                        properties: BTreeMap::new(),
                    });
                }
                graph.edges.push(GraphEdge {
                    from: source_id,
                    to: format!("entity:{entity_name}"),
                    kind: GraphEdgeKind::DerivedFrom,
                    label: "derived from".to_string(),
                    confidence: profile.confidence,
                });
            }

            graph.schemas.push(schema);
            graph.observations.push(observation);
            graph.intelligence.push(profile);
        }

        let mut sorted_relationships: Vec<_> = relationships.into_iter().collect();
        sorted_relationships.sort_by(|(a, _), (b, _)| a.cmp(b));

        for ((from, to, kind), confidence) in sorted_relationships {
            graph.relationships.push(Relationship {
                from: from.clone(),
                to: to.clone(),
                kind,
                confidence,
            });
            graph.edges.push(GraphEdge {
                from: format!("entity:{from}"),
                to: format!("entity:{to}"),
                kind,
                label: relationship_label(kind),
                confidence: f32::from(confidence) / 100.0,
            });
        }

        graph
    }
}

fn infer_entity(
    entity_name: &str,
    aggregate: &EntityAggregate,
    entity_names: &[String],
) -> (
    EntitySchema,
    EntityObservation,
    EntityProfile,
    Vec<Relationship>,
) {
    let sample_count = aggregate.samples.len().max(1);
    let mut field_stats: BTreeMap<String, FieldAggregate> = BTreeMap::new();

    for sample in &aggregate.samples {
        for (field, value) in sample {
            let entry = field_stats.entry(field.clone()).or_default();
            entry.present += 1;
            if value.is_null() {
                entry.nulls += 1;
            }

            let kind = classify_kind(value);
            *entry.type_counts.entry(kind).or_insert(0) += 1;
            if is_temporal_field(field, value) {
                entry.timestamp_hits += 1;
            }
            if looks_like_pii(field) {
                entry.pii_hits += 1;
            }
        }
    }

    let mut properties = Vec::new();
    let mut pii_fields = Vec::new();

    for (name, stats) in field_stats {
        let kind = infer_kind(&stats.type_counts, stats.timestamp_hits > 0);
        let required_ratio = ratio(stats.present, sample_count);
        let nullable_ratio = ratio(stats.nulls, stats.present.max(1));
        let confidence = ((required_ratio * 0.6) + ((1.0 - nullable_ratio) * 0.4)).clamp(0.5, 0.99);
        let pii = stats.pii_hits > 0;
        let temporal = stats.timestamp_hits > 0 || matches!(kind, ValueKind::Timestamp);
        if pii {
            pii_fields.push(name.clone());
        }

        properties.push(FieldSchema {
            name,
            kind,
            required_ratio,
            nullable_ratio,
            confidence,
            pii,
            temporal,
        });
    }

    properties.sort_by(|a, b| a.name.cmp(&b.name));

    let identities = infer_identities(entity_name, &properties);
    let primary_identifier = identities
        .iter()
        .find(|identity| identity.kind == IdentityKind::Primary)
        .map(|identity| identity.field.clone());

    let entity_confidence = ((properties.len() as f32 / 6.0).min(1.0) * 0.4
        + ratio(aggregate.sources.len(), 2).min(1.0) * 0.3
        + if primary_identifier.is_some() {
            0.3
        } else {
            0.1
        })
    .clamp(0.5, 0.99);

    let mut foreign_relations = Vec::new();
    let mut related = BTreeSet::new();

    for property in &properties {
        if let Some(target) = infer_relationship_target(&property.name, entity_names, entity_name) {
            related.insert(target.clone());
            foreign_relations.push(Relationship {
                from: entity_name.to_string(),
                to: target,
                kind: GraphEdgeKind::BelongsTo,
                confidence: 94,
            });
        }
    }

    let related: Vec<String> = related.into_iter().collect();
    let sources: Vec<String> = aggregate.sources.iter().cloned().collect();

    let schema = EntitySchema {
        name: entity_name.to_string(),
        identities: identities.clone(),
        properties: properties.clone(),
        confidence: entity_confidence,
    };

    let observation = EntityObservation {
        entity: entity_name.to_string(),
        instances: aggregate.samples.len(),
        sources: aggregate.sources.clone(),
        sample_paths: aggregate.sample_paths.clone(),
    };

    let required_ratio = if properties.is_empty() {
        0.0
    } else {
        properties
            .iter()
            .map(|field| field.required_ratio)
            .sum::<f32>()
            / properties.len() as f32
    };

    let nullable_ratio = if properties.is_empty() {
        0.0
    } else {
        properties
            .iter()
            .map(|field| field.nullable_ratio)
            .sum::<f32>()
            / properties.len() as f32
    };

    let profile = EntityProfile {
        name: entity_name.to_string(),
        instances: aggregate.samples.len(),
        fields: properties.len(),
        primary_identifier,
        required_ratio,
        nullable_ratio,
        related,
        detected_pii: pii_fields,
        sources,
        confidence: entity_confidence,
    };

    (schema, observation, profile, foreign_relations)
}

fn infer_identities(entity_name: &str, properties: &[FieldSchema]) -> Vec<Identity> {
    let mut identities = Vec::new();
    let lower_entity = entity_name.to_lowercase();

    for field in properties {
        let lower = field.name.to_lowercase();
        if lower == "id"
            || lower == format!("{}_id", lower_entity)
            || lower == format!("{}id", lower_entity)
        {
            identities.push(Identity {
                field: field.name.clone(),
                kind: IdentityKind::Primary,
                confidence: 0.98,
            });
            return identities;
        }
    }

    for field in properties {
        let lower = field.name.to_lowercase();
        if lower.ends_with("_id") || lower.ends_with("id") {
            identities.push(Identity {
                field: field.name.clone(),
                kind: IdentityKind::Foreign,
                confidence: 0.90,
            });
        }
    }

    if identities.is_empty() {
        identities.push(Identity {
            field: "_synthetic".to_string(),
            kind: IdentityKind::Synthetic,
            confidence: 0.6,
        });
    }

    identities
}

fn infer_relationship_target(
    field: &str,
    entity_names: &[String],
    current: &str,
) -> Option<String> {
    let normalized = field
        .trim_end_matches("Id")
        .trim_end_matches("id")
        .trim_end_matches("_id")
        .to_lowercase();

    if normalized.is_empty() {
        return None;
    }

    entity_names
        .iter()
        .find(|name| name.to_lowercase() == normalized && *name != current)
        .cloned()
}

fn classify_kind(value: &Value) -> ValueKind {
    match value {
        Value::String(_) => ValueKind::String,
        Value::Number(_) => ValueKind::Number,
        Value::Bool(_) => ValueKind::Boolean,
        Value::Object(_) => ValueKind::Object,
        Value::Array(_) => ValueKind::Array,
        Value::Null => ValueKind::Null,
    }
}

fn infer_kind(type_counts: &BTreeMap<ValueKind, usize>, temporal_hint: bool) -> ValueKind {
    if temporal_hint {
        return ValueKind::Timestamp;
    }

    if type_counts.len() > 1 {
        return ValueKind::Mixed;
    }

    type_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(kind, _)| *kind)
        .unwrap_or(ValueKind::Mixed)
}

fn ratio(count: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }

    count as f32 / total as f32
}

fn collect_object_items(items: &[Value]) -> Vec<Map<String, Value>> {
    items
        .iter()
        .filter_map(|item| match item {
            Value::Object(map) => Some(map.clone()),
            _ => None,
        })
        .collect()
}

fn source_stem(source: &str) -> &str {
    source
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or(source)
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(source)
}

fn normalize_name(raw: &str) -> String {
    let trimmed = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if trimmed.is_empty() {
        return "Entity".to_string();
    }

    let mut out = String::new();
    let mut capitalize = true;
    for ch in trimmed.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            capitalize = true;
            continue;
        }

        if capitalize {
            out.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }

    if out.is_empty() {
        "Entity".to_string()
    } else {
        out
    }
}

fn singularize(name: String) -> String {
    if name.ends_with("ies") && name.len() > 3 {
        return format!("{}y", &name[..name.len() - 3]);
    }

    if name.ends_with('s') && name.len() > 1 {
        return name[..name.len() - 1].to_string();
    }

    name
}

fn looks_like_pii(field: &str) -> bool {
    let lower = field.to_lowercase();
    ["email", "phone", "address", "ssn", "dob"]
        .iter()
        .any(|token| lower.contains(token))
}

fn is_temporal_field(field: &str, value: &Value) -> bool {
    let lower = field.to_lowercase();
    if lower.contains("created")
        || lower.contains("updated")
        || lower.contains("deleted")
        || lower.contains("timestamp")
        || lower.ends_with("_at")
    {
        return true;
    }

    if let Value::String(v) = value {
        let bytes = v.as_bytes();
        return bytes.len() >= 10
            && bytes.get(4) == Some(&b'-')
            && bytes.get(7) == Some(&b'-')
            && (v.contains('T') || v.contains(':'));
    }

    false
}

fn relationship_label(kind: GraphEdgeKind) -> String {
    match kind {
        GraphEdgeKind::DerivedFrom => "derived from".to_string(),
        GraphEdgeKind::HasField => "has field".to_string(),
        GraphEdgeKind::HasMany => "has many".to_string(),
        GraphEdgeKind::BelongsTo => "belongs to".to_string(),
        GraphEdgeKind::Related => "related".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use treehouse_core::Document;

    use super::*;

    #[test]
    fn infers_customer_schema_and_relationships() {
        let doc = Document::new(
            json!({
                "customers": [
                    {
                        "customerId": "123",
                        "name": "Bob",
                        "email": "bob@example.com",
                        "created_at": "2026-01-01T00:00:00Z",
                        "orders": [
                            {
                                "orderId": "abc",
                                "total": 99
                            }
                        ]
                    }
                ]
            }),
            0,
        );

        let graph = UniversalDataGraph::build(&[GraphSource {
            name: "customers.json",
            document: &doc,
        }]);

        assert!(graph.schemas.iter().any(|schema| schema.name == "Customer"));
        assert!(graph.schemas.iter().any(|schema| schema.name == "Order"));

        let customer = graph
            .schemas
            .iter()
            .find(|schema| schema.name == "Customer")
            .unwrap();
        assert!(customer.identities.iter().any(
            |identity| identity.field == "customerId" && identity.kind == IdentityKind::Primary
        ));
        assert!(customer
            .properties
            .iter()
            .any(|field| field.name == "created_at" && field.temporal));

        assert!(graph.relationships.iter().any(|relationship| {
            relationship.from == "Customer"
                && relationship.to == "Order"
                && relationship.kind == GraphEdgeKind::HasMany
        }));
    }

    #[test]
    fn detects_entities_across_sources() {
        let customers = Document::new(json!([{"id": "c1", "email": "a@b.com"}]), 0);
        let orders = Document::new(json!([{"id": "o1", "customerId": "c1"}]), 0);
        let payments = Document::new(json!([{"id": "p1", "orderId": "o1"}]), 0);

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
                name: "payments.json",
                document: &payments,
            },
        ]);

        assert!(graph.schemas.iter().any(|schema| schema.name == "Customer"));
        assert!(graph.schemas.iter().any(|schema| schema.name == "Order"));
        assert!(graph.schemas.iter().any(|schema| schema.name == "Payment"));

        assert!(graph.relationships.iter().any(|relationship| {
            relationship.from == "Order"
                && relationship.to == "Customer"
                && relationship.kind == GraphEdgeKind::BelongsTo
                && relationship.confidence >= 90
        }));

        assert!(graph.relationships.iter().any(|relationship| {
            relationship.from == "Payment"
                && relationship.to == "Order"
                && relationship.kind == GraphEdgeKind::BelongsTo
                && relationship.confidence >= 90
        }));

        let customer_profile = graph
            .intelligence
            .iter()
            .find(|profile| profile.name == "Customer")
            .unwrap();
        assert!(customer_profile
            .detected_pii
            .iter()
            .any(|field| field == "email"));
    }
}
