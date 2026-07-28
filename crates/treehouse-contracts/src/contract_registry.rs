use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    contract_definition::{ConsumerDependency, SubsystemContract},
    contract_migration::{build_migration_plan, ContractMigrationPlan},
    contract_observer::{
        detect_subsystem_contract_drift, ContractDriftReport, ObservedContractReality,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompatibilityRecord {
    pub from_version: String,
    pub to_version: String,
    pub breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractPublication {
    pub contract: SubsystemContract,
    pub consumers: Vec<String>,
    pub compatibility: Vec<CompatibilityRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContractRegistry {
    publications: BTreeMap<String, Vec<ContractPublication>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractChangeImpact {
    pub previous_version: String,
    pub next_version: String,
    pub drift_report: ContractDriftReport,
    pub impacted_consumers: Vec<String>,
    pub breaking: bool,
    pub migration_plan: ContractMigrationPlan,
}

impl ContractRegistry {
    pub fn publish(
        &mut self,
        contract: SubsystemContract,
        consumers: Vec<String>,
        compatibility: Vec<CompatibilityRecord>,
    ) {
        self.publications
            .entry(contract.subsystem.clone())
            .or_default()
            .push(ContractPublication {
                contract,
                consumers,
                compatibility,
            });
    }

    pub fn latest(&self, subsystem: &str) -> Option<&ContractPublication> {
        self.publications.get(subsystem).and_then(|entries| {
            entries
                .iter()
                .max_by(|left, right| left.contract.version.cmp(&right.contract.version))
        })
    }

    pub fn publications(&self, subsystem: &str) -> Vec<&ContractPublication> {
        self.publications
            .get(subsystem)
            .map(|entries| entries.iter().collect())
            .unwrap_or_default()
    }

    pub fn analyze_proposed_change(
        &self,
        previous: &SubsystemContract,
        next: &SubsystemContract,
        observed: &ObservedContractReality,
    ) -> ContractChangeImpact {
        let drift_report = detect_subsystem_contract_drift(next, observed);
        let impacted_consumers = impacted_consumers(
            &drift_report,
            &previous.consumer_dependencies,
            &next.consumer_dependencies,
        );
        let breaking = is_breaking_change(previous, next, &drift_report);
        let migration_plan = build_migration_plan(previous, next, &drift_report);

        ContractChangeImpact {
            previous_version: previous.version.clone(),
            next_version: next.version.clone(),
            drift_report,
            impacted_consumers,
            breaking,
            migration_plan,
        }
    }
}

fn impacted_consumers(
    drift_report: &ContractDriftReport,
    previous_dependencies: &[ConsumerDependency],
    next_dependencies: &[ConsumerDependency],
) -> Vec<String> {
    let mut by_name = BTreeMap::new();

    for dependency in previous_dependencies
        .iter()
        .chain(next_dependencies.iter())
        .map(|dependency| dependency.consumer.clone())
    {
        by_name.insert(dependency.clone(), dependency);
    }

    for consumer in drift_report
        .drifts
        .iter()
        .flat_map(|drift| drift.affected_consumers.iter())
    {
        by_name.insert(consumer.clone(), consumer.clone());
    }

    by_name.into_values().collect()
}

fn is_breaking_change(
    previous: &SubsystemContract,
    next: &SubsystemContract,
    drift_report: &ContractDriftReport,
) -> bool {
    if previous.version == next.version {
        return !drift_report.drifts.is_empty();
    }

    drift_report.drifts.iter().any(|drift| {
        drift.expected != drift.actual
            && (drift.message.contains("removed field")
                || drift.message.contains("missing endpoint")
                || drift.message.contains("missing event")
                || drift.message.contains("type"))
    })
}

#[cfg(test)]
mod tests {
    use crate::contract_definition::{
        ApiContract, ConsumerDependency, DataContract, FieldContract, OwnershipDeclaration,
        ProvidedInterfaces, SchemaContract, SubsystemContract,
    };

    use super::*;

    #[test]
    fn tracks_publications_and_change_impact() {
        let previous = SubsystemContract {
            subsystem: "Billing".to_string(),
            version: "2".to_string(),
            owns: OwnershipDeclaration {
                entities: vec!["Invoice".to_string()],
                workflows: vec![],
            },
            provides: ProvidedInterfaces {
                apis: vec![ApiContract {
                    method: "POST".to_string(),
                    path: "/invoice".to_string(),
                    request: SchemaContract::default(),
                    response: SchemaContract {
                        fields: vec![FieldContract {
                            name: "status".to_string(),
                            field_type: "string".to_string(),
                            required: true,
                        }],
                    },
                    authorization: None,
                    lifecycle: vec![],
                }],
                events: vec![],
            },
            data_contracts: vec![DataContract {
                entity: "User".to_string(),
                fields: vec![FieldContract {
                    name: "tenant_id".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                }],
            }],
            consumer_dependencies: vec![ConsumerDependency {
                consumer: "Orders".to_string(),
                dependency: "invoice.status".to_string(),
                expected_type: Some("string".to_string()),
            }],
            ..SubsystemContract::default()
        };

        let next = SubsystemContract {
            version: "3".to_string(),
            ..previous.clone()
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
                request: SchemaContract::default(),
                response: SchemaContract {
                    fields: vec![FieldContract {
                        name: "status".to_string(),
                        field_type: "object".to_string(),
                        required: true,
                    }],
                },
                authorization: None,
                lifecycle: vec![],
            }],
            ..ObservedContractReality::default()
        };

        let mut registry = ContractRegistry::default();
        registry.publish(
            previous.clone(),
            vec!["Orders".to_string()],
            vec![CompatibilityRecord {
                from_version: "1".to_string(),
                to_version: "2".to_string(),
                breaking: false,
            }],
        );
        registry.publish(
            next.clone(),
            vec!["Orders".to_string(), "Reporting".to_string()],
            vec![CompatibilityRecord {
                from_version: "2".to_string(),
                to_version: "3".to_string(),
                breaking: true,
            }],
        );

        let latest = registry.latest("Billing").unwrap();
        assert_eq!(latest.contract.version, "3");

        let impact = registry.analyze_proposed_change(&previous, &next, &observed);
        assert!(impact.breaking);
        assert!(impact
            .drift_report
            .drifts
            .iter()
            .any(|drift| drift.message.contains("removed field User.tenant_id")));
        assert!(impact
            .impacted_consumers
            .iter()
            .any(|consumer| consumer == "Orders"));
        assert!(!impact.migration_plan.steps.is_empty());
    }
}
