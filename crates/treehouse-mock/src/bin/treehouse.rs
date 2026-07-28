use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use treehouse_agent::{detect_architecture_change_with_files, infer_subsystem_contracts};
use treehouse_application_model::ApplicationModel;
use treehouse_convex::compile_convex;
use treehouse_drift::OwnershipPolicy;
use treehouse_evidence::EvidenceQuery;
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_mock::run_mock_server;
use treehouse_model_inference::infer_application_model;
use treehouse_observer::{
    append_snapshot_evidence, capture_snapshot, compute_system_diff, load_evidence_snapshot,
    load_state, query_evidence, render_report, save_report, save_state, SnapshotState,
};
use treehouse_parser::parse_structured_file;
use treehouse_postgres::compile_postgres;
use treehouse_scan::{run_scan, summary_markdown, ScanOutputFormat, ScanRequest};
use treehouse_subsystem_engine::{discover_subsystems, SubsystemSignals};
use treehouse_system_graph::{
    append_graph_version, append_knowledge_timeline_entry,
    build_knowledge_graph_from_evidence_snapshot, build_system_graph_from_evidence_snapshot,
    build_system_graph_version, KnowledgeDrift, KnowledgeGraph, KnowledgeTimeline,
    SystemGraphTimeline,
};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        bail!(usage());
    };

    match command.as_str() {
        "mock" => {
            let Some(model_path) = args.next() else {
                bail!("usage: treehouse mock <model-file>");
            };
            run_mock_server(Path::new(&model_path))
        }
        "analyze" => {
            let inputs: Vec<PathBuf> = args.map(PathBuf::from).collect();
            if inputs.is_empty() {
                bail!("usage: treehouse analyze <dump.sql|structured files...>");
            }
            let model = analyze_inputs(&inputs)?;
            print_analysis(&model);
            Ok(())
        }
        "compile" => {
            let mut target = None;
            let mut output_dir = None;
            let mut inputs = Vec::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => {
                        target = args.next();
                    }
                    "--output" => {
                        output_dir = args.next().map(PathBuf::from);
                    }
                    _ => inputs.push(PathBuf::from(arg)),
                }
            }

            let target =
                target.ok_or_else(|| anyhow!("missing --target <postgres|convex> argument"))?;
            if inputs.is_empty() {
                bail!(
                    "usage: treehouse compile --target <postgres|convex> [--output dir] <files...>"
                );
            }

            let model = analyze_inputs(&inputs)?;
            match target.as_str() {
                "postgres" => {
                    let base_dir = output_dir.unwrap_or_else(|| PathBuf::from("generated"));
                    let artifacts = compile_postgres(&model);
                    write_artifacts(
                        &base_dir,
                        artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&base_dir, &model)?;
                    println!("Generated PostgreSQL artifacts in {}", base_dir.display());
                }
                "convex" => {
                    let base_dir = output_dir.unwrap_or_else(|| PathBuf::from("convex"));
                    let artifacts = compile_convex(&model);
                    write_artifacts(
                        &base_dir,
                        artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&base_dir, &model)?;
                    println!("Generated Convex artifacts in {}", base_dir.display());
                }
                _ => bail!("unknown compile target `{target}`. expected postgres or convex"),
            }
            Ok(())
        }
        "project" => {
            let Some(model_path) = args.next() else {
                bail!(
                    "usage: treehouse project <application-model.json> --target <postgres|convex|gateway|all> [--output dir]"
                );
            };

            let mut target = None;
            let mut output_dir = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--target" => target = args.next(),
                    "--output" => output_dir = args.next().map(PathBuf::from),
                    _ => bail!("unknown project argument `{arg}`"),
                }
            }

            let target = target.ok_or_else(|| {
                anyhow!("missing --target <postgres|convex|gateway|all> argument")
            })?;
            let model_path = PathBuf::from(model_path);
            let model = load_application_model(&model_path)?;

            match target.as_str() {
                "postgres" => {
                    let base_dir = output_dir
                        .unwrap_or_else(|| PathBuf::from(".treehouse/projection/postgres"));
                    let artifacts = compile_postgres(&model);
                    write_artifacts(
                        &base_dir,
                        artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&base_dir, &model)?;
                    println!("Generated PostgreSQL projection in {}", base_dir.display());
                }
                "convex" => {
                    let base_dir =
                        output_dir.unwrap_or_else(|| PathBuf::from(".treehouse/projection/convex"));
                    let artifacts = compile_convex(&model);
                    write_artifacts(
                        &base_dir,
                        artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&base_dir, &model)?;
                    println!("Generated Convex projection in {}", base_dir.display());
                }
                "gateway" => {
                    println!(
                        "Starting API gateway projection from {}",
                        model_path.display()
                    );
                    return run_mock_server(&model_path);
                }
                "all" => {
                    let base_dir =
                        output_dir.unwrap_or_else(|| PathBuf::from(".treehouse/projection"));
                    let postgres_dir = base_dir.join("postgres");
                    let convex_dir = base_dir.join("convex");

                    let postgres_artifacts = compile_postgres(&model);
                    write_artifacts(
                        &postgres_dir,
                        postgres_artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&postgres_dir, &model)?;

                    let convex_artifacts = compile_convex(&model);
                    write_artifacts(
                        &convex_dir,
                        convex_artifacts
                            .files
                            .into_iter()
                            .map(|file| (file.relative_path, file.content)),
                    )?;
                    write_model(&convex_dir, &model)?;

                    println!("Generated projections in {}", base_dir.display());
                    println!(
                        "To run API gateway projection: treehouse project {} --target gateway",
                        model_path.display()
                    );
                }
                _ => {
                    bail!(
                        "unknown project target `{target}`. expected postgres, convex, gateway, or all"
                    )
                }
            }
            Ok(())
        }
        "connect" => {
            let Some(repo_path) = args.next() else {
                bail!(
                    "usage: treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous]"
                );
            };
            let mut state_path = None;
            let mut report_path = None;
            let mut interval_secs = 2_u64;
            let mut iterations = 1_u64;
            let mut continuous = false;

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--state" => state_path = args.next().map(PathBuf::from),
                    "--report" => report_path = args.next().map(PathBuf::from),
                    "--interval" => {
                        let raw = args
                            .next()
                            .ok_or_else(|| anyhow!("missing value for --interval"))?;
                        interval_secs = raw
                            .parse::<u64>()
                            .with_context(|| format!("invalid --interval value: {raw}"))?;
                    }
                    "--iterations" => {
                        let raw = args
                            .next()
                            .ok_or_else(|| anyhow!("missing value for --iterations"))?;
                        iterations = raw
                            .parse::<u64>()
                            .with_context(|| format!("invalid --iterations value: {raw}"))?;
                    }
                    "--continuous" => continuous = true,
                    _ => bail!("unknown connect argument `{arg}`"),
                }
            }

            if !continuous && iterations == 0 {
                bail!("--iterations must be at least 1");
            }
            run_connect(
                Path::new(&repo_path),
                state_path.as_deref(),
                report_path.as_deref(),
                interval_secs,
                if continuous { None } else { Some(iterations) },
            )
        }
        "watch" => {
            let Some(repo_path) = args.next() else {
                bail!(
                    "usage: treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous]"
                );
            };
            let mut state_path = None;
            let mut report_path = None;
            let mut interval_secs = 2_u64;
            let mut iterations = 1_u64;
            let mut continuous = false;

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--state" => state_path = args.next().map(PathBuf::from),
                    "--report" => report_path = args.next().map(PathBuf::from),
                    "--interval" => {
                        let raw = args
                            .next()
                            .ok_or_else(|| anyhow!("missing value for --interval"))?;
                        interval_secs = raw
                            .parse::<u64>()
                            .with_context(|| format!("invalid --interval value: {raw}"))?;
                    }
                    "--iterations" => {
                        let raw = args
                            .next()
                            .ok_or_else(|| anyhow!("missing value for --iterations"))?;
                        iterations = raw
                            .parse::<u64>()
                            .with_context(|| format!("invalid --iterations value: {raw}"))?;
                    }
                    "--continuous" => continuous = true,
                    _ => bail!("unknown watch argument `{arg}`"),
                }
            }

            if !continuous && iterations == 0 {
                bail!("--iterations must be at least 1");
            }
            run_watch(
                Path::new(&repo_path),
                state_path.as_deref(),
                report_path.as_deref(),
                interval_secs,
                if continuous { None } else { Some(iterations) },
            )
        }
        "scan" => {
            let Some(repo_path) = args.next() else {
                bail!(
                    "usage: treehouse scan <repo-path> --target <path|name> [--local-llm [heuristic|ollama:<model>]] [--output dir] [--baseline-only] [--goals-only] [--format json|markdown]"
                );
            };
            let mut target = None;
            let mut output = None;
            let mut local_llm: Option<String> = None;
            let mut baseline_only = false;
            let mut goals_only = false;
            let mut format = ScanOutputFormat::Json;

            let scan_args: Vec<String> = args.collect();
            let mut idx = 0;
            while idx < scan_args.len() {
                match scan_args[idx].as_str() {
                    "--target" => {
                        idx += 1;
                        let value = scan_args
                            .get(idx)
                            .ok_or_else(|| anyhow!("missing value for --target"))?;
                        target = Some(value.clone());
                    }
                    "--output" => {
                        idx += 1;
                        let value = scan_args
                            .get(idx)
                            .ok_or_else(|| anyhow!("missing value for --output"))?;
                        output = Some(PathBuf::from(value));
                    }
                    "--local-llm" => {
                        let maybe_value = scan_args.get(idx + 1).cloned();
                        if let Some(value) = maybe_value {
                            if value.starts_with("--") {
                                local_llm = Some("heuristic".to_string());
                            } else {
                                local_llm = Some(value);
                                idx += 1;
                            }
                        } else {
                            local_llm = Some("heuristic".to_string());
                        }
                    }
                    "--baseline-only" => baseline_only = true,
                    "--goals-only" => goals_only = true,
                    "--format" => {
                        idx += 1;
                        let value = scan_args
                            .get(idx)
                            .ok_or_else(|| anyhow!("missing value for --format"))?;
                        format = match value.as_str() {
                            "json" => ScanOutputFormat::Json,
                            "markdown" => ScanOutputFormat::Markdown,
                            _ => bail!("invalid --format value `{value}`. expected json|markdown"),
                        };
                    }
                    arg => bail!("unknown scan argument `{arg}`"),
                }
                idx += 1;
            }

            if !baseline_only && target.is_none() {
                bail!("missing --target <path|name> argument");
            }

            let request = ScanRequest {
                repo_path: PathBuf::from(repo_path),
                target,
                output,
                local_llm,
                baseline_only,
                goals_only,
                format,
            };
            let result = run_scan(&request)?;
            println!("Scan output: {}", result.output_dir.display());
            match format {
                ScanOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result.summary)?);
                }
                ScanOutputFormat::Markdown => {
                    println!("{}", summary_markdown(&result.summary));
                }
            }
            Ok(())
        }
        "evidence" => {
            let Some(action) = args.next() else {
                bail!("usage: treehouse evidence <query|snapshot> ...");
            };
            match action.as_str() {
                "query" => {
                    let mut repo = None;
                    let mut kind = None;
                    let mut subsystem = None;
                    let mut since = None;
                    while let Some(arg) = args.next() {
                        match arg.as_str() {
                            "--repo" => repo = args.next().map(PathBuf::from),
                            "--kind" => kind = args.next(),
                            "--subsystem" => subsystem = args.next(),
                            "--since" => since = args.next(),
                            _ => bail!("unknown evidence query argument `{arg}`"),
                        }
                    }
                    let repo = repo.ok_or_else(|| anyhow!("missing --repo <path>"))?;
                    let mut query = EvidenceQuery::new();
                    if let Some(kind) = kind {
                        query = query.kind(kind);
                    }
                    if let Some(subsystem) = subsystem {
                        query = query.subsystem(subsystem);
                    }
                    if let Some(since_raw) = since {
                        query = query.since_unix(parse_since(&since_raw)?);
                    }
                    let result = query_evidence(&repo, &query)?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    Ok(())
                }
                "snapshot" => {
                    let mut repo = None;
                    let mut output = None;
                    while let Some(arg) = args.next() {
                        match arg.as_str() {
                            "--repo" => repo = args.next().map(PathBuf::from),
                            "--output" => output = args.next().map(PathBuf::from),
                            _ => bail!("unknown evidence snapshot argument `{arg}`"),
                        }
                    }
                    let repo = repo.ok_or_else(|| anyhow!("missing --repo <path>"))?;
                    let output = output.ok_or_else(|| anyhow!("missing --output <path>"))?;
                    let snapshot = load_evidence_snapshot(&repo)?;
                    if let Some(parent) = output.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("failed to create snapshot directory {}", parent.display())
                        })?;
                    }
                    fs::write(&output, serde_json::to_string_pretty(&snapshot)?)
                        .with_context(|| format!("failed to write {}", output.display()))?;
                    println!("Wrote evidence snapshot to {}", output.display());
                    Ok(())
                }
                _ => bail!("unknown evidence command `{action}`"),
            }
        }
        "graph" => {
            let Some(repo_path) = args.next() else {
                bail!("usage: treehouse graph <repo-path> [--contains text] [--type node-type]");
            };
            let mut contains = String::new();
            let mut type_filter = String::new();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--contains" => contains = args.next().unwrap_or_default(),
                    "--type" => type_filter = args.next().unwrap_or_default(),
                    _ => bail!("unknown graph argument `{arg}`"),
                }
            }
            let graph = load_knowledge_graph(Path::new(&repo_path))?;
            let contains = contains.trim().to_ascii_lowercase();
            let type_filter = type_filter.trim().to_ascii_lowercase();
            let nodes: Vec<_> = graph
                .nodes
                .iter()
                .filter(|node| {
                    (contains.is_empty()
                        || node.name.to_ascii_lowercase().contains(&contains)
                        || node.id.to_ascii_lowercase().contains(&contains))
                        && (type_filter.is_empty()
                            || format!("{:?}", node.node_type).to_ascii_lowercase()
                                == type_filter)
                })
                .cloned()
                .collect();
            let node_ids: std::collections::BTreeSet<_> =
                nodes.iter().map(|node| node.id.clone()).collect();
            let edges: Vec<_> = graph
                .edges
                .iter()
                .filter(|edge| node_ids.contains(&edge.from) || node_ids.contains(&edge.to))
                .cloned()
                .collect();

            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "repository": graph.repository,
                    "version": graph.version,
                    "nodes": nodes,
                    "edges": edges,
                }))?
            );
            Ok(())
        }
        "why" => {
            let Some(repo_path) = args.next() else {
                bail!("usage: treehouse why <repo-path> <term>");
            };
            let Some(term) = args.next() else {
                bail!("usage: treehouse why <repo-path> <term>");
            };
            let graph = load_knowledge_graph(Path::new(&repo_path))?;
            let term_lower = term.to_ascii_lowercase();
            let matched: Vec<_> = graph
                .nodes
                .iter()
                .filter(|node| {
                    node.name.to_ascii_lowercase().contains(&term_lower)
                        || node.id.to_ascii_lowercase().contains(&term_lower)
                })
                .cloned()
                .collect();
            let matched_ids: std::collections::BTreeSet<_> =
                matched.iter().map(|node| node.id.clone()).collect();
            let related_edges: Vec<_> = graph
                .edges
                .iter()
                .filter(|edge| matched_ids.contains(&edge.from) || matched_ids.contains(&edge.to))
                .cloned()
                .collect();
            let drift_findings: Vec<_> = graph
                .drifts
                .iter()
                .filter(|drift| {
                    drift.title.to_ascii_lowercase().contains(&term_lower)
                        || drift.message.to_ascii_lowercase().contains(&term_lower)
                })
                .cloned()
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "term": term,
                    "matches": matched,
                    "related_edges": related_edges,
                    "drift": drift_findings,
                }))?
            );
            Ok(())
        }
        "drift" => {
            let Some(repo_path) = args.next() else {
                bail!("usage: treehouse drift <repo-path>");
            };
            let path = Path::new(&repo_path).join(".treehouse/knowledge/drift/report.json");
            let content = fs::read_to_string(&path)
                .with_context(|| format!("failed reading {}", path.display()))?;
            println!("{}", content);
            Ok(())
        }
        _ => bail!("unknown command `{command}`.\n{}", usage()),
    }
}

fn usage() -> &'static str {
    "usage:
  treehouse mock <model-file>
  treehouse analyze <structured files...>
  treehouse compile --target <postgres|convex> [--output dir] <structured files...>
    treehouse project <application-model.json> --target <postgres|convex|gateway|all> [--output dir]
    treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous]
    treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous]
  treehouse scan <repo-path> --target <path|name> [--local-llm [heuristic|ollama:<model>]] [--output dir] [--baseline-only] [--goals-only] [--format json|markdown]
  treehouse evidence query --repo <path> [--kind kind] [--subsystem id] [--since unix|YYYY-MM-DD]
    treehouse evidence snapshot --repo <path> --output <file>
    treehouse graph <repo-path> [--contains text] [--type node-type]
    treehouse why <repo-path> <term>
    treehouse drift <repo-path>"
}

fn parse_since(raw: &str) -> Result<u64> {
    if let Ok(unix) = raw.parse::<u64>() {
        return Ok(unix);
    }

    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() != 3 {
        bail!("invalid --since value `{raw}`. expected unix or YYYY-MM-DD");
    }
    let year = parts[0]
        .parse::<i32>()
        .with_context(|| format!("invalid year in --since value `{raw}`"))?;
    let month = parts[1]
        .parse::<u32>()
        .with_context(|| format!("invalid month in --since value `{raw}`"))?;
    let day = parts[2]
        .parse::<u32>()
        .with_context(|| format!("invalid day in --since value `{raw}`"))?;
    date_to_unix(year, month, day)
}

fn date_to_unix(year: i32, month: u32, day: u32) -> Result<u64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        bail!("invalid date components");
    }
    let mut days = 0_i64;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let month_days = [31_i64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        days += month_days[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            days += 1;
        }
    }
    days += i64::from(day - 1);
    Ok((days * 86_400).max(0) as u64)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn analyze_inputs(paths: &[PathBuf]) -> Result<ApplicationModel> {
    let parsed = paths
        .iter()
        .map(|path| {
            parse_structured_file(path)
                .with_context(|| format!("failed to parse {}", path.display()))
        })
        .collect::<Result<Vec<_>>>()?;

    let source_names: Vec<String> = parsed
        .iter()
        .map(|parsed| parsed.path.to_string_lossy().to_string())
        .collect();
    let sources: Vec<GraphSource<'_>> = parsed
        .iter()
        .zip(source_names.iter())
        .map(|(parsed, source_name)| GraphSource {
            name: source_name.as_str(),
            document: &parsed.document,
        })
        .collect();
    let graph = UniversalDataGraph::build(&sources);
    Ok(infer_application_model(&graph, None))
}

fn load_application_model(path: &Path) -> Result<ApplicationModel> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read model file {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse application model {}", path.display()))
}

fn load_knowledge_graph(repo_path: &Path) -> Result<KnowledgeGraph> {
    let path = repo_path.join(".treehouse/knowledge/graph.json");
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse knowledge graph {}", path.display()))
}

fn print_analysis(model: &ApplicationModel) {
    println!("Detected Application:");
    println!("{}", model.application.name);
    println!("Entities:");
    for entity in &model.entities {
        println!(" ✓ {}", entity.name);
    }
    println!("Relationships:");
    for entity in &model.entities {
        for relationship in &entity.relationships {
            println!(" ✓ {} → {}", entity.name, relationship.target);
        }
    }
    let confidence = if model.entities.is_empty() {
        0.0
    } else {
        model
            .entities
            .iter()
            .map(|entity| entity.confidence)
            .sum::<f32>()
            / model.entities.len() as f32
    };
    println!("Confidence:");
    println!("{:.0}%", confidence * 100.0);
}

fn write_artifacts<I>(base_dir: &Path, files: I) -> Result<()>
where
    I: IntoIterator<Item = (String, String)>,
{
    for (relative_path, content) in files {
        let full_path = base_dir.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&full_path, content)
            .with_context(|| format!("failed to write {}", full_path.display()))?;
    }
    Ok(())
}

fn write_model(base_dir: &Path, model: &ApplicationModel) -> Result<()> {
    let model_json = serde_json::to_string_pretty(model)?;
    let model_path = base_dir.join("application-model.json");
    fs::write(&model_path, model_json)
        .with_context(|| format!("failed to write {}", model_path.display()))
}

fn run_connect(
    repo_path: &Path,
    state_path: Option<&Path>,
    report_path: Option<&Path>,
    interval_secs: u64,
    iterations: Option<u64>,
) -> Result<()> {
    println!(
        "treehouse connect is supported; for real-time architecture mode use `treehouse watch`."
    );
    run_watch(
        repo_path,
        state_path,
        report_path,
        interval_secs,
        iterations,
    )
}

fn run_watch(
    repo_path: &Path,
    state_path: Option<&Path>,
    report_path: Option<&Path>,
    interval_secs: u64,
    iterations: Option<u64>,
) -> Result<()> {
    let state_path = state_path
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_path.join(".treehouse/development-state.json"));
    let report_path = report_path
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_path.join(".treehouse/system-diff.json"));
    let timeline_path = repo_path.join(".treehouse/system-graph-timeline.json");
    let contracts_path = repo_path.join(".treehouse/subsystem-contracts.json");
    let knowledge_graph_path = repo_path.join(".treehouse/knowledge/graph.json");
    let knowledge_nodes_path = repo_path.join(".treehouse/knowledge/nodes.json");
    let knowledge_edges_path = repo_path.join(".treehouse/knowledge/edges.json");
    let knowledge_drift_path = repo_path.join(".treehouse/knowledge/drift/report.json");
    let knowledge_timeline_path = repo_path.join(".treehouse/knowledge/timeline.json");

    let existing = load_state(&state_path)?;
    let mut previous = existing.as_ref().map(|state| state.snapshot.clone());
    let mut timeline = load_graph_timeline(&timeline_path)?;
    let mut knowledge_timeline = load_knowledge_timeline(&knowledge_timeline_path)?;
    let repository_name = repo_path
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or("repository")
        .to_string();

    println!("Watching application...");
    let mut index = 0_u64;
    loop {
        let current = capture_snapshot(repo_path)?;
        let report = compute_system_diff(previous.as_ref(), &current);
        println!("{}", render_report(&report));
        let evidence_snapshot = append_snapshot_evidence(repo_path, &current, Some(&report))?;
        let current_graph = build_system_graph_from_evidence_snapshot(
            current.generated_at_unix,
            &evidence_snapshot,
        );
        let previous_graph = previous
            .as_ref()
            .map(|snapshot| snapshot_to_graph(snapshot, snapshot.generated_at_unix));
        if timeline.versions.is_empty() {
            if let Some(previous_graph) = previous_graph.as_ref() {
                append_graph_version(&mut timeline, previous_graph.clone(), 0);
            }
        }
        append_graph_version(&mut timeline, current_graph.clone(), 0);
        save_graph_timeline(&timeline_path, &timeline)?;

        let subsystem_contracts = infer_subsystem_contracts(&current_graph);
        save_subsystem_contracts(&contracts_path, &subsystem_contracts)?;

        if let Some(event) = detect_architecture_change_with_files(
            previous_graph.as_ref(),
            &current_graph,
            &default_ownership_policies(),
            &report.changed_files,
        ) {
            println!("{}", serde_json::to_string_pretty(&event)?);
        }

        let mut drift_events = Vec::new();
        for finding in &report.architecture_drift {
            drift_events.push(KnowledgeDrift {
                severity: "HIGH".to_string(),
                title: "Architecture Drift".to_string(),
                message: finding.clone(),
                confidence: 0.85,
            });
        }
        for drift in &report.drift_events {
            drift_events.push(KnowledgeDrift {
                severity: format!("{:?}", drift.recommendation.action),
                title: format!("{:?}", drift.drift_type),
                message: drift.recommendation.details.clone(),
                confidence: 0.90,
            });
        }

        let knowledge_graph = build_knowledge_graph_from_evidence_snapshot(
            &repository_name,
            current.generated_at_unix,
            &evidence_snapshot,
            drift_events,
        );
        save_knowledge_projection(&knowledge_graph_path, &knowledge_graph)?;
        save_knowledge_nodes(&knowledge_nodes_path, &knowledge_graph.nodes)?;
        save_knowledge_edges(&knowledge_edges_path, &knowledge_graph.edges)?;
        save_knowledge_drift(&knowledge_drift_path, &knowledge_graph.drifts)?;
        append_knowledge_timeline_entry(&mut knowledge_timeline, &knowledge_graph, 500);
        save_knowledge_timeline(&knowledge_timeline_path, &knowledge_timeline)?;

        save_report(&report_path, &report)?;
        save_state(
            &state_path,
            &SnapshotState {
                snapshot: current.clone(),
            },
        )?;
        previous = Some(current);
        index += 1;
        if let Some(max_iterations) = iterations {
            if index >= max_iterations {
                break;
            }
        }
        thread::sleep(Duration::from_secs(interval_secs));
    }

    Ok(())
}

fn load_graph_timeline(path: &Path) -> Result<SystemGraphTimeline> {
    if !path.exists() {
        return Ok(SystemGraphTimeline::default());
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    let timeline = serde_json::from_str(&content)
        .with_context(|| format!("failed parsing {}", path.display()))?;
    Ok(timeline)
}

fn save_graph_timeline(path: &Path, timeline: &SystemGraphTimeline) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating timeline directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(timeline)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn save_subsystem_contracts(
    path: &Path,
    contracts: &[treehouse_contracts::SubsystemContract],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating contract directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(contracts)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn load_knowledge_timeline(path: &Path) -> Result<KnowledgeTimeline> {
    if !path.exists() {
        return Ok(KnowledgeTimeline::default());
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    let timeline = serde_json::from_str(&content)
        .with_context(|| format!("failed parsing {}", path.display()))?;
    Ok(timeline)
}

fn save_knowledge_timeline(path: &Path, timeline: &KnowledgeTimeline) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed creating knowledge timeline directory {}",
                parent.display()
            )
        })?;
    }
    let serialized = serde_json::to_string_pretty(timeline)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn save_knowledge_projection(
    path: &Path,
    graph: &treehouse_system_graph::KnowledgeGraph,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating knowledge directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(graph)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn save_knowledge_nodes(path: &Path, nodes: &[treehouse_system_graph::KnowledgeNode]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating knowledge directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(nodes)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn save_knowledge_edges(path: &Path, edges: &[treehouse_system_graph::KnowledgeEdge]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating knowledge directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(edges)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn save_knowledge_drift(path: &Path, drifts: &[KnowledgeDrift]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed creating drift directory {}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(drifts)?;
    fs::write(path, serialized).with_context(|| format!("failed writing {}", path.display()))
}

fn snapshot_to_graph(
    snapshot: &treehouse_observer::DevelopmentSnapshot,
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_inferrs_entity_from_json_input() {
        let temp_dir = std::env::temp_dir().join("treehouse-cli-analyze-test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("customers.json");
        fs::write(
            &file_path,
            r#"[{"id":"c1","email":"alice@example.com","created_at":"2026-01-01T00:00:00Z"}]"#,
        )
        .unwrap();

        let model = analyze_inputs(&[file_path]).unwrap();
        assert!(model
            .entities
            .iter()
            .any(|entity| entity.name == "Customer"));
    }

    #[test]
    fn write_artifacts_creates_nested_paths() {
        let temp_dir = std::env::temp_dir().join("treehouse-cli-write-test");
        let _ = fs::remove_dir_all(&temp_dir);
        write_artifacts(
            &temp_dir,
            vec![("migrations/001.sql".to_string(), "SELECT 1;".to_string())],
        )
        .unwrap();
        let written = fs::read_to_string(temp_dir.join("migrations/001.sql")).unwrap();
        assert_eq!(written, "SELECT 1;");
    }

    #[test]
    fn connect_writes_state_and_report() {
        let temp_dir = std::env::temp_dir().join("treehouse-cli-connect-test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("migrations")).unwrap();
        fs::write(
            temp_dir.join("customers.json"),
            r#"[{"id":"c1","email":"alice@example.com"}]"#,
        )
        .unwrap();
        fs::write(
            temp_dir.join("migrations/001_create_invoices.sql"),
            "CREATE TABLE invoices (id UUID, customer_id UUID, amount DECIMAL);",
        )
        .unwrap();
        fs::write(temp_dir.join("events.log"), "event: invoice.created").unwrap();

        let state = temp_dir.join(".treehouse/state.json");
        let report = temp_dir.join(".treehouse/report.json");
        run_connect(&temp_dir, Some(&state), Some(&report), 1, 1).unwrap();

        assert!(state.exists());
        assert!(report.exists());
        assert!(temp_dir.join(".treehouse/evidence/nodes.jsonl").exists());
        let report_content = fs::read_to_string(report).unwrap();
        assert!(report_content.contains("new_capabilities"));
    }

    #[test]
    fn parses_since_date_string() {
        let unix = parse_since("2026-07-01").unwrap();
        assert!(unix > 0);
        assert_eq!(parse_since("1725148800").unwrap(), 1_725_148_800);
    }

    #[test]
    fn scan_command_writes_summary() {
        let temp_dir = std::env::temp_dir().join("treehouse-cli-scan-test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("targets")).unwrap();
        fs::write(
            temp_dir.join("orders.json"),
            r#"[{"id":"o1","customer":"alice","total":12.5}]"#,
        )
        .unwrap();
        fs::write(
            temp_dir.join("targets/event-driven.md"),
            "# Event Driven\n## Capabilities\n- Add InvoiceProjection",
        )
        .unwrap();

        let request = ScanRequest {
            repo_path: temp_dir.clone(),
            target: Some("event-driven".to_string()),
            output: Some(temp_dir.join(".treehouse/scan/test")),
            local_llm: Some("heuristic".to_string()),
            baseline_only: false,
            goals_only: false,
            format: ScanOutputFormat::Json,
        };
        let result = run_scan(&request).unwrap();
        assert!(result.output_dir.join("summary.json").exists());
    }
}
