use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use treehouse_core::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentFormat {
    Json,
    Jsonl,
    Yaml,
    Toml,
}

#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub path: PathBuf,
    pub format: DocumentFormat,
    pub document: Document,
}

pub fn detect_format(path: &Path) -> Option<DocumentFormat> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "json" => Some(DocumentFormat::Json),
        "jsonl" | "ndjson" => Some(DocumentFormat::Jsonl),
        "yaml" | "yml" => Some(DocumentFormat::Yaml),
        "toml" => Some(DocumentFormat::Toml),
        _ => None,
    }
}

pub fn parse_json_str(input: &str) -> Result<Document> {
    parse_str_with_format(input, DocumentFormat::Json)
}

pub fn parse_str_with_format(input: &str, format: DocumentFormat) -> Result<Document> {
    let root = match format {
        DocumentFormat::Json => serde_json::from_str(input).context("failed to parse JSON")?,
        DocumentFormat::Jsonl => {
            let mut rows = Vec::new();
            for line in input.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                rows.push(
                    serde_json::from_str(trimmed)
                        .context("failed to parse JSONL line as JSON value")?,
                );
            }
            serde_json::Value::Array(rows)
        }
        DocumentFormat::Yaml => serde_yaml::from_str(input).context("failed to parse YAML")?,
        DocumentFormat::Toml => {
            let toml_value: toml::Value = toml::from_str(input).context("failed to parse TOML")?;
            serde_json::to_value(toml_value)
                .context("failed to convert TOML to JSON representation")?
        }
    };

    Ok(Document::new(root, input.len()))
}

pub fn parse_structured_file(path: &Path) -> Result<ParsedDocument> {
    let format = detect_format(path)
        .ok_or_else(|| anyhow!("unsupported file format: {}", path.display()))?;

    let file =
        File::open(path).with_context(|| format!("failed to open file: {}", path.display()))?;
    let source_len = file
        .metadata()
        .with_context(|| format!("failed to stat file: {}", path.display()))?
        .len() as usize;

    // SAFETY: The mapping is read-only and valid for this scope while we parse from it.
    // The mmap keeps the underlying OS mapping alive independently of the `File` object,
    // and this assumes the file is not truncated or deleted during mapping usage.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("failed to memory-map file: {}", path.display()))?;

    let document = match format {
        DocumentFormat::Json => {
            let root = serde_json::from_slice(&mmap).context("failed to parse JSON")?;
            Document::new(root, source_len)
        }
        DocumentFormat::Jsonl | DocumentFormat::Yaml | DocumentFormat::Toml => {
            let source = std::str::from_utf8(&mmap).with_context(|| {
                format!(
                    "failed to read UTF-8 text for {} file: {}",
                    match format {
                        DocumentFormat::Jsonl => "JSONL",
                        DocumentFormat::Yaml => "YAML",
                        DocumentFormat::Toml => "TOML",
                        DocumentFormat::Json => "JSON",
                    },
                    path.display()
                )
            })?;
            parse_str_with_format(source, format)?
        }
    };

    Ok(ParsedDocument {
        path: path.to_path_buf(),
        format,
        document,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn detects_extensions() {
        assert_eq!(
            detect_format(Path::new("a.json")),
            Some(DocumentFormat::Json)
        );
        assert_eq!(
            detect_format(Path::new("a.jsonl")),
            Some(DocumentFormat::Jsonl)
        );
        assert_eq!(
            detect_format(Path::new("a.yaml")),
            Some(DocumentFormat::Yaml)
        );
        assert_eq!(
            detect_format(Path::new("a.toml")),
            Some(DocumentFormat::Toml)
        );
        assert_eq!(detect_format(Path::new("a.xml")), None);
    }

    #[test]
    fn parses_valid_json() {
        let doc = parse_json_str("{\"name\":\"treehouse\",\"items\":[1,2]}").unwrap();
        assert_eq!(doc.root_meta().child_count, 2);
        assert_eq!(doc.source_len(), 34);
    }

    #[test]
    fn parses_yaml_and_toml_strings() {
        let yaml = parse_str_with_format(
            "name: treehouse\nitems:\n  - 1\n  - 2\n",
            DocumentFormat::Yaml,
        )
        .unwrap();
        assert_eq!(yaml.root_meta().child_count, 2);

        let toml = parse_str_with_format("name = \"treehouse\"\ncount = 2\n", DocumentFormat::Toml)
            .unwrap();
        assert_eq!(toml.root_meta().child_count, 2);

        let jsonl =
            parse_str_with_format("{\"id\":1}\n{\"id\":2}\n", DocumentFormat::Jsonl).unwrap();
        assert_eq!(jsonl.root_meta().child_count, 2);
    }

    #[test]
    fn parses_from_file_with_mmap() {
        let temp_path = std::env::temp_dir().join("treehouse-parser-test.yaml");
        fs::write(&temp_path, "ok: true\n").unwrap();

        let parsed = parse_structured_file(&temp_path).unwrap();
        fs::remove_file(&temp_path).unwrap();

        assert_eq!(parsed.format, DocumentFormat::Yaml);
        assert_eq!(parsed.document.root_meta().child_count, 1);
    }

    #[test]
    fn fails_on_invalid_json() {
        let err = parse_json_str("{ invalid").unwrap_err();
        assert!(err.to_string().contains("failed to parse JSON"));
    }
}
