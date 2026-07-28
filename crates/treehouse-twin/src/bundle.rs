use serde::{Deserialize, Serialize};
use treehouse_application_model::ApplicationModel;
use treehouse_system_graph::{KnowledgeEdgeType, KnowledgeGraph, KnowledgeNodeType};

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
    #[serde(default)]
    pub executable_workflows: Vec<ExecutableWorkflow>,
    #[serde(default)]
    pub dataflows: Vec<DataflowEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowBehavior {
    pub name: String,
    pub steps: Vec<String>,
    #[serde(default)]
    pub transitions: Vec<WorkflowTransition>,
    #[serde(default)]
    pub inferred_from: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTransition {
    pub from: String,
    pub to: String,
    pub trigger: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutableWorkflow {
    pub name: String,
    pub confidence: f32,
    pub steps: Vec<ExecutableStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutableStep {
    pub action: String,
    #[serde(default)]
    pub consumes: Vec<String>,
    #[serde(default)]
    pub emits: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataflowEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityModel {
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub taxonomy: Vec<TaxonomyBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub owner: Option<String>,
    pub depends_on: Vec<String>,
    pub apis: Vec<String>,
    pub intent: IntentProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentProfile {
    pub domain: String,
    pub intent: String,
    pub confidence: f32,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaxonomyBucket {
    pub domain: String,
    pub count: usize,
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
    let mut workflow_signals = Vec::new();
    let mut runtime_event_signals = Vec::new();

    for node in &knowledge.nodes {
        match node.node_type {
            KnowledgeNodeType::Subsystem => subsystems.push(node.name.clone()),
            KnowledgeNodeType::Api => apis.push(node.name.clone()),
            KnowledgeNodeType::Symbol => symbols.push(node.name.clone()),
            KnowledgeNodeType::Workflow => workflow_signals.push(node.name.clone()),
            KnowledgeNodeType::RuntimeEvent => runtime_event_signals.push(node.name.clone()),
            KnowledgeNodeType::Capability => capabilities.push(Capability {
                id: node.id.clone(),
                name: node.name.clone(),
                owner: node.owner.clone(),
                depends_on: Vec::new(),
                apis: Vec::new(),
                intent: IntentProfile {
                    domain: "unknown".to_string(),
                    intent: "unknown".to_string(),
                    confidence: 0.0,
                    evidence: Vec::new(),
                },
            }),
            _ => {}
        }
    }

    let mut dataflows = Vec::new();

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

        if matches!(
            edge.edge_type,
            KnowledgeEdgeType::DependsOn
                | KnowledgeEdgeType::Exposes
                | KnowledgeEdgeType::Produces
                | KnowledgeEdgeType::Observes
        ) {
            dataflows.push(DataflowEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                relation: format!("{:?}", edge.edge_type),
                confidence: edge.confidence,
            });
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
        .map(|workflow| {
            let mut inferred_from = vec!["application_model".to_string()];
            let mut transitions = Vec::new();
            for transition in &workflow.transitions {
                for target in &transition.allowed {
                    transitions.push(WorkflowTransition {
                        from: transition.from.clone(),
                        to: target.clone(),
                        trigger: format!("{}_to_{}", transition.from, target),
                        source: "workflow_transition".to_string(),
                    });
                }
            }

            for signal in &workflow_signals {
                if signal
                    .to_ascii_lowercase()
                    .contains(&workflow.entity.to_ascii_lowercase())
                {
                    inferred_from.push(format!("workflow_signal:{signal}"));
                }
            }

            let event_transitions = infer_event_transitions(&workflow.entity, &runtime_event_signals);
            if !event_transitions.is_empty() {
                inferred_from.push("runtime_events".to_string());
                transitions.extend(event_transitions);
            }

            let confidence = workflow_confidence(&workflow.states, &transitions, &inferred_from);

            WorkflowBehavior {
                name: workflow.entity.clone(),
                steps: workflow.states.clone(),
                transitions: dedupe_transitions(transitions),
                inferred_from: dedupe_sorted(inferred_from),
                confidence,
            }
        })
        .collect();

    for capability in &mut capabilities {
        capability.intent = infer_capability_intent(capability, &workflows);
    }

    let taxonomy = build_taxonomy(&capabilities);

    let executable_workflows = build_executable_workflows(&workflows, &capabilities);

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
            executable_workflows,
            dataflows,
        },
        capability: CapabilityModel {
            capabilities,
            taxonomy,
        },
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
        steps.push(format!(
            "Intent: {}/{} ({:.0}% confidence)",
            found.intent.domain,
            found.intent.intent,
            found.intent.confidence * 100.0
        ));
        for evidence in &found.intent.evidence {
            steps.push(format!("Intent evidence: {}", evidence));
        }
        for api in &found.apis {
            steps.push(format!("Exposes: {}", api));
        }
        for dependency in &found.depends_on {
            steps.push(format!("Depends on: {}", dependency));
        }
        for workflow in &bundle.behavior.executable_workflows {
            if workflow
                .name
                .to_ascii_lowercase()
                .contains(&capability_l)
            {
                steps.push(format!(
                    "Executable workflow: {} ({:.0}% confidence)",
                    workflow.name,
                    workflow.confidence * 100.0
                ));
                for exec_step in &workflow.steps {
                    steps.push(format!(" -> {}", exec_step.action));
                }
            }
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

fn infer_event_transitions(workflow_name: &str, runtime_events: &[String]) -> Vec<WorkflowTransition> {
    let workflow_token = workflow_name.to_ascii_lowercase();
    let mut ordered_states = Vec::new();

    for event in runtime_events {
        let event_l = event.to_ascii_lowercase();
        if !event_l.contains(&workflow_token) {
            continue;
        }
        for token in tokenize_signal(&event_l) {
            if token == workflow_token || token.len() < 3 {
                continue;
            }
            if is_common_noise_token(&token) {
                continue;
            }
            ordered_states.push(token);
        }
    }

    ordered_states = dedupe_preserve_order(ordered_states);
    let mut inferred = Vec::new();
    for pair in ordered_states.windows(2) {
        inferred.push(WorkflowTransition {
            from: pair[0].clone(),
            to: pair[1].clone(),
            trigger: format!("event_{}_to_{}", pair[0], pair[1]),
            source: "runtime_event_inference".to_string(),
        });
    }
    inferred
}

fn workflow_confidence(
    states: &[String],
    transitions: &[WorkflowTransition],
    inferred_from: &[String],
) -> f32 {
    let mut score = 0.40_f32;
    if !states.is_empty() {
        score += 0.20;
    }
    if !transitions.is_empty() {
        score += (transitions.len() as f32 * 0.03).min(0.20);
    }
    score += (inferred_from.len() as f32 * 0.05).min(0.20);
    score.clamp(0.0, 0.98)
}

fn build_executable_workflows(
    workflows: &[WorkflowBehavior],
    capabilities: &[Capability],
) -> Vec<ExecutableWorkflow> {
    workflows
        .iter()
        .map(|workflow| {
            let workflow_l = workflow.name.to_ascii_lowercase();
            let capability_deps: Vec<String> = capabilities
                .iter()
                .filter(|capability| {
                    capability
                        .name
                        .to_ascii_lowercase()
                        .contains(&workflow_l)
                        || workflow_l.contains(&capability.name.to_ascii_lowercase())
                })
                .map(|capability| capability.id.clone())
                .collect();

            let mut steps = Vec::new();
            for transition in &workflow.transitions {
                steps.push(ExecutableStep {
                    action: format!(
                        "{} -> {} ({})",
                        transition.from, transition.to, transition.trigger
                    ),
                    consumes: vec![format!("state:{}", transition.from)],
                    emits: vec![format!("state:{}", transition.to)],
                    depends_on: capability_deps.clone(),
                });
            }

            if steps.is_empty() {
                for step in &workflow.steps {
                    steps.push(ExecutableStep {
                        action: format!("enter state {step}"),
                        consumes: Vec::new(),
                        emits: vec![format!("state:{step}")],
                        depends_on: capability_deps.clone(),
                    });
                }
            }

            ExecutableWorkflow {
                name: workflow.name.clone(),
                confidence: workflow.confidence,
                steps,
            }
        })
        .collect()
}

fn infer_capability_intent(
    capability: &Capability,
    workflows: &[WorkflowBehavior],
) -> IntentProfile {
    let name_l = capability.name.to_ascii_lowercase();
    let domain = infer_domain(&name_l, &capability.apis, &capability.depends_on);
    let intent = infer_intent(&name_l, &capability.apis, &capability.depends_on);

    let workflow_hits = workflows
        .iter()
        .filter(|workflow| {
            workflow
                .name
                .to_ascii_lowercase()
                .contains(&name_l)
                || name_l.contains(&workflow.name.to_ascii_lowercase())
        })
        .count();

    let mut evidence = Vec::new();
    evidence.push(format!("domain:{domain}"));
    evidence.push(format!("intent:{intent}"));
    if let Some(owner) = &capability.owner {
        evidence.push(format!("owner:{owner}"));
    }
    if !capability.apis.is_empty() {
        evidence.push(format!("apis:{}", capability.apis.len()));
    }
    if !capability.depends_on.is_empty() {
        evidence.push(format!("dependencies:{}", capability.depends_on.len()));
    }
    if workflow_hits > 0 {
        evidence.push(format!("workflow_hits:{workflow_hits}"));
    }

    let mut confidence = 0.35_f32;
    if capability.owner.is_some() {
        confidence += 0.10;
    }
    confidence += (capability.apis.len() as f32 * 0.05).min(0.20);
    confidence += (capability.depends_on.len() as f32 * 0.03).min(0.15);
    if workflow_hits > 0 {
        confidence += 0.15;
    }
    if domain != "core" {
        confidence += 0.10;
    }
    if intent != "orchestration" {
        confidence += 0.10;
    }

    IntentProfile {
        domain,
        intent,
        confidence: confidence.clamp(0.0, 0.99),
        evidence,
    }
}

fn infer_domain(name_l: &str, apis: &[String], depends_on: &[String]) -> String {
    infer_label(
        name_l,
        apis,
        depends_on,
        &[
            ("identity", &["auth", "identity", "token", "session"]),
            ("billing", &["invoice", "payment", "billing", "charge"]),
            ("order", &["order", "cart", "checkout", "shipment"]),
            ("catalog", &["catalog", "product", "inventory"]),
            ("search", &["search", "query", "index"]),
            ("notification", &["notify", "email", "sms", "webhook"]),
            ("analytics", &["metric", "analytics", "report", "insight"]),
            ("integration", &["provider", "adapter", "connector", "sync"]),
            ("workflow", &["workflow", "state", "orchestr"]),
        ],
        "core",
    )
}

fn infer_intent(name_l: &str, apis: &[String], depends_on: &[String]) -> String {
    infer_label(
        name_l,
        apis,
        depends_on,
        &[
            ("query", &["list", "get", "search", "read", "find"]),
            ("command", &["create", "update", "write", "emit", "publish"]),
            ("integration", &["provider", "adapter", "sync", "import", "export"]),
            ("policy", &["policy", "auth", "permission", "guard"]),
            ("orchestration", &["workflow", "orchestr", "process", "pipeline"]),
        ],
        "orchestration",
    )
}

fn infer_label(
    name_l: &str,
    apis: &[String],
    depends_on: &[String],
    candidates: &[(&str, &[&str])],
    fallback: &str,
) -> String {
    let mut best = fallback.to_string();
    let mut best_score = 0_usize;
    for (label, keywords) in candidates {
        let mut score = keyword_hits(name_l, keywords);
        for api in apis {
            score += keyword_hits(&api.to_ascii_lowercase(), keywords);
        }
        for dep in depends_on {
            score += keyword_hits(&dep.to_ascii_lowercase(), keywords);
        }
        if score > best_score {
            best = (*label).to_string();
            best_score = score;
        }
    }
    best
}

fn keyword_hits(haystack: &str, keywords: &[&str]) -> usize {
    keywords
        .iter()
        .filter(|keyword| haystack.contains(**keyword))
        .count()
}

fn build_taxonomy(capabilities: &[Capability]) -> Vec<TaxonomyBucket> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for capability in capabilities {
        *counts.entry(capability.intent.domain.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(domain, count)| TaxonomyBucket { domain, count })
        .collect()
}

fn dedupe_transitions(mut transitions: Vec<WorkflowTransition>) -> Vec<WorkflowTransition> {
    transitions.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then(a.to.cmp(&b.to))
            .then(a.trigger.cmp(&b.trigger))
            .then(a.source.cmp(&b.source))
    });
    transitions.dedup_by(|a, b| {
        a.from == b.from && a.to == b.to && a.trigger == b.trigger && a.source == b.source
    });
    transitions
}

fn tokenize_signal(raw: &str) -> Vec<String> {
    raw.split(|c: char| !(c.is_ascii_alphanumeric()))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn is_common_noise_token(token: &str) -> bool {
    matches!(
        token,
        "event"
            | "runtime"
            | "service"
            | "worker"
            | "job"
            | "trace"
            | "log"
            | "api"
            | "http"
            | "request"
            | "response"
    )
}

fn dedupe_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn dedupe_sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
