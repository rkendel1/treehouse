use std::{fs, path::Path};

use anyhow::{Context, Result};
use treehouse_core::Document;

pub fn parse_json_str(input: &str) -> Result<Document> {
    let root = serde_json::from_str(input).context("failed to parse JSON")?;
    Ok(Document::new(root, input.len()))
}

pub fn parse_json_file(path: &Path) -> Result<Document> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;
    parse_json_str(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_json() {
        let doc = parse_json_str("{\"name\":\"treehouse\",\"items\":[1,2]}").unwrap();
        assert_eq!(doc.root_meta().child_count, 2);
        assert_eq!(doc.source_len(), 34);
    }

    #[test]
    fn fails_on_invalid_json() {
        let err = parse_json_str("{ invalid").unwrap_err();
        assert!(err.to_string().contains("failed to parse JSON"));
    }
}
