use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use treehouse_application_model::ApplicationModel;
use treehouse_evidence::{
    Confidence, EvidenceKind, EvidenceNode, EvidenceSnapshot, EvidenceStore, FileEvidenceStore,
    Provenance, SourceKind,
};
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_model_inference::infer_application_model;
use treehouse_observer::{append_snapshot_evidence, capture_snapshot};
use treehouse_parser::parse_structured_file;
use treehouse_planner::{infer_target_architecture, LocalLlmBackend, TargetArchitecture};
use treehouse_system_graph::{build_system_graph_from_evidence_snapshot, SystemGraphVersion};
use treehouse_target::{load_scan_target, sanitize_target_name};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanBaseline {
    pub evidence: EvidenceSnapshot,
    pub model: ApplicationModel,
    pub system_graph: SystemGraphVersion,
    pub generated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissingFile {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissingContract {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissingMigration {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiGap {
    pub surface: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GapAnalysis {
    pub missing_files: Vec<MissingFile>,
    pub missing_contracts: Vec<MissingContract>,
    pub missing_migrations: Vec<MissingMigration>,
    pub api_gaps: Vec<ApiGap>,
    pub subsystem_gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanSummary {
    pub target_name: Option<String>,
    pub generated_at: u64,
    pub baseline_entities: usize,
    pub target_entities: usize,
    pub goals: usize,
    pub missing_files: usize,
    pub missing_contracts: usize,
    pub missing_migrations: usize,
    pub api_gaps: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScanOutputFormat {
    Json,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub repo_path: PathBuf,
    pub target: Option<String>,
    pub output: Option<PathBuf>,
    pub local_llm: Option<String>,
    pub baseline_only: bool,
    pub goals_only: bool,
    pub format: ScanOutputFormat,
}

impl ScanRequest {
    pub fn new(repo_path: PathBuf, target: Option<String>) -> Self {
        Self {
            repo_path,
            target,
            output: None,
            local_llm: None,
            baseline_only: false,
            goals_only: false,
            format: ScanOutputFormat::Json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanResult {
    pub output_dir: PathBuf,
    pub baseline: ScanBaseline,
    pub target: Option<TargetArchitecture>,
    pub gap: Option<GapAnalysis>,
    pub summary: ScanSummary,
}

pub fn run_scan(request: &ScanRequest) -> Result<ScanResult> {
    let target_name = request.target.as_deref().unwrap_or("baseline");
    let output_dir = request.output.clone().unwrap_or_else(|| {
        request
            .repo_path
            .join(".treehouse/scan")
            .join(sanitize_target_name(target_name))
    });

    let baseline = if request.goals_only {
        load_baseline(&output_dir)?
    } else {
        let baseline = build_baseline(&request.repo_path)?;
        write_baseline(&output_dir, &baseline)?;
        append_scan_baseline_evidence(&request.repo_path, &baseline)?;
        baseline
    };

    if request.baseline_only {
        let summary = build_summary(None, &baseline, None);
        write_summary(&output_dir, &summary)?;
        return Ok(ScanResult {
            output_dir,
            baseline,
            target: None,
            gap: None,
            summary,
        });
    }

    let target_raw = request
        .target
        .as_deref()
        .ok_or_else(|| anyhow!("missing --target <path|name> argument"))?;
    let scan_target = load_scan_target(&request.repo_path, target_raw)?;

    let backend = if request.local_llm.is_some() {
        LocalLlmBackend::parse(request.local_llm.as_deref())
    } else {
        LocalLlmBackend::Heuristic
    };

    let target_architecture = infer_target_architecture(
        &baseline.model,
        &baseline.system_graph,
        &scan_target,
        backend,
    )?;

    write_target(&output_dir, &target_architecture)?;
    append_target_evidence(&request.repo_path, &target_architecture)?;

    let gap = analyze_gap(&baseline, &target_architecture);
    write_gap(&output_dir, &gap)?;
    append_gap_evidence(&request.repo_path, &gap)?;

    let summary = build_summary(Some(&target_architecture), &baseline, Some(&gap));
    write_summary(&output_dir, &summary)?;

    Ok(ScanResult {
        output_dir,
        baseline,
        target: Some(target_architecture),
        gap: Some(gap),
        summary,
    })
}

pub fn summary_markdown(summary: &ScanSummary) -> String {
    format!(
        "# Scan Summary\n\n- Target: {}\n- Baseline entities: {}\n- Target entities: {}\n- Goals: {}\n- Missing files: {}\n- Missing contracts: {}\n- Missing migrations: {}\n- API gaps: {}\n",
        summary.target_name.clone().unwrap_or_else(|| "baseline-only".to_string()),
        summary.baseline_entities,
        summary.target_entities,
        summary.goals,
        summary.missing_files,
        summary.missing_contracts,
        summary.missing_migrations,
        summary.api_gaps,
    )
}

fn build_baseline(repo_root: &Path) -> Result<ScanBaseline> {
    let snapshot = capture_snapshot(repo_root)?;
    let evidence = append_snapshot_evidence(repo_root, &snapshot, None)?;
    let model = infer_model_from_repo(repo_root)?;
    let system_graph =
        build_system_graph_from_evidence_snapshot(snapshot.generated_at_unix, &evidence);

    Ok(ScanBaseline {
        evidence,
        model,
        system_graph,
        generated_at: snapshot.generated_at_unix,
    })
}

fn infer_model_from_repo(repo_root: &Path) -> Result<ApplicationModel> {
    let files = collect_files(repo_root)?;
    let mut parsed = Vec::new();
    for file in files {
        if let Ok(parsed_file) = parse_structured_file(&file) {
            parsed.push(parsed_file);
        }
    }

    let source_names: Vec<String> = parsed
        .iter()
        .map(|item| item.path.to_string_lossy().to_string())
        .collect();
    let sources: Vec<GraphSource<'_>> = parsed
        .iter()
        .zip(source_names.iter())
        .map(|(parsed, name)| GraphSource {
            name: name.as_str(),
            document: &parsed.document,
        })
        .collect();
    let graph = UniversalDataGraph::build(&sources);
    Ok(infer_application_model(&graph, None))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    walk_dir(root, root, &mut output)?;
    output.sort();
    Ok(output)
}

fn walk_dir(_root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("failed reading {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
            continue;
        }
        if path.is_dir() {
            walk_dir(_root, &path, output)?;
            continue;
        }
        output.push(path);
    }
    Ok(())
}

fn analyze_gap(baseline: &ScanBaseline, target: &TargetArchitecture) -> GapAnalysis {
    let baseline_entities: Vec<String> = baseline
        .model
        .entities
        .iter()
        .map(|entity| entity.name.to_ascii_lowercase())
        .collect();

    let mut missing_files = Vec::new();
    let mut missing_migrations = Vec::new();

    for entity in &target.model.entities {
        if baseline_entities
            .iter()
            .any(|name| name == &entity.name.to_ascii_lowercase())
        {
            continue;
        }
        missing_files.push(MissingFile {
            path: format!("src/domain/{}.rs", to_snake_case(&entity.name)),
            reason: format!("Target model adds entity `{}`", entity.name),
        });
        missing_migrations.push(MissingMigration {
            name: format!("create_{}", pluralize(&entity.name)),
            reason: format!("Database table for `{}` is missing", entity.name),
        });
    }

    let baseline_api: Vec<String> = baseline
        .model
        .api
        .iter()
        .map(|endpoint| format!("{} {}", endpoint.method, endpoint.path))
        .collect();

    let mut api_gaps = Vec::new();
    for endpoint in &target.model.api {
        let surface = format!("{} {}", endpoint.method, endpoint.path);
        if baseline_api.iter().any(|existing| existing == &surface) {
            continue;
        }
        api_gaps.push(ApiGap {
            surface,
            reason: format!(
                "Target API for `{}` not present in baseline",
                endpoint.entity
            ),
        });
    }

    let subsystem_names: Vec<String> = baseline
        .system_graph
        .subsystems
        .iter()
        .map(|subsystem| subsystem.id.to_ascii_lowercase())
        .collect();
    let mut subsystem_gaps = Vec::new();
    for goal in &target.goals {
        if !subsystem_names
            .iter()
            .any(|subsystem| subsystem == &goal.subsystem.to_ascii_lowercase())
        {
            subsystem_gaps.push(goal.subsystem.clone());
        }
    }
    subsystem_gaps.sort();
    subsystem_gaps.dedup();

    let missing_contracts = target
        .goals
        .iter()
        .map(|goal| MissingContract {
            name: format!("{}-contract", to_kebab_case(&goal.description)),
            reason: format!("Goal `{}` requires explicit subsystem contract", goal.id),
        })
        .collect();

    GapAnalysis {
        missing_files,
        missing_contracts,
        missing_migrations,
        api_gaps,
        subsystem_gaps,
    }
}

fn write_baseline(output_dir: &Path, baseline: &ScanBaseline) -> Result<()> {
    write_json(
        &output_dir.join("baseline/evidence-snapshot.json"),
        &baseline.evidence,
    )?;
    write_json(
        &output_dir.join("baseline/application-model.json"),
        &baseline.model,
    )?;
    write_json(
        &output_dir.join("baseline/system-graph.json"),
        &baseline.system_graph,
    )?;
    Ok(())
}

fn write_target(output_dir: &Path, target: &TargetArchitecture) -> Result<()> {
    write_json(
        &output_dir.join("target/inferred-architecture.json"),
        target,
    )?;
    write_json(&output_dir.join("target/goals.json"), &target.goals)?;
    write_text(
        &output_dir.join("target/plan.md"),
        &render_plan_markdown(target),
    )?;
    Ok(())
}

fn write_gap(output_dir: &Path, gap: &GapAnalysis) -> Result<()> {
    write_text(
        &output_dir.join("gap/analysis.md"),
        &render_gap_markdown(gap),
    )?;

    for file in &gap.missing_files {
        write_text(
            &output_dir
                .join("gap/files-to-add")
                .join(format!("{}.txt", to_kebab_case(&file.path))),
            &format!("{}\n{}", file.path, file.reason),
        )?;
    }
    for contract in &gap.missing_contracts {
        write_text(
            &output_dir
                .join("gap/contracts-to-add")
                .join(format!("{}.txt", to_kebab_case(&contract.name))),
            &format!("{}\n{}", contract.name, contract.reason),
        )?;
    }
    for migration in &gap.missing_migrations {
        write_text(
            &output_dir
                .join("gap/migrations-to-add")
                .join(format!("{}.txt", to_kebab_case(&migration.name))),
            &format!("{}\n{}", migration.name, migration.reason),
        )?;
    }
    for api in &gap.api_gaps {
        write_text(
            &output_dir
                .join("gap/api-surfaces-to-add")
                .join(format!("{}.txt", to_kebab_case(&api.surface))),
            &format!("{}\n{}", api.surface, api.reason),
        )?;
    }

    Ok(())
}

fn write_summary(output_dir: &Path, summary: &ScanSummary) -> Result<()> {
    write_json(&output_dir.join("summary.json"), summary)
}

fn append_scan_baseline_evidence(repo_root: &Path, baseline: &ScanBaseline) -> Result<()> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    store.append_node(&EvidenceNode::new(
        EvidenceKind::ScanBaseline {
            summary: format!(
                "entities={}, subsystems={}",
                baseline.model.entities.len(),
                baseline.system_graph.subsystems.len()
            ),
        },
        baseline.generated_at,
        Confidence::new(0.98, Some("scan baseline".to_string())),
        Provenance::new(SourceKind::Other, "scan/baseline", "treehouse-scan"),
        None,
        serde_json::json!({"generated_at": baseline.generated_at}),
    ))
}

fn append_target_evidence(repo_root: &Path, target: &TargetArchitecture) -> Result<()> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    store.append_node(&EvidenceNode::new(
        EvidenceKind::LlmInferredTarget {
            summary: target.plan.summary.clone(),
        },
        target.model.metadata.generated_at_unix,
        Confidence::new(
            target.provenance.confidence,
            Some("local llm planning".to_string()),
        ),
        Provenance::new(SourceKind::Other, "scan/target", "treehouse-planner"),
        None,
        serde_json::json!({"backend": target.provenance.backend}),
    ))
}

fn append_gap_evidence(repo_root: &Path, gap: &GapAnalysis) -> Result<()> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    for item in gap
        .missing_files
        .iter()
        .map(|file| format!("file: {}", file.path))
        .chain(
            gap.missing_contracts
                .iter()
                .map(|contract| format!("contract: {}", contract.name)),
        )
        .chain(
            gap.missing_migrations
                .iter()
                .map(|migration| format!("migration: {}", migration.name)),
        )
        .chain(
            gap.api_gaps
                .iter()
                .map(|api| format!("api: {}", api.surface)),
        )
    {
        store.append_node(&EvidenceNode::new(
            EvidenceKind::GapRecommendation {
                recommendation: item,
            },
            now_unix(),
            Confidence::new(0.82, Some("gap analysis".to_string())),
            Provenance::new(SourceKind::Other, "scan/gap", "treehouse-scan"),
            None,
            serde_json::Value::Null,
        ))?;
    }
    Ok(())
}

fn build_summary(
    target: Option<&TargetArchitecture>,
    baseline: &ScanBaseline,
    gap: Option<&GapAnalysis>,
) -> ScanSummary {
    ScanSummary {
        target_name: target.map(|item| item.plan.summary.clone()),
        generated_at: now_unix(),
        baseline_entities: baseline.model.entities.len(),
        target_entities: target
            .map(|item| item.model.entities.len())
            .unwrap_or(baseline.model.entities.len()),
        goals: target.map(|item| item.goals.len()).unwrap_or(0),
        missing_files: gap.map(|item| item.missing_files.len()).unwrap_or(0),
        missing_contracts: gap.map(|item| item.missing_contracts.len()).unwrap_or(0),
        missing_migrations: gap.map(|item| item.missing_migrations.len()).unwrap_or(0),
        api_gaps: gap.map(|item| item.api_gaps.len()).unwrap_or(0),
    }
}

fn load_baseline(output_dir: &Path) -> Result<ScanBaseline> {
    let evidence: EvidenceSnapshot =
        read_json(&output_dir.join("baseline/evidence-snapshot.json"))?;
    let model: ApplicationModel = read_json(&output_dir.join("baseline/application-model.json"))?;
    let system_graph: SystemGraphVersion =
        read_json(&output_dir.join("baseline/system-graph.json"))?;
    Ok(ScanBaseline {
        generated_at: evidence.observed_through_unix,
        evidence,
        model,
        system_graph,
    })
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content).with_context(|| format!("failed writing {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed parsing {}", path.display()))
}

fn write_text(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating {}", parent.display()))?;
    }
    fs::write(path, value).with_context(|| format!("failed writing {}", path.display()))
}

fn render_plan_markdown(target: &TargetArchitecture) -> String {
    let mut output = vec![
        "# Plan".to_string(),
        String::new(),
        target.plan.summary.clone(),
    ];
    for step in &target.plan.steps {
        output.push(format!("\n## {}", step.title));
        output.push(step.details.clone());
        if !step.artifacts.is_empty() {
            output.push("Artifacts:".to_string());
            for artifact in &step.artifacts {
                output.push(format!("- {artifact}"));
            }
        }
    }
    output.join("\n")
}

fn render_gap_markdown(gap: &GapAnalysis) -> String {
    let mut output = vec!["# Gap Analysis".to_string()];
    output.push(format!("- Missing files: {}", gap.missing_files.len()));
    output.push(format!(
        "- Missing contracts: {}",
        gap.missing_contracts.len()
    ));
    output.push(format!(
        "- Missing migrations: {}",
        gap.missing_migrations.len()
    ));
    output.push(format!("- API gaps: {}", gap.api_gaps.len()));
    output.push("\n## Files to add".to_string());
    for file in &gap.missing_files {
        output.push(format!("- {}: {}", file.path, file.reason));
    }
    output.push("\n## Contracts to add".to_string());
    for contract in &gap.missing_contracts {
        output.push(format!("- {}: {}", contract.name, contract.reason));
    }
    output.push("\n## Migrations to add".to_string());
    for migration in &gap.missing_migrations {
        output.push(format!("- {}: {}", migration.name, migration.reason));
    }
    output.push("\n## API surfaces to add".to_string());
    for api in &gap.api_gaps {
        output.push(format!("- {}: {}", api.surface, api.reason));
    }
    output.join("\n")
}

fn to_snake_case(input: &str) -> String {
    let mut output = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() && idx > 0 {
            output.push('_');
        }
        output.push(ch.to_ascii_lowercase());
    }
    output
}

fn to_kebab_case(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn pluralize(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.ends_with('s') {
        lower
    } else {
        format!("{lower}s")
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_generates_expected_artifacts() {
        let root = std::env::temp_dir().join("treehouse-scan-e2e-test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("targets")).unwrap();
        fs::write(
            root.join("orders.json"),
            r#"[{"id":"o1","customer_id":"c1","total":9.5}]"#,
        )
        .unwrap();
        fs::write(
            root.join("targets/event-driven.md"),
            "# Event Driven\n## Capabilities\n- Add InvoiceProjection",
        )
        .unwrap();

        let request = ScanRequest {
            repo_path: root.clone(),
            target: Some("event-driven".to_string()),
            output: Some(root.join(".treehouse/scan/test")),
            local_llm: Some("heuristic".to_string()),
            baseline_only: false,
            goals_only: false,
            format: ScanOutputFormat::Json,
        };

        let result = run_scan(&request).unwrap();
        assert!(result
            .output_dir
            .join("baseline/evidence-snapshot.json")
            .exists());
        assert!(result
            .output_dir
            .join("target/inferred-architecture.json")
            .exists());
        assert!(result.output_dir.join("gap/analysis.md").exists());
        assert!(result.output_dir.join("summary.json").exists());
    }
}
