use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use treehouse_application_model::ApplicationModel;
use treehouse_convex::compile_convex;
use treehouse_graph::{GraphSource, UniversalDataGraph};
use treehouse_mock::run_mock_server;
use treehouse_model_inference::infer_application_model;
use treehouse_parser::parse_structured_file;
use treehouse_postgres::compile_postgres;

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
        _ => bail!("unknown command `{command}`.\n{}", usage()),
    }
}

fn usage() -> &'static str {
    "usage:
  treehouse mock <model-file>
  treehouse analyze <structured files...>
  treehouse compile --target <postgres|convex> [--output dir] <structured files...>"
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
}
