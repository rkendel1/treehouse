use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use treehouse_application_model::{ApiEndpoint, Integration, Relationship, Workflow};
use treehouse_drift::{detect_drift, DriftEvent, OwnershipPolicy};
use treehouse_evidence::{
    Confidence, EvidenceEdge, EvidenceKind, EvidenceNode, EvidenceQuery, EvidenceSnapshot,
    EvidenceStore, FileEvidenceStore, Provenance, RelationKind, SourceKind,
};
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_model_inference::infer_application_model;
use treehouse_parser::parse_structured_file;
use treehouse_subsystem_engine::{discover_subsystems, SubsystemSignals};
use treehouse_system_graph::{build_system_graph_version, Subsystem};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotState {
    pub snapshot: DevelopmentSnapshot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevelopmentSnapshot {
    pub generated_at_unix: u64,
    pub git_head: Option<String>,
    pub git_changes: Vec<String>,
    pub parsed_sources: usize,
    pub entities: Vec<String>,
    pub relationships: Vec<String>,
    pub workflows: Vec<String>,
    pub api_endpoints: Vec<String>,
    pub integrations: Vec<String>,
    pub migration_tables: Vec<String>,
    pub migration_files: Vec<String>,
    pub test_files: Vec<String>,
    pub test_cases: Vec<String>,
    pub runtime_events: Vec<String>,
    pub db_signals: Vec<String>,
    pub code_symbols: Vec<String>,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemDiffReport {
    pub summary: String,
    pub changed_files: Vec<String>,
    pub code_symbols_added: Vec<String>,
    pub code_symbols_removed: Vec<String>,
    pub entities_added: Vec<String>,
    pub entities_removed: Vec<String>,
    pub relationships_added: Vec<String>,
    pub api_added: Vec<String>,
    pub workflows_added: Vec<String>,
    pub integrations_added: Vec<String>,
    pub migration_tables_added: Vec<String>,
    pub runtime_events_added: Vec<String>,
    pub db_signals_added: Vec<String>,
    pub new_capabilities: Vec<String>,
    pub potential_breaks: Vec<String>,
    pub architecture_drift: Vec<String>,
    pub subsystem_alert: Option<String>,
    pub detected_subsystems: Vec<Subsystem>,
    pub drift_events: Vec<DriftEvent>,
    pub architecture_confidence: u8,
}

pub fn capture_snapshot(repo_root: &Path) -> Result<DevelopmentSnapshot> {
    let files = collect_files(repo_root).with_context(|| {
        format!(
            "failed to collect files for development observation: {}",
            repo_root.display()
        )
    })?;

    let git_changes = git_status_porcelain(repo_root).unwrap_or_default();
    let git_head = git_head(repo_root).ok().flatten();
    let code_symbols = extract_code_symbols(repo_root, &files);
    let migration_files = collect_matching_files(&files, |path| is_migration_file(path));
    let migration_tables = extract_migration_tables(repo_root, &migration_files);
    let test_files = collect_matching_files(&files, |path| is_test_file(path));
    let test_cases = extract_test_cases(repo_root, &test_files);
    let runtime_events = collect_runtime_events(repo_root, &files);
    let db_signals = collect_db_signals(repo_root, &files, &migration_tables);

    let mut parsed = Vec::new();
    for rel in &files {
        let path = repo_root.join(rel);
        if !is_structured_file(&path) {
            continue;
        }
        if let Ok(parsed_file) = parse_structured_file(&path) {
            parsed.push(parsed_file);
        }
    }

    let source_names: Vec<String> = parsed
        .iter()
        .map(|parsed| parsed.path.to_string_lossy().to_string())
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
    let model = infer_application_model(&graph, None);

    let mut domains = infer_domains(model.entities.iter().map(|entity| entity.name.as_str()));
    domains.extend(infer_domains(migration_tables.iter().map(String::as_str)));
    domains = dedupe_sorted(domains);

    Ok(DevelopmentSnapshot {
        generated_at_unix: now_unix(),
        git_head,
        git_changes,
        parsed_sources: sources.len(),
        entities: model
            .entities
            .iter()
            .map(|entity| entity.name.clone())
            .collect(),
        relationships: flatten_relationships(&model.entities),
        workflows: flatten_workflows(&model.workflows),
        api_endpoints: flatten_api(&model.api),
        integrations: flatten_integrations(&model.integrations),
        migration_tables,
        migration_files,
        test_files,
        test_cases,
        runtime_events,
        db_signals,
        code_symbols,
        domains,
    })
}

pub fn compute_system_diff(
    previous: Option<&DevelopmentSnapshot>,
    current: &DevelopmentSnapshot,
) -> SystemDiffReport {
    let empty = DevelopmentSnapshot::default();
    let before = previous.unwrap_or(&empty);

    let changed_files = current.git_changes.clone();
    let code_symbols_added = set_added(&before.code_symbols, &current.code_symbols);
    let code_symbols_removed = set_removed(&before.code_symbols, &current.code_symbols);
    let entities_added = set_added(&before.entities, &current.entities);
    let entities_removed = set_removed(&before.entities, &current.entities);
    let relationships_added = set_added(&before.relationships, &current.relationships);
    let api_added = set_added(&before.api_endpoints, &current.api_endpoints);
    let workflows_added = set_added(&before.workflows, &current.workflows);
    let integrations_added = set_added(&before.integrations, &current.integrations);
    let migration_tables_added = set_added(&before.migration_tables, &current.migration_tables);
    let runtime_events_added = set_added(&before.runtime_events, &current.runtime_events);
    let db_signals_added = set_added(&before.db_signals, &current.db_signals);

    let mut new_capabilities = Vec::new();
    new_capabilities.extend(
        entities_added
            .iter()
            .map(|entity| format!("Entity: {entity}")),
    );
    for table in &migration_tables_added {
        new_capabilities.push(format!("Database: new table `{table}`"));
    }
    for endpoint in &api_added {
        new_capabilities.push(format!("API: {endpoint}"));
    }
    if !runtime_events_added.is_empty() {
        new_capabilities.push(format!(
            "Runtime: {} new runtime/event markers",
            runtime_events_added.len()
        ));
    }
    new_capabilities = dedupe_sorted(new_capabilities);

    let mut potential_breaks = Vec::new();
    for entity in &entities_added {
        let entity_l = entity.to_ascii_lowercase();
        let has_status = current.db_signals.iter().any(|signal| {
            signal.contains(entity) && signal.to_ascii_lowercase().contains("status")
        });
        if (entity_l.contains("invoice")
            || entity_l.contains("payment")
            || entity_l.contains("refund"))
            && !has_status
        {
            potential_breaks.push(format!(
                "{entity} appears without lifecycle status signal; workflow states may be underspecified"
            ));
        }
    }
    if api_added
        .iter()
        .any(|endpoint| endpoint.to_ascii_lowercase().contains("refund"))
        && !migration_tables_added
            .iter()
            .any(|table| table.to_ascii_lowercase().contains("refund"))
    {
        potential_breaks.push(
            "Refund API surface appeared without matching refund table migration in this snapshot"
                .to_string(),
        );
    }

    let before_graph = snapshot_to_system_graph(before, before.generated_at_unix);
    let current_graph = snapshot_to_system_graph(current, current.generated_at_unix);
    let ownership_policies = default_ownership_policies();
    let drift_events = detect_drift(Some(&before_graph), &current_graph, &ownership_policies);

    let mut architecture_drift = detect_architecture_drift(before, current);
    architecture_drift.extend(
        drift_events
            .iter()
            .filter(|event| {
                matches!(
                    event.drift_type,
                    treehouse_drift::DriftType::ArchitectureDrift
                )
            })
            .filter_map(|event| event.evidence.first().cloned()),
    );
    architecture_drift = dedupe_sorted(architecture_drift);
    let subsystem_alert = detect_subsystem_alert(before, current);

    let summary = format!(
        "System Change: +{} entities, +{} relationships, +{} APIs, +{} workflows",
        entities_added.len(),
        relationships_added.len(),
        api_added.len(),
        workflows_added.len()
    );

    SystemDiffReport {
        summary,
        changed_files,
        code_symbols_added,
        code_symbols_removed,
        entities_added,
        entities_removed,
        relationships_added,
        api_added,
        workflows_added,
        integrations_added,
        migration_tables_added,
        runtime_events_added,
        db_signals_added,
        new_capabilities,
        potential_breaks,
        architecture_drift,
        subsystem_alert,
        detected_subsystems: current_graph.subsystems.clone(),
        drift_events,
        architecture_confidence: (current_graph.architecture_confidence * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8,
    }
}

pub fn append_snapshot_evidence(
    repo_root: &Path,
    snapshot: &DevelopmentSnapshot,
    report: Option<&SystemDiffReport>,
) -> Result<EvidenceSnapshot> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    let mut node_ids = Vec::new();

    for change in &snapshot.git_changes {
        let (status, file) = parse_git_change(change);
        let node = EvidenceNode::new(
            EvidenceKind::GitDelta { file, status },
            snapshot.generated_at_unix,
            Confidence::new(0.95, Some("git status observation".to_string())),
            Provenance::new(
                SourceKind::Git,
                "git status --porcelain",
                "treehouse-observer",
            ),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for symbol in &snapshot.code_symbols {
        let node = EvidenceNode::new(
            EvidenceKind::Symbol {
                language: infer_language(symbol),
                kind: "symbol".to_string(),
                name: symbol.clone(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.75, Some("syntax extraction".to_string())),
            Provenance::new(SourceKind::Ast, symbol, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for table in &snapshot.migration_tables {
        let node = EvidenceNode::new(
            EvidenceKind::Migration {
                table: table.clone(),
                operation: "create_or_alter".to_string(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.90, Some("migration parser".to_string())),
            Provenance::new(SourceKind::Migration, table, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for endpoint in &snapshot.api_endpoints {
        let (method, path) = split_api_endpoint(endpoint);
        let node = EvidenceNode::new(
            EvidenceKind::ApiSurface { method, path },
            snapshot.generated_at_unix,
            Confidence::new(0.80, Some("inferred API surface".to_string())),
            Provenance::new(SourceKind::Api, endpoint, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for workflow in &snapshot.workflows {
        let node = EvidenceNode::new(
            EvidenceKind::Workflow {
                name: workflow.clone(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.80, Some("workflow inference".to_string())),
            Provenance::new(SourceKind::Workflow, workflow, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for entity in &snapshot.entities {
        let subsystem = infer_entity_subsystem(entity);
        let node = EvidenceNode::new(
            EvidenceKind::Entity {
                name: entity.clone(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.85, Some("entity inference".to_string())),
            Provenance::new(SourceKind::Entity, entity, "treehouse-observer"),
            subsystem,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for test_case in &snapshot.test_cases {
        let node = EvidenceNode::new(
            EvidenceKind::TestResult {
                name: test_case.clone(),
                status: "observed".to_string(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.70, Some("test discovery".to_string())),
            Provenance::new(SourceKind::Test, test_case, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for event in &snapshot.runtime_events {
        let node = EvidenceNode::new(
            EvidenceKind::RuntimeEvent {
                event: event.clone(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.70, Some("runtime log marker".to_string())),
            Provenance::new(SourceKind::Runtime, event, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    for signal in &snapshot.db_signals {
        let node = EvidenceNode::new(
            EvidenceKind::DbSignal {
                signal: signal.clone(),
            },
            snapshot.generated_at_unix,
            Confidence::new(0.75, Some("db signal extraction".to_string())),
            Provenance::new(SourceKind::Database, signal, "treehouse-observer"),
            None,
            serde_json::Value::Null,
        );
        store.append_node(&node)?;
        node_ids.push(node.id);
    }

    let mut finding_ids = Vec::new();
    if let Some(report) = report {
        for finding in report
            .new_capabilities
            .iter()
            .chain(report.potential_breaks.iter())
            .chain(report.architecture_drift.iter())
        {
            let node = EvidenceNode::new(
                EvidenceKind::SystemDiffFinding {
                    finding: finding.clone(),
                },
                snapshot.generated_at_unix,
                Confidence::new(0.78, Some("system diff".to_string())),
                Provenance::new(SourceKind::SystemDiff, "system-diff", "treehouse-observer"),
                None,
                serde_json::json!({ "summary": report.summary }),
            );
            store.append_node(&node)?;
            finding_ids.push(node.id);
        }
    }

    if let Some(anchor) = node_ids.first() {
        for finding in finding_ids {
            store.append_edge(&EvidenceEdge {
                from: finding,
                to: anchor.clone(),
                relation: RelationKind::DerivedFrom,
                confidence: Confidence::new(0.78, Some("diff derived from snapshot".to_string())),
                observed_at_unix: snapshot.generated_at_unix,
            })?;
        }
    }

    store.snapshot()
}

pub fn load_evidence_snapshot(repo_root: &Path) -> Result<EvidenceSnapshot> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    store.snapshot()
}

pub fn query_evidence(repo_root: &Path, query: &EvidenceQuery) -> Result<Vec<EvidenceNode>> {
    let store = FileEvidenceStore::new(repo_root.join(".treehouse/evidence"));
    store.query(query)
}

pub fn render_report(report: &SystemDiffReport) -> String {
    let mut out = Vec::new();
    out.push(report.summary.clone());
    if !report.new_capabilities.is_empty() {
        out.push("New Capability Detected:".to_string());
        for capability in &report.new_capabilities {
            out.push(format!("  + {capability}"));
        }
    }
    if !report.relationships_added.is_empty() {
        out.push("Relationships:".to_string());
        for relationship in &report.relationships_added {
            out.push(format!("  + {relationship}"));
        }
    }
    if !report.potential_breaks.is_empty() {
        out.push("Potential Issues:".to_string());
        for issue in &report.potential_breaks {
            out.push(format!("  ! {issue}"));
        }
    }
    if !report.architecture_drift.is_empty() {
        out.push("Architecture Drift:".to_string());
        for drift in &report.architecture_drift {
            out.push(format!("  ! {drift}"));
        }
    }
    if let Some(alert) = &report.subsystem_alert {
        out.push(format!("Subsystem Alert: {alert}"));
    }
    if !report.detected_subsystems.is_empty() {
        out.push("Subsystems:".to_string());
        for subsystem in &report.detected_subsystems {
            out.push(format!(
                "  ✓ {} (owner: {}, confidence: {:.0}%)",
                subsystem.id,
                subsystem.owner.as_deref().unwrap_or("unassigned"),
                subsystem.confidence * 100.0
            ));
        }
    }
    if !report.drift_events.is_empty() {
        out.push("Drift Events:".to_string());
        for event in &report.drift_events {
            out.push(format!(
                "  ! {:?}: {}",
                event.drift_type, event.recommendation.details
            ));
        }
    }
    out.push(format!(
        "Architecture confidence: {}%",
        report.architecture_confidence
    ));
    out.join("\n")
}

pub fn load_state(path: &Path) -> Result<Option<SnapshotState>> {
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    let state = serde_json::from_str(&content)
        .with_context(|| format!("failed parsing {}", path.display()))?;
    Ok(Some(state))
}

pub fn save_state(path: &Path, state: &SnapshotState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating state directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(state)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

pub fn save_report(path: &Path, report: &SystemDiffReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating report directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(report)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_dir(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_dir(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
            continue;
        }
        if path.is_dir() {
            walk_dir(root, &path, out)?;
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

fn git_head(root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "HEAD"])
        .output();
    match output {
        Ok(output) if output.status.success() => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )),
        _ => Ok(None),
    }
}

fn git_status_porcelain(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain"])
        .output()
        .context("failed to run git status --porcelain")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    lines.sort();
    Ok(lines)
}

fn collect_matching_files<F>(files: &[PathBuf], predicate: F) -> Vec<String>
where
    F: Fn(&Path) -> bool,
{
    let mut selected: Vec<String> = files
        .iter()
        .filter(|path| predicate(path.as_path()))
        .map(|path| path.display().to_string())
        .collect();
    selected.sort();
    selected
}

fn is_structured_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "json" | "jsonl" | "ndjson" | "yaml" | "yml" | "toml" | "xml" | "csv")
    )
}

fn is_migration_file(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("sql"))
        != Some(true)
    {
        return false;
    }
    let lower = path.to_string_lossy().to_ascii_lowercase();
    lower.contains("migration") || lower.contains("migrations")
}

fn is_test_file(path: &Path) -> bool {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    lower.contains("/tests/")
        || lower.ends_with("_test.rs")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".test.js")
}

fn extract_migration_tables(root: &Path, migration_files: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for rel in migration_files {
        let path = root.join(rel);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();
            if let Some(table) = extract_after_token(&lower, "create table") {
                out.insert(clean_table_name(table));
            }
            if let Some(table) = extract_after_token(&lower, "alter table") {
                out.insert(clean_table_name(table));
            }
        }
    }
    out.into_iter().collect()
}

fn extract_after_token<'a>(line: &'a str, token: &str) -> Option<&'a str> {
    let index = line.find(token)?;
    let remainder = line[index + token.len()..].trim();
    if remainder.is_empty() {
        None
    } else {
        Some(remainder)
    }
}

fn parse_git_change(change: &str) -> (String, String) {
    let mut parts = change.split_whitespace();
    let status = parts.next().unwrap_or("??").to_string();
    let file = parts.next().unwrap_or(change).to_string();
    (status, file)
}

fn split_api_endpoint(endpoint: &str) -> (String, String) {
    match endpoint.split_once(' ') {
        Some((method, path)) => (method.to_string(), path.to_string()),
        None => ("GET".to_string(), endpoint.to_string()),
    }
}

fn infer_entity_subsystem(entity: &str) -> Option<String> {
    let lower = entity.to_ascii_lowercase();
    if lower.contains("invoice")
        || lower.contains("payment")
        || lower.contains("refund")
        || lower.contains("subscription")
    {
        return Some("Billing".to_string());
    }
    if lower.contains("user") || lower.contains("tenant") || lower.contains("session") {
        return Some("Identity".to_string());
    }
    None
}

fn infer_language(symbol: &str) -> String {
    let lower = symbol.to_ascii_lowercase();
    if lower.ends_with(".rs") || lower.contains(".rs::") {
        return "rust".to_string();
    }
    if lower.ends_with(".ts") || lower.contains(".ts::") || lower.ends_with(".tsx") {
        return "typescript".to_string();
    }
    if lower.ends_with(".js") || lower.contains(".js::") {
        return "javascript".to_string();
    }
    if lower.ends_with(".py") || lower.contains(".py::") {
        return "python".to_string();
    }
    "unknown".to_string()
}

fn clean_table_name(raw: &str) -> String {
    raw.split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .to_string()
}

fn extract_test_cases(root: &Path, test_files: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for rel in test_files {
        let path = root.join(rel);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(name) = parse_rust_test_name(trimmed) {
                out.insert(format!("{rel}::{name}"));
            } else if let Some(name) = parse_js_test_name(trimmed) {
                out.insert(format!("{rel}::{name}"));
            }
        }
    }
    out.into_iter().collect()
}

fn parse_rust_test_name(line: &str) -> Option<String> {
    if !line.starts_with("fn ") {
        return None;
    }
    let tail = line.trim_start_matches("fn ").trim();
    let name = tail.split('(').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_js_test_name(line: &str) -> Option<String> {
    for prefix in ["it(\"", "test(\"", "describe(\""] {
        if let Some(rest) = line.strip_prefix(prefix) {
            if let Some((name, _)) = rest.split_once('"') {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn collect_runtime_events(root: &Path, files: &[PathBuf]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for rel in files {
        let lower = rel.to_string_lossy().to_ascii_lowercase();
        if !(lower.contains("event")
            || lower.contains("trace")
            || lower.contains("runtime")
            || lower.contains("log"))
        {
            continue;
        }
        let path = root.join(rel);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines().take(500) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.contains("->") || line.contains("status") || line.contains("event") {
                out.insert(line.to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn collect_db_signals(root: &Path, files: &[PathBuf], migration_tables: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for table in migration_tables {
        out.insert(format!("table:{table}"));
    }
    for rel in files {
        let lower = rel.to_string_lossy().to_ascii_lowercase();
        if !(lower.ends_with(".sql")
            || lower.ends_with(".json")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml"))
        {
            continue;
        }
        let path = root.join(rel);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for token in ["id", "_id", "tenant", "status", "due_date", "created_at"] {
            if content.contains(token) {
                out.insert(format!("{}:{token}", rel.display()));
            }
        }
    }
    out.into_iter().collect()
}

fn extract_code_symbols(root: &Path, files: &[PathBuf]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for rel in files {
        let ext = rel
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());
        if !matches!(
            ext.as_deref(),
            Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go")
        ) {
            continue;
        }
        let path = root.join(rel);
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            for prefix in [
                "fn ",
                "pub fn ",
                "struct ",
                "pub struct ",
                "enum ",
                "pub enum ",
                "class ",
                "def ",
                "function ",
            ] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let symbol = rest
                        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !symbol.is_empty() {
                        out.insert(format!("{}::{symbol}", rel.display()));
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

fn flatten_relationships(entities: &[treehouse_application_model::Entity]) -> Vec<String> {
    let mut out = Vec::new();
    for entity in entities {
        for Relationship { target, .. } in &entity.relationships {
            out.push(format!("{} -> {}", entity.name, target));
        }
    }
    dedupe_sorted(out)
}

fn flatten_workflows(workflows: &[Workflow]) -> Vec<String> {
    let mut out = Vec::new();
    for workflow in workflows {
        out.push(format!(
            "{}:{}",
            workflow.entity,
            workflow.states.join("->")
        ));
    }
    dedupe_sorted(out)
}

fn flatten_api(api: &[ApiEndpoint]) -> Vec<String> {
    let mut out = Vec::new();
    for endpoint in api {
        out.push(format!("{} {}", endpoint.method, endpoint.path));
    }
    dedupe_sorted(out)
}

fn flatten_integrations(integrations: &[Integration]) -> Vec<String> {
    let mut out = Vec::new();
    for integration in integrations {
        out.push(format!(
            "{} -> {}",
            integration.integration_type, integration.target
        ));
    }
    dedupe_sorted(out)
}

fn set_added(before: &[String], after: &[String]) -> Vec<String> {
    let before_set: BTreeSet<&str> = before.iter().map(String::as_str).collect();
    let mut out: Vec<String> = after
        .iter()
        .filter(|item| !before_set.contains(item.as_str()))
        .cloned()
        .collect();
    out.sort();
    out
}

fn set_removed(before: &[String], after: &[String]) -> Vec<String> {
    set_added(after, before)
}

fn infer_domains<I, S>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut tags = BTreeSet::new();
    for name in names {
        let lower = name.as_ref().to_ascii_lowercase();
        if lower.contains("customer")
            || lower.contains("contact")
            || lower.contains("opportunity")
            || lower.contains("crm")
        {
            tags.insert("crm".to_string());
        }
        if lower.contains("invoice")
            || lower.contains("billing")
            || lower.contains("payment")
            || lower.contains("refund")
        {
            tags.insert("billing".to_string());
        }
        if lower.contains("inventory") || lower.contains("warehouse") || lower.contains("shipment")
        {
            tags.insert("operations".to_string());
        }
        if lower.contains("notification")
            || lower.contains("template")
            || lower.contains("channel")
            || lower.contains("sms")
            || lower.contains("email")
        {
            tags.insert("communication".to_string());
        }
        if lower.contains("user") || lower.contains("auth") || lower.contains("identity") {
            tags.insert("identity".to_string());
        }
    }
    tags.into_iter().collect()
}

fn snapshot_to_system_graph(
    snapshot: &DevelopmentSnapshot,
    version: u64,
) -> treehouse_system_graph::SystemGraphVersion {
    let subsystems = discover_subsystems(&SubsystemSignals {
        entities: snapshot.entities.clone(),
        apis: snapshot.api_endpoints.clone(),
        workflows: snapshot.workflows.clone(),
        events: snapshot.runtime_events.clone(),
        code_symbols: snapshot.code_symbols.clone(),
        db_signals: snapshot.db_signals.clone(),
    });

    build_system_graph_version(version, subsystems, snapshot.relationships.clone())
}

fn default_ownership_policies() -> Vec<OwnershipPolicy> {
    vec![
        OwnershipPolicy {
            subsystem: "Billing".to_string(),
            owns: vec![
                "Invoice".to_string(),
                "Payment".to_string(),
                "Subscription".to_string(),
            ],
        },
        OwnershipPolicy {
            subsystem: "Identity".to_string(),
            owns: vec![
                "User".to_string(),
                "Tenant".to_string(),
                "Session".to_string(),
            ],
        },
        OwnershipPolicy {
            subsystem: "Notifications".to_string(),
            owns: vec![
                "Message".to_string(),
                "Template".to_string(),
                "Delivery".to_string(),
            ],
        },
    ]
}

fn detect_architecture_drift(
    before: &DevelopmentSnapshot,
    current: &DevelopmentSnapshot,
) -> Vec<String> {
    let before_set: BTreeSet<&str> = before.domains.iter().map(String::as_str).collect();
    let new_domains: Vec<&str> = current
        .domains
        .iter()
        .map(String::as_str)
        .filter(|domain| !before_set.contains(*domain))
        .collect();
    let new_ratio = if current.domains.is_empty() {
        0.0
    } else {
        new_domains.len() as f32 / current.domains.len() as f32
    };
    let confidence = (65.0 + (new_ratio * 30.0)).round().clamp(65.0, 95.0) as u8;
    let mut drift = Vec::new();
    for domain in new_domains {
        drift.push(format!(
            "New domain `{domain}` emerging (confidence: {confidence}%)"
        ));
    }
    drift.sort();
    drift
}

fn detect_subsystem_alert(
    before: &DevelopmentSnapshot,
    current: &DevelopmentSnapshot,
) -> Option<String> {
    let entity_growth = current.entities.len().saturating_sub(before.entities.len());
    let workflow_growth = current
        .workflows
        .len()
        .saturating_sub(before.workflows.len());
    let integration_growth = current
        .integrations
        .len()
        .saturating_sub(before.integrations.len());
    if entity_growth >= 4 && workflow_growth >= 1 && integration_growth >= 1 {
        Some(format!(
            "This change set is subsystem-scale: +{entity_growth} entities, +{workflow_growth} workflows, +{integration_growth} integrations"
        ))
    } else {
        None
    }
}

fn dedupe_sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_system_diff_with_capability_and_drift_detection() {
        let previous = DevelopmentSnapshot {
            entities: vec!["Customer".to_string()],
            api_endpoints: vec!["GET /customers".to_string()],
            domains: vec!["crm".to_string()],
            ..DevelopmentSnapshot::default()
        };
        let current = DevelopmentSnapshot {
            entities: vec!["Customer".to_string(), "Invoice".to_string()],
            relationships: vec!["Customer -> Invoice".to_string()],
            api_endpoints: vec![
                "GET /customers".to_string(),
                "POST /invoices".to_string(),
                "GET /invoices".to_string(),
            ],
            migration_tables: vec!["invoices".to_string()],
            db_signals: vec!["table:invoices".to_string()],
            domains: vec!["crm".to_string(), "billing".to_string()],
            ..DevelopmentSnapshot::default()
        };

        let diff = compute_system_diff(Some(&previous), &current);
        assert!(diff.entities_added.contains(&"Invoice".to_string()));
        assert!(diff
            .new_capabilities
            .iter()
            .any(|capability| capability.contains("Entity: Invoice")));
        assert!(diff
            .new_capabilities
            .iter()
            .any(|capability| capability.contains("Database: new table `invoices`")));
        assert!(diff
            .architecture_drift
            .iter()
            .any(|drift| drift.contains("billing")));
        assert!(diff
            .detected_subsystems
            .iter()
            .any(|subsystem| subsystem.id == "Billing"));
        assert!(diff.architecture_confidence > 0);
    }

    #[test]
    fn renders_human_readable_report_sections() {
        let report = SystemDiffReport {
            summary: "System Change: +1 entities, +1 relationships, +1 APIs, +0 workflows"
                .to_string(),
            new_capabilities: vec!["Entity: Refund".to_string()],
            relationships_added: vec!["Order -> Refund".to_string()],
            potential_breaks: vec!["Refund lacks status".to_string()],
            architecture_drift: vec!["New domain `billing` emerging (confidence: 91%)".to_string()],
            ..SystemDiffReport::default()
        };
        let text = render_report(&report);
        assert!(text.contains("New Capability Detected"));
        assert!(text.contains("Relationships"));
        assert!(text.contains("Potential Issues"));
        assert!(text.contains("Architecture Drift"));
        assert!(text.contains("Architecture confidence"));
    }

    #[test]
    fn extracts_tables_from_migration_lines() {
        assert_eq!(
            clean_table_name("invoices ( id uuid );"),
            "invoices".to_string()
        );
        assert_eq!(
            extract_after_token("create table invoices", "create table"),
            Some("invoices")
        );
    }

    #[test]
    fn appends_and_queries_evidence() {
        let temp_dir = std::env::temp_dir().join("treehouse-observer-evidence-test");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let snapshot = DevelopmentSnapshot {
            generated_at_unix: 42,
            entities: vec!["Invoice".to_string()],
            api_endpoints: vec!["POST /invoices".to_string()],
            git_changes: vec!["M src/lib.rs".to_string()],
            ..DevelopmentSnapshot::default()
        };
        let report = SystemDiffReport {
            summary: "System Change: +1 entities, +0 relationships, +1 APIs, +0 workflows"
                .to_string(),
            new_capabilities: vec!["Entity: Invoice".to_string()],
            ..SystemDiffReport::default()
        };
        let written = append_snapshot_evidence(&temp_dir, &snapshot, Some(&report)).unwrap();
        assert!(!written.nodes.is_empty());

        let entities = query_evidence(
            &temp_dir,
            &EvidenceQuery::new().kind("entity").since_unix(1),
        )
        .unwrap();
        assert_eq!(entities.len(), 1);
    }
}
