use std::path::{Path, PathBuf};

use crate::graph::CodeGraph;
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
