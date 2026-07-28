use serde::{Deserialize, Serialize};

use crate::{
    bundle::{TwinBundle, WorkflowBehavior},
    simulation::{simulate_workflow, SimulationResult, SystemTwin},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactAnalysis {
    pub affected_database: Vec<String>,
    pub affected_apis: Vec<String>,
    pub affected_processes: Vec<String>,
    pub affected_ui: Vec<String>,
    pub risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProposedChange {
    RemoveState { workflow: String, state: String },
    RemoveTransition { workflow: String, from: String, to: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhatIfImpactReport {
    pub workflow: String,
    pub proposed_change: ProposedChange,
    pub baseline: SimulationResult,
    pub scenario: SimulationResult,
    pub broken_transitions: Vec<String>,
    pub impacted_capabilities: Vec<String>,
    pub risk_score: f32,
    pub risk_level: String,
}

pub fn analyze_status_field_removal(entity: &str, twin: &SystemTwin) -> ImpactAnalysis {
    let entity_lc = entity.to_ascii_lowercase();
    ImpactAnalysis {
        affected_database: twin
            .entities
            .iter()
            .filter(|candidate| candidate.to_ascii_lowercase() == entity_lc)
            .cloned()
            .collect(),
        affected_apis: twin
            .apis
            .iter()
            .filter(|api| api.to_ascii_lowercase().contains(&entity_lc))
            .cloned()
            .collect(),
        affected_processes: twin
            .processes
            .iter()
            .filter(|process| process.to_ascii_lowercase().contains(&entity_lc))
            .cloned()
            .collect(),
        affected_ui: vec![format!("{} workflow screen", entity)],
        risk: "high".to_string(),
    }
}

pub fn run_pre_change_what_if(
    bundle: &TwinBundle,
    workflow: &str,
    events: &[String],
    proposed_change: ProposedChange,
) -> WhatIfImpactReport {
    let baseline = simulate_workflow(bundle, workflow, events);
    let scenario_bundle = apply_change(bundle.clone(), &proposed_change);
    let scenario = simulate_workflow(&scenario_bundle, workflow, events);

    let broken_transitions = collect_broken_transitions(&baseline, &scenario);
    let impacted_capabilities = impacted_capabilities(bundle, workflow);

    let mut risk_score = 0.20_f32;
    risk_score += (broken_transitions.len() as f32 * 0.15).min(0.50);
    risk_score += (impacted_capabilities.len() as f32 * 0.05).min(0.20);
    if baseline.final_state != scenario.final_state {
        risk_score += 0.20;
    }
    let risk_score = risk_score.clamp(0.0, 0.99);

    let risk_level = if risk_score >= 0.75 {
        "high"
    } else if risk_score >= 0.45 {
        "medium"
    } else {
        "low"
    }
    .to_string();

    WhatIfImpactReport {
        workflow: workflow.to_string(),
        proposed_change,
        baseline,
        scenario,
        broken_transitions,
        impacted_capabilities,
        risk_score,
        risk_level,
    }
}

fn apply_change(mut bundle: TwinBundle, proposed_change: &ProposedChange) -> TwinBundle {
    match proposed_change {
        ProposedChange::RemoveState { workflow, state } => {
            if let Some(flow) = find_workflow_mut(&mut bundle, workflow) {
                flow.steps.retain(|step| !step.eq_ignore_ascii_case(state));
                flow.transitions.retain(|transition| {
                    !transition.from.eq_ignore_ascii_case(state)
                        && !transition.to.eq_ignore_ascii_case(state)
                });
            }
        }
        ProposedChange::RemoveTransition { workflow, from, to } => {
            if let Some(flow) = find_workflow_mut(&mut bundle, workflow) {
                flow.transitions.retain(|transition| {
                    !(transition.from.eq_ignore_ascii_case(from)
                        && transition.to.eq_ignore_ascii_case(to))
                });
            }
        }
    }
    bundle
}

fn find_workflow_mut<'a>(
    bundle: &'a mut TwinBundle,
    workflow: &str,
) -> Option<&'a mut WorkflowBehavior> {
    let workflow_l = workflow.to_ascii_lowercase();
    let exact = bundle
        .behavior
        .workflows
        .iter()
        .position(|flow| flow.name.to_ascii_lowercase() == workflow_l);
    let index = exact.or_else(|| {
        bundle
            .behavior
            .workflows
            .iter()
            .position(|flow| flow.name.to_ascii_lowercase().contains(&workflow_l))
    })?;
    bundle.behavior.workflows.get_mut(index)
}

fn collect_broken_transitions(
    baseline: &SimulationResult,
    scenario: &SimulationResult,
) -> Vec<String> {
    let mut broken = Vec::new();
    for (index, baseline_step) in baseline.steps.iter().enumerate() {
        let scenario_step = scenario.steps.get(index);
        if baseline_step.applied && scenario_step.map(|step| step.applied).unwrap_or(false) {
            continue;
        }
        if baseline_step.applied {
            broken.push(format!(
                "{} -> {} ({})",
                baseline_step.from_state, baseline_step.to_state, baseline_step.transition_trigger
            ));
        }
    }
    broken
}

fn impacted_capabilities(bundle: &TwinBundle, workflow: &str) -> Vec<String> {
    let workflow_l = workflow.to_ascii_lowercase();
    bundle
        .capability
        .capabilities
        .iter()
        .filter(|capability| {
            capability
                .name
                .to_ascii_lowercase()
                .contains(&workflow_l)
                || workflow_l.contains(&capability.intent.domain.to_ascii_lowercase())
        })
        .map(|capability| capability.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{
        ArchitectureModel, BehaviorModel, Capability, CapabilityModel, IntentProfile,
        RuntimeModel, TwinBundle, WorkflowBehavior, WorkflowTransition,
    };

    #[test]
    fn reports_high_impact_for_status_removal() {
        let twin = SystemTwin {
            entities: vec!["orders".to_string()],
            processes: vec!["order lifecycle".to_string()],
            apis: vec!["PATCH /orders/:id".to_string()],
            permissions: Vec::new(),
            events: Vec::new(),
        };

        let analysis = analyze_status_field_removal("orders", &twin);
        assert_eq!(analysis.risk, "high");
        assert!(analysis
            .affected_apis
            .iter()
            .any(|api| api == "PATCH /orders/:id"));
    }

    #[test]
    fn what_if_reports_broken_transition_risk() {
        let bundle = TwinBundle {
            bundle_version: "twin.v1".to_string(),
            repository: "repo".to_string(),
            generated_at_unix: 0,
            architecture: ArchitectureModel {
                nodes: 0,
                edges: 0,
                subsystems: vec![],
                apis: vec![],
                symbols: vec![],
            },
            behavior: BehaviorModel {
                workflows: vec![WorkflowBehavior {
                    name: "order".to_string(),
                    steps: vec!["created".to_string(), "paid".to_string()],
                    transitions: vec![WorkflowTransition {
                        from: "created".to_string(),
                        to: "paid".to_string(),
                        trigger: "created_to_paid".to_string(),
                        source: "workflow_transition".to_string(),
                    }],
                    inferred_from: vec![],
                    confidence: 0.9,
                }],
                transitions: 1,
                executable_workflows: vec![],
                dataflows: vec![],
            },
            capability: CapabilityModel {
                capabilities: vec![Capability {
                    id: "capability/order".to_string(),
                    name: "order".to_string(),
                    owner: None,
                    depends_on: vec![],
                    apis: vec![],
                    intent: IntentProfile {
                        domain: "order".to_string(),
                        intent: "orchestration".to_string(),
                        confidence: 0.8,
                        evidence: vec![],
                    },
                }],
                taxonomy: vec![],
            },
            runtime: RuntimeModel {
                architecture_confidence: 90,
                alarms: 0,
                health: vec![],
            },
        };

        let report = run_pre_change_what_if(
            &bundle,
            "order",
            &["created_to_paid".to_string()],
            ProposedChange::RemoveTransition {
                workflow: "order".to_string(),
                from: "created".to_string(),
                to: "paid".to_string(),
            },
        );
        assert!(!report.broken_transitions.is_empty());
        assert!(report.risk_score > 0.40);
    }
}
