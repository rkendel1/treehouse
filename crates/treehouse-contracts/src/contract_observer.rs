use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::contract_definition::{
    ApiContract, ConsumerDependency, DataContract, EventContract, ProcessContract, SchemaContract,
    SubsystemContract,
};

// Two shared tokens (e.g., `invoice` + `status`) are required to map a drift key
// to a consumer dependency without requiring exact string matching.
const MIN_TOKEN_OVERLAP: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractDriftKind {
    Data,
    Api,
    Event,
    Process,
    Ownership,
    Guarantee,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractDrift {
    pub kind: ContractDriftKind,
    pub message: String,
    pub expected: String,
    pub actual: String,
    pub affected_consumers: Vec<String>,
    pub impacted_areas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContractDriftReport {
    pub subsystem: String,
    pub drifts: Vec<ContractDrift>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OwnershipWriteObservation {
    pub actor_subsystem: String,
    pub entity_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ObservedContractReality {
    pub data_contracts: Vec<DataContract>,
    pub apis: Vec<ApiContract>,
    pub events: Vec<EventContract>,
    pub process_emissions: Vec<ProcessContract>,
    pub ownership_writes: Vec<OwnershipWriteObservation>,
    pub violated_guarantees: Vec<String>,
}

pub fn detect_subsystem_contract_drift(
    declared: &SubsystemContract,
    observed: &ObservedContractReality,
) -> ContractDriftReport {
    let mut drifts = Vec::new();

    drifts.extend(detect_data_drift(
        &declared.data_contracts,
        &observed.data_contracts,
        &declared.consumer_dependencies,
    ));
    drifts.extend(detect_api_drift(
        &declared.provides.apis,
        &observed.apis,
        &declared.consumer_dependencies,
    ));
    drifts.extend(detect_event_drift(
        &declared.provides.events,
        &observed.events,
        &declared.consumer_dependencies,
    ));
    drifts.extend(detect_process_drift(
        &declared.process_contracts,
        &observed.process_emissions,
    ));
    drifts.extend(detect_ownership_drift(
        declared.subsystem.as_str(),
        &declared.ownership_contracts,
        &observed.ownership_writes,
    ));
    drifts.extend(detect_guarantee_drift(
        &declared.guarantees,
        &observed.violated_guarantees,
    ));

    ContractDriftReport {
        subsystem: declared.subsystem.clone(),
        drifts,
    }
}

fn detect_data_drift(
    declared: &[DataContract],
    observed: &[DataContract],
    dependencies: &[ConsumerDependency],
) -> Vec<ContractDrift> {
    let observed_by_entity: BTreeMap<&str, &DataContract> = observed
        .iter()
        .map(|contract| (contract.entity.as_str(), contract))
        .collect();

    let mut drifts = Vec::new();
    for expected in declared {
        let expected_entity = expected.entity.to_ascii_lowercase();
        let Some(actual) = observed_by_entity.get(expected.entity.as_str()).copied() else {
            drifts.push(ContractDrift {
                kind: ContractDriftKind::Data,
                message: format!("missing observed entity {}", expected.entity),
                expected: expected.entity.clone(),
                actual: "<missing>".to_string(),
                affected_consumers: impacted_consumers(
                    dependencies,
                    expected_entity.as_str(),
                    None,
                ),
                impacted_areas: vec!["entity availability".to_string()],
            });
            continue;
        };

        let actual_fields: BTreeMap<&str, &str> = actual
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.field_type.as_str()))
            .collect();

        for field in &expected.fields {
            match actual_fields.get(field.name.as_str()) {
                None => drifts.push(ContractDrift {
                    kind: ContractDriftKind::Data,
                    message: format!("removed field {}.{}", expected.entity, field.name),
                    expected: field.field_type.clone(),
                    actual: "<missing>".to_string(),
                    affected_consumers: impacted_consumers(
                        dependencies,
                        &format!(
                            "{}.{}",
                            expected.entity.to_ascii_lowercase(),
                            field.name.to_ascii_lowercase()
                        ),
                        Some(field.field_type.as_str()),
                    ),
                    impacted_areas: vec!["authorization".to_string(), "reporting".to_string()],
                }),
                Some(actual_type) if *actual_type != field.field_type => {
                    drifts.push(ContractDrift {
                        kind: ContractDriftKind::Data,
                        message: format!("field type mismatch {}.{}", expected.entity, field.name),
                        expected: field.field_type.clone(),
                        actual: (*actual_type).to_string(),
                        affected_consumers: impacted_consumers(
                            dependencies,
                            &format!(
                                "{}.{}",
                                expected.entity.to_ascii_lowercase(),
                                field.name.to_ascii_lowercase()
                            ),
                            Some(field.field_type.as_str()),
                        ),
                        impacted_areas: vec!["data compatibility".to_string()],
                    })
                }
                _ => {}
            }
        }
    }

    drifts
}

fn detect_api_drift(
    declared: &[ApiContract],
    observed: &[ApiContract],
    dependencies: &[ConsumerDependency],
) -> Vec<ContractDrift> {
    let observed_by_key: BTreeMap<(String, String), &ApiContract> = observed
        .iter()
        .map(|api| ((api.method.clone(), api.path.clone()), api))
        .collect();

    let mut drifts = Vec::new();
    for expected in declared {
        let endpoint = format!("{} {}", expected.method, expected.path);
        let endpoint_lower = endpoint.to_ascii_lowercase();
        let key = (expected.method.clone(), expected.path.clone());
        let Some(actual) = observed_by_key.get(&key).copied() else {
            drifts.push(ContractDrift {
                kind: ContractDriftKind::Api,
                message: format!("missing endpoint {} {}", expected.method, expected.path),
                expected: endpoint.clone(),
                actual: "<missing>".to_string(),
                affected_consumers: impacted_consumers(dependencies, endpoint_lower.as_str(), None),
                impacted_areas: vec!["integration".to_string()],
            });
            continue;
        };

        drifts.extend(compare_schema_contract(
            ContractDriftKind::Api,
            format!("{} {} request", expected.method, expected.path),
            &expected.request,
            &actual.request,
            dependencies,
        ));
        drifts.extend(compare_schema_contract(
            ContractDriftKind::Api,
            format!("{} {} response", expected.method, expected.path),
            &expected.response,
            &actual.response,
            dependencies,
        ));

        if expected.authorization != actual.authorization {
            drifts.push(ContractDrift {
                kind: ContractDriftKind::Api,
                message: format!(
                    "authorization drift for {} {}",
                    expected.method, expected.path
                ),
                expected: expected.authorization.clone().unwrap_or_default(),
                actual: actual.authorization.clone().unwrap_or_default(),
                affected_consumers: impacted_consumers(dependencies, endpoint_lower.as_str(), None),
                impacted_areas: vec!["authorization".to_string()],
            });
        }
    }

    drifts
}

fn detect_event_drift(
    declared: &[EventContract],
    observed: &[EventContract],
    dependencies: &[ConsumerDependency],
) -> Vec<ContractDrift> {
    let observed_by_name: BTreeMap<&str, &EventContract> = observed
        .iter()
        .map(|event| (event.name.as_str(), event))
        .collect();

    let mut drifts = Vec::new();
    for expected in declared {
        let event_name = expected.name.to_ascii_lowercase();
        let Some(actual) = observed_by_name.get(expected.name.as_str()).copied() else {
            drifts.push(ContractDrift {
                kind: ContractDriftKind::Event,
                message: format!("missing event {}", expected.name),
                expected: expected.name.clone(),
                actual: "<missing>".to_string(),
                affected_consumers: impacted_consumers(dependencies, event_name.as_str(), None),
                impacted_areas: vec!["async processing".to_string()],
            });
            continue;
        };

        drifts.extend(compare_schema_contract(
            ContractDriftKind::Event,
            format!("event {} payload", expected.name),
            &expected.payload,
            &actual.payload,
            dependencies,
        ));

        if expected.ordering_key != actual.ordering_key {
            drifts.push(ContractDrift {
                kind: ContractDriftKind::Event,
                message: format!("event ordering changed {}", expected.name),
                expected: expected.ordering_key.clone().unwrap_or_default(),
                actual: actual.ordering_key.clone().unwrap_or_default(),
                affected_consumers: impacted_consumers(dependencies, event_name.as_str(), None),
                impacted_areas: vec!["event ordering".to_string()],
            });
        }
    }

    drifts
}

fn detect_process_drift(
    declared: &[ProcessContract],
    observed: &[ProcessContract],
) -> Vec<ContractDrift> {
    let observed_pairs: BTreeSet<(String, String)> = observed
        .iter()
        .map(|process| {
            (
                process.trigger_event.clone(),
                process.must_eventually_emit.clone(),
            )
        })
        .collect();

    declared
        .iter()
        .filter_map(|process| {
            let key = (
                process.trigger_event.clone(),
                process.must_eventually_emit.clone(),
            );
            if observed_pairs.contains(&key) {
                return None;
            }
            Some(ContractDrift {
                kind: ContractDriftKind::Process,
                message: format!(
                    "process guarantee missing {} -> {}",
                    process.trigger_event, process.must_eventually_emit
                ),
                expected: format!(
                    "{} -> {}",
                    process.trigger_event, process.must_eventually_emit
                ),
                actual: "not observed".to_string(),
                affected_consumers: Vec::new(),
                impacted_areas: vec!["workflow consistency".to_string()],
            })
        })
        .collect()
}

fn detect_ownership_drift(
    owner_subsystem: &str,
    declared: &[crate::contract_definition::OwnershipContract],
    observed: &[OwnershipWriteObservation],
) -> Vec<ContractDrift> {
    let owner_by_field: BTreeMap<&str, &str> = declared
        .iter()
        .map(|ownership| {
            (
                ownership.entity_field.as_str(),
                ownership.owner_subsystem.as_str(),
            )
        })
        .collect();

    observed
        .iter()
        .filter_map(|write| {
            let expected_owner = owner_by_field.get(write.entity_field.as_str()).copied()?;
            if expected_owner == write.actor_subsystem || write.actor_subsystem == owner_subsystem {
                return None;
            }

            Some(ContractDrift {
                kind: ContractDriftKind::Ownership,
                message: format!(
                    "ownership violation on {} by {}",
                    write.entity_field, write.actor_subsystem
                ),
                expected: expected_owner.to_string(),
                actual: write.actor_subsystem.clone(),
                affected_consumers: Vec::new(),
                impacted_areas: vec!["boundary integrity".to_string()],
            })
        })
        .collect()
}

fn detect_guarantee_drift(declared: &[String], violated: &[String]) -> Vec<ContractDrift> {
    let declared_set: BTreeSet<&str> = declared.iter().map(String::as_str).collect();

    violated
        .iter()
        .filter(|guarantee| declared_set.contains(guarantee.as_str()))
        .map(|guarantee| ContractDrift {
            kind: ContractDriftKind::Guarantee,
            message: format!("guarantee violated: {guarantee}"),
            expected: guarantee.clone(),
            actual: "violation observed".to_string(),
            affected_consumers: Vec::new(),
            impacted_areas: vec!["system guarantees".to_string()],
        })
        .collect()
}

fn compare_schema_contract(
    kind: ContractDriftKind,
    scope: String,
    expected: &SchemaContract,
    actual: &SchemaContract,
    dependencies: &[ConsumerDependency],
) -> Vec<ContractDrift> {
    let actual_by_name: BTreeMap<&str, &str> = actual
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field.field_type.as_str()))
        .collect();

    let scope_lower = scope.to_ascii_lowercase();
    expected
        .fields
        .iter()
        .filter_map(|field| {
            let Some(actual_type) = actual_by_name.get(field.name.as_str()) else {
                return Some(ContractDrift {
                    kind: kind.clone(),
                    message: format!("removed field {}.{}", scope, field.name),
                    expected: field.field_type.clone(),
                    actual: "<missing>".to_string(),
                    affected_consumers: impacted_consumers(
                        dependencies,
                        format!("{}.{}", scope_lower, field.name.to_ascii_lowercase()).as_str(),
                        Some(field.field_type.as_str()),
                    ),
                    impacted_areas: vec!["schema compatibility".to_string()],
                });
            };

            if *actual_type == field.field_type {
                return None;
            }

            Some(ContractDrift {
                kind: kind.clone(),
                message: format!("field type changed {}.{}", scope, field.name),
                expected: field.field_type.clone(),
                actual: (*actual_type).to_string(),
                affected_consumers: impacted_consumers(
                    dependencies,
                    format!("{}.{}", scope_lower, field.name.to_ascii_lowercase()).as_str(),
                    Some(field.field_type.as_str()),
                ),
                impacted_areas: vec!["schema compatibility".to_string()],
            })
        })
        .collect()
}

fn impacted_consumers(
    dependencies: &[ConsumerDependency],
    dependency_key: &str,
    expected_type: Option<&str>,
) -> Vec<String> {
    let key = normalize_key(dependency_key);

    dependencies
        .iter()
        .filter(|dependency| {
            let dependency_name = normalize_key(&dependency.dependency);
            dependency_name.contains(&key)
                || key.contains(&dependency_name)
                || has_token_overlap(&key, &dependency_name)
        })
        .filter(|dependency| {
            expected_type.is_none_or(|kind| {
                dependency
                    .expected_type
                    .as_deref()
                    .is_none_or(|expected| expected.eq_ignore_ascii_case(kind))
            })
        })
        .map(|dependency| dependency.consumer.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_key(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len() + 1);
    let mut previous_was_sep = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_sep = false;
        } else if !previous_was_sep {
            normalized.push('.');
            previous_was_sep = true;
        }
    }

    if normalized.starts_with('.') {
        normalized.remove(0);
    }
    if normalized.ends_with('.') {
        normalized.pop();
    }

    normalized
}

fn has_token_overlap(left: &str, right: &str) -> bool {
    let left_tokens: BTreeSet<&str> = left.split('.').filter(|token| !token.is_empty()).collect();
    let right_tokens: BTreeSet<&str> = right.split('.').filter(|token| !token.is_empty()).collect();

    left_tokens.intersection(&right_tokens).count() >= MIN_TOKEN_OVERLAP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_definition::{
        ApiContract, ConsumerDependency, DataContract, EventContract, FieldContract,
        OwnershipContract, OwnershipDeclaration, ProcessContract, ProvidedInterfaces,
        SchemaContract, SubsystemContract,
    };

    #[test]
    fn detects_contract_drift_with_consumer_impact() {
        let declared = SubsystemContract {
            subsystem: "Billing".to_string(),
            version: "3".to_string(),
            owns: OwnershipDeclaration {
                entities: vec!["Invoice".to_string()],
                workflows: vec!["Collect Payment".to_string()],
            },
            provides: ProvidedInterfaces {
                apis: vec![ApiContract {
                    method: "POST".to_string(),
                    path: "/invoice".to_string(),
                    request: SchemaContract {
                        fields: vec![FieldContract {
                            name: "customer_id".to_string(),
                            field_type: "string".to_string(),
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
                    lifecycle: vec!["invoice-created".to_string()],
                }],
                events: vec![EventContract {
                    name: "PaymentCompleted".to_string(),
                    payload: SchemaContract {
                        fields: vec![FieldContract {
                            name: "amount".to_string(),
                            field_type: "number".to_string(),
                            required: true,
                        }],
                    },
                    ordering_key: Some("payment_id".to_string()),
                }],
            },
            guarantees: vec!["Invoice has immutable total".to_string()],
            data_contracts: vec![DataContract {
                entity: "User".to_string(),
                fields: vec![FieldContract {
                    name: "tenant_id".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                }],
            }],
            process_contracts: vec![ProcessContract {
                trigger_event: "OrderCreated".to_string(),
                must_eventually_emit: "PaymentRequested".to_string(),
            }],
            ownership_contracts: vec![OwnershipContract {
                entity_field: "Invoice.status".to_string(),
                owner_subsystem: "Billing".to_string(),
            }],
            consumer_dependencies: vec![
                ConsumerDependency {
                    consumer: "Orders".to_string(),
                    dependency: "invoice.status:string".to_string(),
                    expected_type: Some("string".to_string()),
                },
                ConsumerDependency {
                    consumer: "Reporting".to_string(),
                    dependency: "user.tenant_id".to_string(),
                    expected_type: Some("string".to_string()),
                },
            ],
            ..SubsystemContract::default()
        };

        let observed = ObservedContractReality {
            data_contracts: vec![DataContract {
                entity: "User".to_string(),
                fields: vec![FieldContract {
                    name: "organization_id".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                }],
            }],
            apis: vec![ApiContract {
                method: "POST".to_string(),
                path: "/invoice".to_string(),
                request: SchemaContract {
                    fields: vec![FieldContract {
                        name: "customer_id".to_string(),
                        field_type: "string".to_string(),
                        required: true,
                    }],
                },
                response: SchemaContract {
                    fields: vec![FieldContract {
                        name: "status".to_string(),
                        field_type: "object".to_string(),
                        required: true,
                    }],
                },
                authorization: Some("organization-context".to_string()),
                lifecycle: vec![],
            }],
            events: vec![EventContract {
                name: "PaymentCompleted".to_string(),
                payload: SchemaContract {
                    fields: vec![FieldContract {
                        name: "amount".to_string(),
                        field_type: "number".to_string(),
                        required: true,
                    }],
                },
                ordering_key: Some("invoice_id".to_string()),
            }],
            process_emissions: vec![],
            ownership_writes: vec![OwnershipWriteObservation {
                actor_subsystem: "Orders".to_string(),
                entity_field: "Invoice.status".to_string(),
            }],
            violated_guarantees: vec!["Invoice has immutable total".to_string()],
        };

        let report = detect_subsystem_contract_drift(&declared, &observed);

        assert!(!report.drifts.is_empty());
        assert!(report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("removed field User.tenant_id")));
        assert!(report.drifts.iter().any(|drift| drift
            .message
            .contains("field type changed POST /invoice response.status")));
        assert!(report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("authorization drift")));
        assert!(report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("process guarantee missing")));
        assert!(report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("ownership violation")));
        assert!(report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("guarantee violated")));
        assert!(report.drifts.iter().any(|drift| {
            drift
                .affected_consumers
                .iter()
                .any(|consumer| consumer == "Orders")
        }));
    }
}
