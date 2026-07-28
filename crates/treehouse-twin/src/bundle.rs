use serde::{Deserialize, Serialize};
use treehouse_application_model::ApplicationModel;
use treehouse_system_graph::{KnowledgeGraph, KnowledgeNodeType};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TwinBundle {
    pub bundle_version: String,
    pub repository: String,
    pub generated_at_unix: u64,
    pub architecture: ArchitectureModel,
    pub behavior: BehaviorModel,
    pub capability: CapabilityModel,
    pub runtime: RuntimeModel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureModel {
    pub nodes: usize,
    pub edges: usize,
    pub subsystems: Vec<String>,
    pub apis: Vec<String>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehaviorModel {
    pub workflows: Vec<WorkflowBehavior>,
    pub transitions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowBehavior {
    pub name: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityModel {
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub owner: Option<String>,
    pub depends_on: Vec<String>,
    pub apis: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeModel {
    pub architecture_confidence: u8,
    pub alarms: usize,
    pub health: Vec<RuntimeHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeHealth {
    pub subsystem: String,
    pub overall: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeProjection {
    pub architecture_confidence: u8,
    #[serde(default)]
    pub health: Vec<RuntimeProjectionHealth>,
    #[serde(default)]
    pub alarms: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeProjectionHealth {
    pub subsystem: String,
    pub overall: u8,
}

pub fn build_twin_bundle(
    repository: &str,
    generated_at_unix: u64,
    model: &ApplicationModel,
    knowledge: &KnowledgeGraph,
    runtime: &RuntimeProjection,
) -> TwinBundle {
    let mut subsystems = Vec::new();
    let mut apis = Vec::new();
    let mut symbols = Vec::new();
    let mut capabilities = Vec::new();

    for node in &knowledge.nodes {
        match node.node_type {
            KnowledgeNodeType::Subsystem => subsystems.push(node.name.clone()),
            KnowledgeNodeType::Api => apis.push(node.name.clone()),
            KnowledgeNodeType::Symbol => symbols.push(node.name.clone()),
            KnowledgeNodeType::Capability => capabilities.push(Capability {
                id: node.id.clone(),
                name: node.name.clone(),
                owner: node.owner.clone(),
                depends_on: Vec::new(),
                apis: Vec::new(),
            }),
            _ => {}
        }
    }

    for edge in &knowledge.edges {
        if let Some(capability) = capabilities.iter_mut().find(|cap| cap.id == edge.to) {
            capability.depends_on.push(edge.from.clone());
        }
        if let Some(capability) = capabilities.iter_mut().find(|cap| cap.id == edge.from) {
            capability.depends_on.push(edge.to.clone());
        }
        if edge.from.starts_with("api/") {
            if let Some(capability) = capabilities.iter_mut().find(|cap| cap.id == edge.to) {
                capability.apis.push(edge.from.clone());
            }
        }
    }

    for capability in &mut capabilities {
        capability.depends_on.sort();
        capability.depends_on.dedup();
        capability.apis.sort();
        capability.apis.dedup();
    }

    let workflows: Vec<WorkflowBehavior> = model
        .workflows
        .iter()
        .map(|workflow| WorkflowBehavior {
            name: workflow.entity.clone(),
            steps: workflow.states.clone(),
        })
        .collect();

    let transitions = model
        .workflows
        .iter()
        .map(|workflow| workflow.transitions.len())
        .sum();

    TwinBundle {
        bundle_version: "twin.v1".to_string(),
        repository: repository.to_string(),
        generated_at_unix,
        architecture: ArchitectureModel {
            nodes: knowledge.nodes.len(),
            edges: knowledge.edges.len(),
            subsystems: dedupe_sorted(subsystems),
            apis: dedupe_sorted(apis),
            symbols: dedupe_sorted(symbols),
        },
        behavior: BehaviorModel {
            workflows,
            transitions,
        },
        capability: CapabilityModel { capabilities },
        runtime: RuntimeModel {
            architecture_confidence: runtime.architecture_confidence,
            alarms: runtime.alarms.len(),
            health: runtime
                .health
                .iter()
                .map(|h| RuntimeHealth {
                    subsystem: h.subsystem.clone(),
                    overall: h.overall,
                })
                .collect(),
        },
    }
}

pub fn capability_similarity(a: &TwinBundle, b: &TwinBundle) -> f32 {
    use std::collections::BTreeSet;
    let a_set: BTreeSet<&str> = a
        .capability
        .capabilities
        .iter()
        .map(|cap| cap.name.as_str())
        .collect();
    let b_set: BTreeSet<&str> = b
        .capability
        .capabilities
        .iter()
        .map(|cap| cap.name.as_str())
        .collect();
    if a_set.is_empty() && b_set.is_empty() {
        return 1.0;
    }
    let overlap = a_set.intersection(&b_set).count() as f32;
    let union = a_set.union(&b_set).count() as f32;
    if union == 0.0 { 0.0 } else { overlap / union }
}

pub fn execute_capability(bundle: &TwinBundle, capability: &str) -> Vec<String> {
    let capability_l = capability.to_ascii_lowercase();
    if let Some(found) = bundle
        .capability
        .capabilities
        .iter()
        .find(|cap| cap.name.to_ascii_lowercase().contains(&capability_l))
    {
        let mut steps = Vec::new();
        steps.push(format!("Capability: {}", found.name));
        if let Some(owner) = &found.owner {
            steps.push(format!("Owner: {}", owner));
        }
        for api in &found.apis {
            steps.push(format!("Exposes: {}", api));
        }
        for dependency in &found.depends_on {
            steps.push(format!("Depends on: {}", dependency));
        }
        if steps.len() == 1 {
            steps.push("No executable steps inferred yet.".to_string());
        }
        return steps;
    }

    let mut inferred = Vec::new();
    for workflow in &bundle.behavior.workflows {
        if workflow.name.to_ascii_lowercase().contains(&capability_l) {
            inferred.push(format!("Workflow: {}", workflow.name));
            for step in &workflow.steps {
                inferred.push(format!(" -> {}", step));
            }
        }
    }
    if inferred.is_empty() {
        vec![format!(
            "No capability or workflow match for '{}'.",
            capability
        )]
    } else {
        inferred
    }
}

fn dedupe_sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
