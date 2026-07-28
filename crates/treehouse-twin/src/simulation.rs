use serde::{Deserialize, Serialize};

use crate::bundle::{TwinBundle, WorkflowBehavior, WorkflowTransition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTwin {
    pub entities: Vec<String>,
    pub processes: Vec<String>,
    pub apis: Vec<String>,
    pub permissions: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimulationStep {
    pub input_event: String,
    pub from_state: String,
    pub to_state: String,
    pub transition_trigger: String,
    pub applied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimulationResult {
    pub workflow: String,
    pub deterministic: bool,
    pub initial_state: String,
    pub final_state: String,
    pub processed_events: Vec<String>,
    pub steps: Vec<SimulationStep>,
}

pub fn simulate_workflow(
    bundle: &TwinBundle,
    workflow_name: &str,
    events: &[String],
) -> SimulationResult {
    let workflow = find_workflow(bundle, workflow_name);
    let Some(workflow) = workflow else {
        return SimulationResult {
            workflow: workflow_name.to_string(),
            deterministic: true,
            initial_state: "unknown".to_string(),
            final_state: "unknown".to_string(),
            processed_events: events.to_vec(),
            steps: vec![SimulationStep {
                input_event: "n/a".to_string(),
                from_state: "unknown".to_string(),
                to_state: "unknown".to_string(),
                transition_trigger: "none".to_string(),
                applied: false,
                reason: "workflow not found in bundle".to_string(),
            }],
        };
    };

    let initial_state = initial_state(workflow);
    let mut current_state = initial_state.clone();
    let mut steps = Vec::new();

    for event in events {
        let candidate = select_transition(workflow, &current_state, event);
        match candidate {
            Some(transition) => {
                steps.push(SimulationStep {
                    input_event: event.clone(),
                    from_state: transition.from.clone(),
                    to_state: transition.to.clone(),
                    transition_trigger: transition.trigger.clone(),
                    applied: true,
                    reason: "matched transition deterministically".to_string(),
                });
                current_state = transition.to.clone();
            }
            None => {
                steps.push(SimulationStep {
                    input_event: event.clone(),
                    from_state: current_state.clone(),
                    to_state: current_state.clone(),
                    transition_trigger: "none".to_string(),
                    applied: false,
                    reason: "no valid transition from current state".to_string(),
                });
            }
        }
    }

    SimulationResult {
        workflow: workflow.name.clone(),
        deterministic: true,
        initial_state,
        final_state: current_state,
        processed_events: events.to_vec(),
        steps,
    }
}

pub fn deterministic_events_for_workflow(bundle: &TwinBundle, workflow_name: &str) -> Vec<String> {
    let Some(workflow) = find_workflow(bundle, workflow_name) else {
        return Vec::new();
    };
    let mut transitions = workflow.transitions.clone();
    transitions.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then(a.to.cmp(&b.to))
            .then(a.trigger.cmp(&b.trigger))
    });
    transitions.into_iter().map(|t| t.trigger).collect()
}

fn find_workflow<'a>(bundle: &'a TwinBundle, workflow_name: &str) -> Option<&'a WorkflowBehavior> {
    let workflow_l = workflow_name.to_ascii_lowercase();
    bundle
        .behavior
        .workflows
        .iter()
        .find(|workflow| workflow.name.to_ascii_lowercase() == workflow_l)
        .or_else(|| {
            bundle
                .behavior
                .workflows
                .iter()
                .find(|workflow| workflow.name.to_ascii_lowercase().contains(&workflow_l))
        })
}

fn initial_state(workflow: &WorkflowBehavior) -> String {
    if let Some(step) = workflow.steps.first() {
        return step.clone();
    }
    if let Some(transition) = workflow.transitions.first() {
        return transition.from.clone();
    }
    "unknown".to_string()
}

fn select_transition<'a>(
    workflow: &'a WorkflowBehavior,
    current_state: &str,
    event: &str,
) -> Option<&'a WorkflowTransition> {
    let event_l = event.to_ascii_lowercase();
    let mut candidates: Vec<&WorkflowTransition> = workflow
        .transitions
        .iter()
        .filter(|transition| transition.from.eq_ignore_ascii_case(current_state))
        .collect();
    candidates.sort_by(|a, b| a.to.cmp(&b.to).then(a.trigger.cmp(&b.trigger)));

    candidates
        .iter()
        .copied()
        .find(|transition| transition.trigger.eq_ignore_ascii_case(&event_l))
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .find(|transition| event_l.contains(&transition.to.to_ascii_lowercase()))
        })
        .or_else(|| {
            if event_l == "next" {
                candidates.first().copied()
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{
        ArchitectureModel, BehaviorModel, CapabilityModel, RuntimeModel, TwinBundle,
        WorkflowBehavior, WorkflowTransition,
    };

    #[test]
    fn deterministic_simulation_progresses_states() {
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
                    name: "Order".to_string(),
                    steps: vec!["created".to_string(), "paid".to_string()],
                    transitions: vec![WorkflowTransition {
                        from: "created".to_string(),
                        to: "paid".to_string(),
                        trigger: "created_to_paid".to_string(),
                        source: "workflow_transition".to_string(),
                    }],
                    inferred_from: vec!["application_model".to_string()],
                    confidence: 0.9,
                }],
                transitions: 1,
                executable_workflows: vec![],
                dataflows: vec![],
            },
            capability: CapabilityModel {
                capabilities: vec![],
                taxonomy: vec![],
            },
            runtime: RuntimeModel {
                architecture_confidence: 90,
                alarms: 0,
                health: vec![],
            },
        };

        let result = simulate_workflow(
            &bundle,
            "order",
            &["created_to_paid".to_string()],
        );
        assert_eq!(result.final_state, "paid");
        assert_eq!(result.steps.len(), 1);
        assert!(result.steps[0].applied);
    }
}
