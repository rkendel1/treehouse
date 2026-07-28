use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use treehouse_application_model::{ApiEndpoint, ApplicationModel, CrudOperation, Entity, Field};
use treehouse_system_graph::SystemGraphVersion;
use treehouse_target::ScanTarget;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: String,
    pub subsystem: String,
    pub description: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub title: String,
    pub details: String,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub summary: String,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmProvenance {
    pub backend: String,
    pub confidence: f32,
    pub grounded_entities: Vec<String>,
    pub grounded_subsystems: Vec<String>,
    pub prompt_excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetArchitecture {
    pub model: ApplicationModel,
    pub goals: Vec<Goal>,
    pub plan: Plan,
    pub provenance: LlmProvenance,
}

#[derive(Debug, Clone)]
pub enum LocalLlmBackend {
    Heuristic,
    Ollama { model: String },
}

impl LocalLlmBackend {
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(value) = raw else {
            return Self::Heuristic;
        };
        let lowered = value.to_ascii_lowercase();
        if lowered == "heuristic" || lowered == "true" || lowered == "1" {
            Self::Heuristic
        } else if lowered.starts_with("ollama:") {
            Self::Ollama {
                model: value["ollama:".len()..].to_string(),
            }
        } else {
            Self::Ollama {
                model: value.to_string(),
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            LocalLlmBackend::Heuristic => "heuristic".to_string(),
            LocalLlmBackend::Ollama { model } => format!("ollama:{model}"),
        }
    }
}

pub fn infer_target_architecture(
    baseline: &ApplicationModel,
    system_graph: &SystemGraphVersion,
    target: &ScanTarget,
    backend: LocalLlmBackend,
) -> Result<TargetArchitecture> {
    let prompt = build_grounded_prompt(baseline, system_graph, target);
    let generated = match backend {
        LocalLlmBackend::Heuristic => infer_heuristic(baseline, system_graph, target),
        LocalLlmBackend::Ollama { model } => {
            infer_with_ollama(baseline, system_graph, target, &model)?
        }
    };

    Ok(TargetArchitecture {
        model: generated.model,
        goals: generated.goals,
        plan: generated.plan,
        provenance: LlmProvenance {
            backend: generated.backend,
            confidence: generated.confidence,
            grounded_entities: baseline
                .entities
                .iter()
                .map(|entity| entity.name.clone())
                .collect(),
            grounded_subsystems: system_graph
                .subsystems
                .iter()
                .map(|subsystem| subsystem.id.clone())
                .collect(),
            prompt_excerpt: prompt.chars().take(400).collect(),
        },
    })
}

struct Inferred {
    model: ApplicationModel,
    goals: Vec<Goal>,
    plan: Plan,
    confidence: f32,
    backend: String,
}

fn infer_heuristic(
    baseline: &ApplicationModel,
    system_graph: &SystemGraphVersion,
    target: &ScanTarget,
) -> Inferred {
    let mut model = baseline.clone();
    let known_entities: Vec<String> = model
        .entities
        .iter()
        .map(|entity| entity.name.to_ascii_lowercase())
        .collect();

    for capability in &target.desired_capabilities {
        let candidate_name = extract_entity_name(capability);
        if candidate_name.is_empty() {
            continue;
        }
        if known_entities
            .iter()
            .any(|existing| existing == &candidate_name.to_ascii_lowercase())
        {
            continue;
        }
        model.entities.push(Entity {
            name: candidate_name.clone(),
            confidence: 0.65,
            fields: vec![
                Field {
                    name: "id".to_string(),
                    field_type: "string".to_string(),
                    required: true,
                    primary: true,
                    unique: true,
                    confidence: 0.7,
                },
                Field {
                    name: "created_at".to_string(),
                    field_type: "datetime".to_string(),
                    required: false,
                    primary: false,
                    unique: false,
                    confidence: 0.6,
                },
            ],
            relationships: vec![],
            constraints: vec![],
        });
        model.api.push(ApiEndpoint {
            method: "POST".to_string(),
            path: format!("/{}/", pluralize_path(&candidate_name)),
            operation: CrudOperation::Create,
            entity: candidate_name,
        });
    }

    let subsystem_fallback = system_graph
        .subsystems
        .first()
        .map(|subsystem| subsystem.id.clone())
        .unwrap_or_else(|| "Platform".to_string());

    let mut goals = Vec::new();
    for (idx, capability) in target.desired_capabilities.iter().enumerate() {
        goals.push(Goal {
            id: format!("goal-{:02}", idx + 1),
            subsystem: subsystem_fallback.clone(),
            description: capability.clone(),
            priority: (idx as u8).saturating_add(1),
        });
    }

    for (idx, constraint) in target.constraints.iter().enumerate() {
        goals.push(Goal {
            id: format!("constraint-{:02}", idx + 1),
            subsystem: subsystem_fallback.clone(),
            description: format!("Constraint: {constraint}"),
            priority: 1,
        });
    }

    let plan = build_plan(target, &goals);

    Inferred {
        model,
        goals,
        plan,
        confidence: 0.72,
        backend: "heuristic".to_string(),
    }
}

fn infer_with_ollama(
    baseline: &ApplicationModel,
    system_graph: &SystemGraphVersion,
    target: &ScanTarget,
    model: &str,
) -> Result<Inferred> {
    let prompt = build_grounded_prompt(baseline, system_graph, target);
    let output = Command::new("ollama")
        .args(["run", model, &prompt])
        .output()
        .with_context(|| format!("failed to start ollama for model `{model}`"))?;

    if !output.status.success() {
        bail!(
            "ollama call failed for model `{model}`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_ollama_response(&text, baseline, target);
    Ok(Inferred {
        model: parsed.0,
        goals: parsed.1,
        plan: parsed.2,
        confidence: 0.8,
        backend: format!("ollama:{model}"),
    })
}

fn parse_ollama_response(
    content: &str,
    baseline: &ApplicationModel,
    target: &ScanTarget,
) -> (ApplicationModel, Vec<Goal>, Plan) {
    let mut model = baseline.clone();
    let mut goals = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(goal) = trimmed.strip_prefix("GOAL:") {
            goals.push(Goal {
                id: format!("goal-{:02}", goals.len() + 1),
                subsystem: "Platform".to_string(),
                description: goal.trim().to_string(),
                priority: (idx as u8).saturating_add(1),
            });
        }
        if let Some(entity) = trimmed.strip_prefix("ENTITY:") {
            let name = extract_entity_name(entity);
            if !name.is_empty()
                && !model
                    .entities
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(&name))
            {
                model.entities.push(Entity {
                    name: name.clone(),
                    confidence: 0.7,
                    fields: vec![],
                    relationships: vec![],
                    constraints: vec![],
                });
                model.api.push(ApiEndpoint {
                    method: "POST".to_string(),
                    path: format!("/{}/", pluralize_path(&name)),
                    operation: CrudOperation::Create,
                    entity: name,
                });
            }
        }
    }

    if goals.is_empty() {
        goals = target
            .desired_capabilities
            .iter()
            .enumerate()
            .map(|(idx, capability)| Goal {
                id: format!("goal-{:02}", idx + 1),
                subsystem: "Platform".to_string(),
                description: capability.clone(),
                priority: (idx as u8).saturating_add(1),
            })
            .collect();
    }

    let plan = build_plan(target, &goals);
    (model, goals, plan)
}

fn build_grounded_prompt(
    baseline: &ApplicationModel,
    system_graph: &SystemGraphVersion,
    target: &ScanTarget,
) -> String {
    let entities = baseline
        .entities
        .iter()
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let subsystems = system_graph
        .subsystems
        .iter()
        .map(|subsystem| subsystem.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Target: {}\nDescription: {}\nKnown entities: {}\nKnown subsystems: {}\nDesired capabilities:\n- {}\nConstraints:\n- {}\nReturn lines prefixed with GOAL: and ENTITY:.",
        target.name,
        target.description,
        entities,
        subsystems,
        target.desired_capabilities.join("\n- "),
        target.constraints.join("\n- "),
    )
}

fn build_plan(target: &ScanTarget, goals: &[Goal]) -> Plan {
    let mut steps = Vec::new();
    steps.push(PlanStep {
        title: "Establish target contracts".to_string(),
        details: format!(
            "Define contracts and interfaces for `{}` aligned with stated constraints.",
            target.name
        ),
        artifacts: vec!["contracts".to_string(), "api".to_string()],
    });
    for goal in goals {
        steps.push(PlanStep {
            title: format!("Implement {}", goal.id),
            details: goal.description.clone(),
            artifacts: vec!["application".to_string(), "tests".to_string()],
        });
    }
    steps.push(PlanStep {
        title: "Validate architecture convergence".to_string(),
        details: "Re-run scan and confirm gap output is reduced to zero for critical capabilities."
            .to_string(),
        artifacts: vec!["scan-summary".to_string()],
    });
    Plan {
        summary: format!(
            "{} prioritized goals for target architecture `{}`",
            goals.len(),
            target.name
        ),
        steps,
    }
}

fn extract_entity_name(raw: &str) -> String {
    let ignored = [
        "add",
        "build",
        "create",
        "enable",
        "implement",
        "support",
        "introduce",
    ];
    raw.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
        })
        .filter(|part| {
            !ignored
                .iter()
                .any(|ignore| part.eq_ignore_ascii_case(ignore))
        })
        .max_by_key(|part| part.len())
        .unwrap_or_default()
        .to_string()
}

fn pluralize_path(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with('s') {
        lower
    } else {
        format!("{lower}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use treehouse_application_model::{ApplicationInfo, GenerationMetadata};
    use treehouse_system_graph::SystemGraphVersion;
    use treehouse_target::{ArchitectureStyle, ScanTarget};

    fn baseline_model() -> ApplicationModel {
        ApplicationModel {
            application: ApplicationInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
            },
            entities: vec![Entity {
                name: "Customer".to_string(),
                confidence: 0.9,
                fields: vec![],
                relationships: vec![],
                constraints: vec![],
            }],
            workflows: vec![],
            permissions: vec![],
            api: vec![],
            experiences: vec![],
            integrations: vec![],
            metadata: GenerationMetadata {
                generated_by: "test".to_string(),
                generated_at_unix: 0,
                source_count: 1,
            },
        }
    }

    #[test]
    fn heuristic_generates_goals_from_target() {
        let target = ScanTarget {
            name: "Event Driven".to_string(),
            description: "Use event choreography".to_string(),
            constraints: vec!["Keep local".to_string()],
            desired_capabilities: vec!["Support InvoiceCreated events".to_string()],
            style: ArchitectureStyle::EventDriven,
        };
        let architecture = infer_target_architecture(
            &baseline_model(),
            &SystemGraphVersion::default(),
            &target,
            LocalLlmBackend::Heuristic,
        )
        .unwrap();
        assert!(!architecture.goals.is_empty());
        assert!(architecture
            .model
            .entities
            .iter()
            .any(|entity| entity.name == "InvoiceCreated"));
    }
}
