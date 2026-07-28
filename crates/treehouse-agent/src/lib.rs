use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use treehouse_contracts::{
    detect_subsystem_contract_drift, ApiContract, ContractDriftKind, ContractDriftReport,
    DataContract, EventContract, ObservedContractReality, OwnershipContract, OwnershipDeclaration,
    ProcessContract, ProvidedInterfaces, SubsystemContract,
};
use treehouse_drift::{detect_drift, DriftEvent, DriftType, OwnershipPolicy};
use treehouse_system_graph::SystemGraphVersion;

const CONSUMES_RELATIONSHIP_TYPE: &str = "relationship";
const WORKFLOW_TRIGGER_SUFFIX: &str = ":trigger";
const WORKFLOW_COMPLETE_SUFFIX: &str = ":complete";
const UNPARSEABLE_API_METHOD: &str = "UNPARSEABLE";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileChangeEventKind {
    FileCreated,
    FileModified,
    FileDeleted,
    MigrationChanged,
    ApiChanged,
    ModelChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileChangeEvent {
    pub kind: FileChangeEventKind,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeveloperAlert {
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub affected_components: Vec<String>,
    pub suggested_fixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ArchitectureDiagrams {
    pub subsystem_map: Vec<String>,
    pub contract_map: Vec<String>,
    pub data_ownership_map: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureImpact {
    pub risk_level: RiskLevel,
    pub summary: String,
    pub affected_components: Vec<String>,
    pub breaking_contracts: Vec<String>,
    pub suggested_migration: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureChangeEvent {
    pub event: String,
    pub changes: Vec<String>,
    pub file_events: Vec<FileChangeEvent>,
    pub subsystem_contracts: Vec<SubsystemContract>,
    pub contract_drifts: Vec<ContractDriftReport>,
    pub impact: Option<ArchitectureImpact>,
    pub alerts: Vec<DeveloperAlert>,
    pub diagrams: ArchitectureDiagrams,
    pub drift_events: Vec<DriftEvent>,
}

pub fn detect_architecture_change(
    previous: Option<&SystemGraphVersion>,
    current: &SystemGraphVersion,
    ownership: &[OwnershipPolicy],
) -> Option<ArchitectureChangeEvent> {
    detect_architecture_change_with_files(previous, current, ownership, &[])
}

pub fn detect_architecture_change_with_files(
    previous: Option<&SystemGraphVersion>,
    current: &SystemGraphVersion,
    ownership: &[OwnershipPolicy],
    changed_files: &[String],
) -> Option<ArchitectureChangeEvent> {
    let drift_events = detect_drift(previous, current, ownership);
    let subsystem_contracts = infer_subsystem_contracts(current);
    let previous_contracts = previous.map(infer_subsystem_contracts).unwrap_or_default();
    let contract_drifts = detect_contract_drifts(&previous_contracts, &subsystem_contracts);
    let mut changes = Vec::new();

    if let Some(before) = previous {
        for subsystem in &current.subsystems {
            let existed = before.subsystems.iter().any(|old| old.id == subsystem.id);
            if !existed {
                changes.push(format!("new subsystem {}", subsystem.id));
            }
        }
    }

    changes.extend(detect_contract_shape_changes(previous, current));

    let file_events = classify_file_changes(changed_files);
    let diagrams = build_diagrams(current, &subsystem_contracts);
    let impact = build_impact(&drift_events, &contract_drifts);
    let alerts = build_alerts(
        &changes,
        &file_events,
        &drift_events,
        &contract_drifts,
        impact.as_ref(),
    );

    if changes.is_empty()
        && drift_events.is_empty()
        && contract_drifts.is_empty()
        && file_events.is_empty()
    {
        return None;
    }

    changes.sort();
    changes.dedup();

    Some(ArchitectureChangeEvent {
        event: "architecture_change".to_string(),
        changes,
        file_events,
        subsystem_contracts,
        contract_drifts,
        impact,
        alerts,
        diagrams,
        drift_events,
    })
}

pub fn infer_subsystem_contracts(graph: &SystemGraphVersion) -> Vec<SubsystemContract> {
    let mut consumes_by_subsystem: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for relationship in &graph.relationships {
        if let Some((left, right)) = relationship.split_once("->") {
            let source = left.trim().to_string();
            let target = right.trim().to_string();
            consumes_by_subsystem
                .entry(source)
                .or_default()
                .insert(target);
        }
    }

    let mut contracts: Vec<SubsystemContract> = graph
        .subsystems
        .iter()
        .map(|subsystem| {
            let apis = subsystem
                .apis
                .iter()
                .map(|api| {
                    let (method, path) = parse_api_signature(api);
                    ApiContract {
                        method,
                        path,
                        ..ApiContract::default()
                    }
                })
                .collect();
            let events = subsystem
                .events
                .iter()
                .map(|event| EventContract {
                    name: event.clone(),
                    ..EventContract::default()
                })
                .collect();
            let data_contracts = subsystem
                .entities
                .iter()
                .map(|entity| DataContract {
                    entity: entity.clone(),
                    fields: Vec::new(),
                })
                .collect();
            let process_contracts = subsystem
                .workflows
                .iter()
                .map(|workflow| ProcessContract {
                    trigger_event: format!("{workflow}{WORKFLOW_TRIGGER_SUFFIX}"),
                    must_eventually_emit: format!("{workflow}{WORKFLOW_COMPLETE_SUFFIX}"),
                })
                .collect();
            let ownership_contracts = subsystem
                .entities
                .iter()
                .map(|entity| OwnershipContract {
                    entity_field: format!("{entity}.*"),
                    owner_subsystem: subsystem.id.clone(),
                })
                .collect();
            let consumes = consumes_by_subsystem
                .get(subsystem.id.as_str())
                .map(|targets| {
                    targets
                        .iter()
                        .map(|target| {
                            (target.clone(), vec![CONSUMES_RELATIONSHIP_TYPE.to_string()])
                        })
                        .collect()
                })
                .unwrap_or_default();
            SubsystemContract {
                subsystem: subsystem.id.clone(),
                version: graph.version.to_string(),
                owns: OwnershipDeclaration {
                    entities: subsystem.entities.clone(),
                    workflows: subsystem.workflows.clone(),
                },
                provides: ProvidedInterfaces { apis, events },
                consumes,
                guarantees: subsystem
                    .workflows
                    .iter()
                    .map(|workflow| format!("{workflow} must complete"))
                    .collect(),
                data_contracts,
                process_contracts,
                ownership_contracts,
                consumer_dependencies: Vec::new(),
            }
        })
        .collect();
    contracts.sort_by(|left, right| left.subsystem.cmp(&right.subsystem));
    contracts
}

fn detect_contract_drifts(
    previous: &[SubsystemContract],
    current: &[SubsystemContract],
) -> Vec<ContractDriftReport> {
    let current_by_name: BTreeMap<&str, &SubsystemContract> = current
        .iter()
        .map(|contract| (contract.subsystem.as_str(), contract))
        .collect();
    let mut out = Vec::new();
    for declared in previous {
        let Some(observed) = current_by_name.get(declared.subsystem.as_str()) else {
            continue;
        };
        let report = detect_subsystem_contract_drift(
            declared,
            &ObservedContractReality {
                data_contracts: observed.data_contracts.clone(),
                apis: observed.provides.apis.clone(),
                events: observed.provides.events.clone(),
                process_emissions: observed.process_contracts.clone(),
                ownership_writes: observed
                    .ownership_contracts
                    .iter()
                    .map(|ownership| treehouse_contracts::OwnershipWriteObservation {
                        actor_subsystem: observed.subsystem.clone(),
                        entity_field: ownership.entity_field.clone(),
                    })
                    .collect(),
                violated_guarantees: Vec::new(),
            },
        );
        if !report.drifts.is_empty() {
            out.push(report);
        }
    }
    out
}

fn detect_contract_shape_changes(
    previous: Option<&SystemGraphVersion>,
    current: &SystemGraphVersion,
) -> Vec<String> {
    let Some(previous) = previous else {
        return Vec::new();
    };

    let mut previous_entities: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for subsystem in &previous.subsystems {
        previous_entities.insert(
            subsystem.id.as_str(),
            subsystem.entities.iter().map(String::as_str).collect(),
        );
    }

    let mut changes = Vec::new();
    for subsystem in &current.subsystems {
        let before = previous_entities
            .get(subsystem.id.as_str())
            .cloned()
            .unwrap_or_default();
        for entity in &subsystem.entities {
            if !before.contains(entity.as_str()) {
                changes.push(format!("{} new owned entity: {}", subsystem.id, entity));
            }
        }
    }
    changes
}

fn classify_file_changes(changed_files: &[String]) -> Vec<FileChangeEvent> {
    let mut out = BTreeSet::new();
    for changed in changed_files {
        let raw = changed.trim_end();
        if raw.trim().is_empty() {
            continue;
        }
        let (change_status, path) = if raw.chars().nth(2) == Some(' ') {
            (
                raw.get(..2).unwrap_or_default().trim().to_string(),
                raw.get(3..).unwrap_or_default().trim().to_string(),
            )
        } else {
            let mut parts = raw.split_whitespace();
            let status = parts.next().unwrap_or_default().to_string();
            let path = parts.collect::<Vec<_>>().join(" ");
            (status, path)
        };
        if path.is_empty() {
            continue;
        }

        let base_kind = if change_status == "??" || change_status.contains('A') {
            FileChangeEventKind::FileCreated
        } else if change_status.contains('D') {
            FileChangeEventKind::FileDeleted
        } else {
            FileChangeEventKind::FileModified
        };
        out.insert(FileChangeEvent {
            kind: base_kind,
            path: path.clone(),
        });

        if is_migration_path(path.as_str()) {
            out.insert(FileChangeEvent {
                kind: FileChangeEventKind::MigrationChanged,
                path: path.clone(),
            });
        }
        if is_api_definition_path(path.as_str()) {
            out.insert(FileChangeEvent {
                kind: FileChangeEventKind::ApiChanged,
                path: path.clone(),
            });
        }
        if is_model_definition_path(path.as_str()) {
            out.insert(FileChangeEvent {
                kind: FileChangeEventKind::ModelChanged,
                path,
            });
        }
    }
    out.into_iter().collect()
}

fn parse_api_signature(api: &str) -> (String, String) {
    let mut tokens = api.split_whitespace();
    let Some(method) = tokens.next() else {
        return (UNPARSEABLE_API_METHOD.to_string(), api.to_string());
    };
    let Some(path) = tokens.next() else {
        return (UNPARSEABLE_API_METHOD.to_string(), api.to_string());
    };
    (method.to_string(), path.to_string())
}

fn is_migration_path(path: &str) -> bool {
    let parsed = Path::new(path);
    let is_sql = parsed
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"));
    if !is_sql {
        return false;
    }
    parsed.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value.eq_ignore_ascii_case("migration") || value.eq_ignore_ascii_case("migrations")
    })
}

fn is_api_definition_path(path: &str) -> bool {
    let parsed = Path::new(path);
    let file_name = parsed
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        file_name.as_str(),
        "openapi.yaml"
            | "openapi.yml"
            | "openapi.json"
            | "swagger.yaml"
            | "swagger.yml"
            | "swagger.json"
    ) {
        return true;
    }

    let extension = parsed
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "yaml" | "yml" | "json") {
        return false;
    }
    parsed.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("api")
    })
}

fn is_model_definition_path(path: &str) -> bool {
    if is_migration_path(path) {
        return false;
    }

    let parsed = Path::new(path);
    let extension = parsed
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "json" | "yaml" | "yml" | "toml"
    ) {
        return false;
    }
    let stem = parsed
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stem.ends_with("_model")
        || stem.ends_with("_entity")
        || stem.ends_with("_schema")
        || stem.ends_with("_contract")
    {
        return true;
    }

    parsed.components().any(|component| {
        matches!(
            component
                .as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            "models" | "entities" | "schemas" | "contracts"
        )
    })
}

fn build_diagrams(
    graph: &SystemGraphVersion,
    contracts: &[SubsystemContract],
) -> ArchitectureDiagrams {
    let subsystem_map = graph.relationships.clone();
    let mut contract_map = Vec::new();
    let mut data_ownership_map = Vec::new();
    for contract in contracts {
        for api in &contract.provides.apis {
            contract_map.push(format!(
                "{} provides {} {}",
                contract.subsystem, api.method, api.path
            ));
        }
        for event in &contract.provides.events {
            contract_map.push(format!(
                "{} provides event {}",
                contract.subsystem, event.name
            ));
        }
        if !contract.owns.entities.is_empty() {
            data_ownership_map.push(format!(
                "{} owns {}",
                contract.subsystem,
                contract.owns.entities.join(", ")
            ));
        }
    }
    contract_map.sort();
    contract_map.dedup();
    data_ownership_map.sort();
    data_ownership_map.dedup();
    ArchitectureDiagrams {
        subsystem_map,
        contract_map,
        data_ownership_map,
    }
}

fn build_impact(
    drift_events: &[DriftEvent],
    contract_drifts: &[ContractDriftReport],
) -> Option<ArchitectureImpact> {
    let mut affected = BTreeSet::new();
    let mut breaking_contracts = BTreeSet::new();
    let mut suggested = BTreeSet::new();
    let mut high_risk = false;
    let mut medium_risk = false;

    for drift in drift_events {
        for subsystem in &drift.affected_subsystems {
            affected.insert(subsystem.clone());
        }
        for evidence in &drift.evidence {
            breaking_contracts.insert(evidence.clone());
        }
        suggested.insert(drift.recommendation.details.clone());
        if matches!(
            drift.drift_type,
            DriftType::OwnershipViolation | DriftType::ArchitectureDrift
        ) {
            high_risk = true;
        } else {
            medium_risk = true;
        }
    }

    for report in contract_drifts {
        if !report.drifts.is_empty() {
            affected.insert(report.subsystem.clone());
            high_risk = true;
        }
        for drift in &report.drifts {
            breaking_contracts.insert(drift.message.clone());
            for area in &drift.impacted_areas {
                suggested.insert(format!("Migrate consumers for {area}"));
            }
            if matches!(drift.kind, ContractDriftKind::Data | ContractDriftKind::Api) {
                suggested.insert(
                    "Add compatibility fields before removing existing contracts".to_string(),
                );
            }
        }
    }

    if affected.is_empty() && breaking_contracts.is_empty() {
        return None;
    }

    let risk_level = if high_risk {
        RiskLevel::High
    } else if medium_risk {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    let summary = match risk_level {
        RiskLevel::High => "HIGH RISK CHANGE: subsystem contracts drifted".to_string(),
        RiskLevel::Medium => "Moderate architecture impact detected".to_string(),
        RiskLevel::Low => "Low impact architecture change".to_string(),
    };

    Some(ArchitectureImpact {
        risk_level,
        summary,
        affected_components: affected.into_iter().collect(),
        breaking_contracts: breaking_contracts.into_iter().collect(),
        suggested_migration: suggested.into_iter().collect(),
    })
}

fn build_alerts(
    changes: &[String],
    file_events: &[FileChangeEvent],
    drift_events: &[DriftEvent],
    contract_drifts: &[ContractDriftReport],
    impact: Option<&ArchitectureImpact>,
) -> Vec<DeveloperAlert> {
    let mut alerts = Vec::new();

    if !changes.is_empty() || !file_events.is_empty() {
        alerts.push(DeveloperAlert {
            severity: AlertSeverity::Info,
            title: "New capability detected".to_string(),
            message: format!(
                "{} architecture deltas and {} file events observed",
                changes.len(),
                file_events.len()
            ),
            affected_components: Vec::new(),
            suggested_fixes: vec!["Review generated subsystem contracts".to_string()],
        });
    }

    for drift in drift_events {
        let severity = match drift.drift_type {
            DriftType::OwnershipViolation | DriftType::ArchitectureDrift => AlertSeverity::Critical,
            DriftType::DuplicateCapability
            | DriftType::SubsystemOverlap
            | DriftType::ModelFragmentation => AlertSeverity::Warning,
        };
        alerts.push(DeveloperAlert {
            severity,
            title: format!("{:?}", drift.drift_type),
            message: drift.recommendation.details.clone(),
            affected_components: drift.affected_subsystems.clone(),
            suggested_fixes: vec![drift.recommendation.action.clone()],
        });
    }

    for report in contract_drifts {
        for drift in &report.drifts {
            alerts.push(DeveloperAlert {
                severity: AlertSeverity::Critical,
                title: format!("Contract drift in {}", report.subsystem),
                message: drift.message.clone(),
                affected_components: drift.affected_consumers.clone(),
                suggested_fixes: drift.impacted_areas.clone(),
            });
        }
    }

    if let Some(impact) = impact {
        let severity = match impact.risk_level {
            RiskLevel::High => AlertSeverity::Critical,
            RiskLevel::Medium => AlertSeverity::Warning,
            RiskLevel::Low => AlertSeverity::Info,
        };
        alerts.push(DeveloperAlert {
            severity,
            title: "Architecture impact".to_string(),
            message: impact.summary.clone(),
            affected_components: impact.affected_components.clone(),
            suggested_fixes: impact.suggested_migration.clone(),
        });
    }

    alerts
}

#[cfg(test)]
mod tests {
    use treehouse_system_graph::{build_system_graph_version, Subsystem};

    use super::*;

    #[test]
    fn emits_architecture_change_event() {
        let previous = build_system_graph_version(
            1,
            vec![Subsystem {
                id: "Identity".to_string(),
                entities: vec!["User".to_string()],
                ..Subsystem::default()
            }],
            vec![],
        );
        let current = build_system_graph_version(
            2,
            vec![
                Subsystem {
                    id: "Identity".to_string(),
                    entities: vec!["User".to_string()],
                    ..Subsystem::default()
                },
                Subsystem {
                    id: "Billing".to_string(),
                    entities: vec!["Subscription".to_string()],
                    ..Subsystem::default()
                },
            ],
            vec![],
        );

        let event = detect_architecture_change(Some(&previous), &current, &[]).unwrap();
        assert_eq!(event.event, "architecture_change");
        assert!(event
            .changes
            .iter()
            .any(|change| change.contains("new subsystem Billing")));
        assert!(event
            .changes
            .iter()
            .any(|change| change.contains("Billing new owned entity: Subscription")));
        assert!(!event.subsystem_contracts.is_empty());
        assert!(event
            .diagrams
            .data_ownership_map
            .iter()
            .any(|entry| entry.contains("Billing owns Subscription")));
        assert!(event
            .alerts
            .iter()
            .any(|alert| matches!(alert.severity, AlertSeverity::Info)));
    }

    #[test]
    fn classifies_file_change_events() {
        let events = classify_file_changes(&[
            "?? migrations/001_create_invoices.sql".to_string(),
            " M api/openapi.yaml".to_string(),
            " D src/customer_model.rs".to_string(),
        ]);

        assert!(events.iter().any(|event| {
            event.kind == FileChangeEventKind::FileCreated
                && event.path == "migrations/001_create_invoices.sql"
        }));
        assert!(events.iter().any(|event| {
            event.kind == FileChangeEventKind::MigrationChanged
                && event.path == "migrations/001_create_invoices.sql"
        }));
        assert!(events.iter().any(|event| {
            event.kind == FileChangeEventKind::ApiChanged && event.path == "api/openapi.yaml"
        }));
        assert!(events.iter().any(|event| {
            event.kind == FileChangeEventKind::ModelChanged && event.path == "src/customer_model.rs"
        }));
    }

    #[test]
    fn reports_contract_drift_as_high_risk_impact() {
        let previous = build_system_graph_version(
            1,
            vec![Subsystem {
                id: "Billing".to_string(),
                entities: vec!["Invoice".to_string()],
                apis: vec!["GET /invoices".to_string()],
                ..Subsystem::default()
            }],
            vec![],
        );
        let current = build_system_graph_version(
            2,
            vec![Subsystem {
                id: "Billing".to_string(),
                entities: vec!["Payment".to_string()],
                apis: vec!["GET /payments".to_string()],
                ..Subsystem::default()
            }],
            vec![],
        );

        let event = detect_architecture_change_with_files(
            Some(&previous),
            &current,
            &[],
            &[" M src/billing_contract.rs".to_string()],
        )
        .unwrap();

        assert!(event.impact.is_some());
        assert_eq!(event.impact.unwrap().risk_level, RiskLevel::High);
        assert!(!event.contract_drifts.is_empty());
    }
}
