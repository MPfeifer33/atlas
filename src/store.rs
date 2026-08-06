use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::graph::{CodeGraph, IndexedFileMeta, INDEX_SCHEMA_VERSION};
use crate::scan;
use crate::AtlasError;

const ATLAS_DIR: &str = ".agent-atlas";
const GRAPH_FILE: &str = "graph.json";

fn atlas_dir(repo: &Path) -> PathBuf {
    repo.join(ATLAS_DIR)
}

pub fn save(repo: &Path, graph: &CodeGraph) -> Result<(), AtlasError> {
    let dir = atlas_dir(repo);
    std::fs::create_dir_all(&dir)?;

    // Write .gitignore
    let gitignore = dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, "*\n")?;
    }

    let filepath = dir.join(GRAPH_FILE);
    let json = serde_json::to_string_pretty(graph)?;
    std::fs::write(&filepath, json)?;

    Ok(())
}

pub fn load(repo: &Path) -> Result<Option<CodeGraph>, AtlasError> {
    let filepath = atlas_dir(repo).join(GRAPH_FILE);
    if !filepath.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&filepath)?;
    let mut graph: CodeGraph = serde_json::from_str(&content)?;
    graph.build_rdeps();
    Ok(Some(graph))
}

pub fn has_index(repo: &Path) -> bool {
    atlas_dir(repo).join(GRAPH_FILE).exists()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexFreshness {
    pub has_metadata: bool,
    pub schema_version: Option<u32>,
    pub expected_schema_version: u32,
    pub generated_at_unix_ms: Option<u64>,
    pub stale: bool,
    pub schema_mismatch: bool,
    pub changed_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub new_files: Vec<String>,
}

impl IndexFreshness {
    pub fn summary(&self) -> String {
        if !self.has_metadata {
            return "index has no metadata; run atlas scan --force".to_string();
        }
        if !self.stale {
            return "index is current".to_string();
        }

        let mut reasons = Vec::new();
        if self.schema_mismatch {
            reasons.push("schema mismatch".to_string());
        }
        if !self.changed_files.is_empty() {
            reasons.push(format!("{} changed", self.changed_files.len()));
        }
        if !self.missing_files.is_empty() {
            reasons.push(format!("{} missing", self.missing_files.len()));
        }
        if !self.new_files.is_empty() {
            reasons.push(format!("{} new", self.new_files.len()));
        }

        format!("index may be stale ({})", reasons.join(", "))
    }
}

pub fn freshness(repo: &Path, graph: &CodeGraph) -> Result<IndexFreshness, AtlasError> {
    let Some(metadata) = &graph.metadata else {
        return Ok(IndexFreshness {
            has_metadata: false,
            schema_version: None,
            expected_schema_version: INDEX_SCHEMA_VERSION,
            generated_at_unix_ms: None,
            stale: true,
            schema_mismatch: true,
            changed_files: Vec::new(),
            missing_files: Vec::new(),
            new_files: Vec::new(),
        });
    };

    let current = scan::current_source_file_metadata(repo)?;
    let old_by_path: HashMap<&str, &IndexedFileMeta> = metadata
        .indexed_files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let current_by_path: HashMap<&str, &IndexedFileMeta> = current
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();

    let mut changed_files: Vec<String> = old_by_path
        .iter()
        .filter_map(|(path, old)| {
            current_by_path.get(path).and_then(|current| {
                if old.modified_unix_ms != current.modified_unix_ms
                    || old.byte_len != current.byte_len
                {
                    Some((*path).to_string())
                } else {
                    None
                }
            })
        })
        .collect();
    let mut missing_files: Vec<String> = old_by_path
        .keys()
        .filter(|path| !current_by_path.contains_key(**path))
        .map(|path| (*path).to_string())
        .collect();

    let old_paths: HashSet<&str> = old_by_path.keys().copied().collect();
    let mut new_files: Vec<String> = current_by_path
        .keys()
        .filter(|path| !old_paths.contains(**path))
        .map(|path| (*path).to_string())
        .collect();

    changed_files.sort();
    missing_files.sort();
    new_files.sort();

    let schema_mismatch = metadata.schema_version != INDEX_SCHEMA_VERSION;
    let stale = schema_mismatch
        || !changed_files.is_empty()
        || !missing_files.is_empty()
        || !new_files.is_empty();

    Ok(IndexFreshness {
        has_metadata: true,
        schema_version: Some(metadata.schema_version),
        expected_schema_version: INDEX_SCHEMA_VERSION,
        generated_at_unix_ms: Some(metadata.generated_at_unix_ms),
        stale,
        schema_mismatch,
        changed_files,
        missing_files,
        new_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_project;
    use std::fs;

    #[test]
    fn freshness_flags_graphs_without_metadata_as_stale() {
        let temp = tempfile::tempdir().unwrap();
        let graph = CodeGraph::new();

        let freshness = freshness(temp.path(), &graph).unwrap();

        assert!(freshness.stale);
        assert!(!freshness.has_metadata);
        assert!(freshness.schema_mismatch);
    }

    #[test]
    fn freshness_detects_changed_new_and_missing_files() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.rs"), "pub fn a() {}\n").unwrap();
        fs::write(src.join("gone.rs"), "pub fn gone() {}\n").unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let fresh = freshness(temp.path(), &graph).unwrap();
        assert!(!fresh.stale);

        fs::write(
            src.join("a.rs"),
            "pub fn a_changed() {}\npub fn extra() {}\n",
        )
        .unwrap();
        fs::write(src.join("new.rs"), "pub fn new_file() {}\n").unwrap();
        fs::remove_file(src.join("gone.rs")).unwrap();

        let stale = freshness(temp.path(), &graph).unwrap();
        assert!(stale.stale);
        assert_eq!(stale.changed_files, vec!["src/a.rs".to_string()]);
        assert_eq!(stale.missing_files, vec!["src/gone.rs".to_string()]);
        assert_eq!(stale.new_files, vec!["src/new.rs".to_string()]);
    }
}
