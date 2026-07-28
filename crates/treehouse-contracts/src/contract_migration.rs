use serde::{Deserialize, Serialize};

use crate::{
    contract_definition::SubsystemContract,
    contract_observer::{ContractDriftKind, ContractDriftReport},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContractMigrationStep {
    pub title: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContractMigrationPlan {
    pub subsystem: String,
    pub from_version: String,
    pub to_version: String,
    pub steps: Vec<ContractMigrationStep>,
}

pub fn build_migration_plan(
    previous: &SubsystemContract,
    next: &SubsystemContract,
    drift_report: &ContractDriftReport,
) -> ContractMigrationPlan {
    let mut steps = Vec::new();

    if !drift_report.drifts.is_empty() {
        steps.push(ContractMigrationStep {
            title: "Run impact analysis".to_string(),
            details: format!(
                "Analyze {} detected drift events before rollout.",
                drift_report.drifts.len()
            ),
        });
    }

    if drift_report
        .drifts
        .iter()
        .any(|drift| drift.kind == ContractDriftKind::Data)
    {
        steps.push(ContractMigrationStep {
            title: "Introduce compatibility fields".to_string(),
            details:
                "Ship dual-read/write support for renamed/removed fields before final removal."
                    .to_string(),
        });
    }

    if drift_report
        .drifts
        .iter()
        .any(|drift| drift.kind == ContractDriftKind::Api || drift.kind == ContractDriftKind::Event)
    {
        steps.push(ContractMigrationStep {
            title: "Version external interfaces".to_string(),
            details:
                "Expose both previous and next contract forms during migration to avoid consumer outages."
                    .to_string(),
        });
    }

    if drift_report
        .drifts
        .iter()
        .any(|drift| drift.kind == ContractDriftKind::Process)
    {
        steps.push(ContractMigrationStep {
            title: "Backfill missing process emissions".to_string(),
            details:
                "Ensure required downstream events are emitted or replayed for in-flight entities."
                    .to_string(),
        });
    }

    if drift_report
        .drifts
        .iter()
        .any(|drift| drift.kind == ContractDriftKind::Ownership)
    {
        steps.push(ContractMigrationStep {
            title: "Restore ownership boundaries".to_string(),
            details: "Move unauthorized writes behind the owning subsystem API.".to_string(),
        });
    }

    if steps.is_empty() {
        steps.push(ContractMigrationStep {
            title: "No migration required".to_string(),
            details: "No drift detected; rollout can proceed directly.".to_string(),
        });
    }

    ContractMigrationPlan {
        subsystem: next.subsystem.clone(),
        from_version: previous.version.clone(),
        to_version: next.version.clone(),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        contract_definition::SubsystemContract,
        contract_observer::{ContractDrift, ContractDriftKind, ContractDriftReport},
    };

    use super::*;

    #[test]
    fn creates_actionable_steps_for_drift_types() {
        let previous = SubsystemContract {
            subsystem: "Billing".to_string(),
            version: "2".to_string(),
            ..SubsystemContract::default()
        };
        let next = SubsystemContract {
            subsystem: "Billing".to_string(),
            version: "3".to_string(),
            ..SubsystemContract::default()
        };
        let report = ContractDriftReport {
            subsystem: "Billing".to_string(),
            drifts: vec![
                ContractDrift {
                    kind: ContractDriftKind::Data,
                    message: "removed field".to_string(),
                    expected: "tenant_id".to_string(),
                    actual: "organization_id".to_string(),
                    affected_consumers: vec![],
                    impacted_areas: vec![],
                },
                ContractDrift {
                    kind: ContractDriftKind::Ownership,
                    message: "ownership violation".to_string(),
                    expected: "Billing".to_string(),
                    actual: "Orders".to_string(),
                    affected_consumers: vec![],
                    impacted_areas: vec![],
                },
            ],
        };

        let plan = build_migration_plan(&previous, &next, &report);
        assert_eq!(plan.subsystem, "Billing");
        assert!(plan
            .steps
            .iter()
            .any(|step| step.title == "Run impact analysis"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step.title == "Introduce compatibility fields"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step.title == "Restore ownership boundaries"));
    }
}
