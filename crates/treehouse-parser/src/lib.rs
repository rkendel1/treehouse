use std::{fs::File, path::Path};

use anyhow::{Context, Result};
use memmap2::Mmap;
use treehouse_core::Document;

pub fn parse_json_str(input: &str) -> Result<Document> {
    let root = serde_json::from_str(input).context("failed to parse JSON")?;
    Ok(Document::new(root, input.len()))
}

pub fn parse_json_file(path: &Path) -> Result<Document> {
    let file =
        File::open(path).with_context(|| format!("failed to open file: {}", path.display()))?;
    let source_len = file
        .metadata()
        .with_context(|| format!("failed to stat file: {}", path.display()))?
        .len() as usize;

    // SAFETY: The mapping is read-only and the file handle stays alive for the lifetime of this scope.
    let mmap = unsafe { Mmap::map(&file) }
        .with_context(|| format!("failed to memory-map file: {}", path.display()))?;

    let root = serde_json::from_slice(&mmap).context("failed to parse JSON")?;
    Ok(Document::new(root, source_len))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parses_valid_json() {
        let doc = parse_json_str("{\"name\":\"treehouse\",\"items\":[1,2]}").unwrap();
        assert_eq!(doc.root_meta().child_count, 2);
        assert_eq!(doc.source_len(), 34);
    }

    #[test]
    fn parses_from_file_with_mmap() {
        let temp_path = std::env::temp_dir().join("treehouse-parser-test.json");
        fs::write(&temp_path, "{\"ok\":true}").unwrap();

        let doc = parse_json_file(&temp_path).unwrap();
        fs::remove_file(&temp_path).unwrap();

        assert_eq!(doc.root_meta().child_count, 1);
        assert_eq!(doc.source_len(), 11);
    }

    #[test]
    fn fails_on_invalid_json() {
        let err = parse_json_str("{ invalid").unwrap_err();
        assert!(err.to_string().contains("failed to parse JSON"));
    }
}
