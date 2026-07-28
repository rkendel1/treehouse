use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use treehouse_agent::detect_architecture_change;
use treehouse_application_model::ApplicationModel;
use treehouse_convex::compile_convex;
use treehouse_drift::OwnershipPolicy;
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_mock::run_mock_server;
use treehouse_model_inference::infer_application_model;
use treehouse_observer::{
    capture_snapshot, compute_system_diff, load_state, render_report, save_report, save_state,
    SnapshotState,
};
use treehouse_parser::parse_structured_file;
use treehouse_postgres::compile_postgres;
use treehouse_subsystem_engine::{discover_subsystems, SubsystemSignals};
use treehouse_system_graph::build_system_graph_version;

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
        "connect" => {
            let Some(repo_path) = args.next() else {
                bail!(
                    "usage: treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]"
                );
            };
            let mut state_path = None;
            let mut report_path = None;
            let mut interval_secs = 2_u64;
            let mut iterations = 1_u64;

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
                    _ => bail!("unknown connect argument `{arg}`"),
                }
            }

            if iterations == 0 {
                bail!("--iterations must be at least 1");
            }
            run_connect(
                Path::new(&repo_path),
                state_path.as_deref(),
                report_path.as_deref(),
                interval_secs,
                iterations,
            )
        }
        "watch" => {
            let Some(repo_path) = args.next() else {
                bail!(
                    "usage: treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]"
                );
            };
            let mut state_path = None;
            let mut report_path = None;
            let mut interval_secs = 2_u64;
            let mut iterations = 1_u64;

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
                    _ => bail!("unknown watch argument `{arg}`"),
                }
            }

            if iterations == 0 {
                bail!("--iterations must be at least 1");
            }
            run_watch(
                Path::new(&repo_path),
                state_path.as_deref(),
                report_path.as_deref(),
                interval_secs,
                iterations,
            )
        }
        _ => bail!("unknown command `{command}`.\n{}", usage()),
    }
}

fn usage() -> &'static str {
    "usage:
  treehouse mock <model-file>
  treehouse analyze <structured files...>
  treehouse compile --target <postgres|convex> [--output dir] <structured files...>
  treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]
  treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]"
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
    iterations: u64,
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
    iterations: u64,
) -> Result<()> {
    let state_path = state_path
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_path.join(".treehouse/development-state.json"));
    let report_path = report_path
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_path.join(".treehouse/system-diff.json"));

    let existing = load_state(&state_path)?;
    let mut previous = existing.as_ref().map(|state| state.snapshot.clone());

    println!("Watching application...");
    for index in 0..iterations {
        let current = capture_snapshot(repo_path)?;
        let report = compute_system_diff(previous.as_ref(), &current);
        println!("{}", render_report(&report));
        let current_graph = snapshot_to_graph(&current, current.generated_at_unix);
        let previous_graph = previous
            .as_ref()
            .map(|snapshot| snapshot_to_graph(snapshot, snapshot.generated_at_unix));
        if let Some(event) = detect_architecture_change(
            previous_graph.as_ref(),
            &current_graph,
            &default_ownership_policies(),
        ) {
            println!("{}", serde_json::to_string_pretty(&event)?);
        }
        save_report(&report_path, &report)?;
        save_state(
            &state_path,
            &SnapshotState {
                snapshot: current.clone(),
            },
        )?;
        previous = Some(current);
        if index + 1 < iterations {
            thread::sleep(Duration::from_secs(interval_secs));
        }
    }

    Ok(())
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
        let report_content = fs::read_to_string(report).unwrap();
        assert!(report_content.contains("new_capabilities"));
    }
}
