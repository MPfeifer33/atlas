use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub const INDEX_SCHEMA_VERSION: u32 = 2;

/// A node in the knowledge graph — one source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Relative path from repo root
    pub path: String,
    /// Language detected
    pub language: Language,
    /// Raw import/use statements found
    pub imports: Vec<String>,
    /// Resolved dependency paths (other files in the graph)
    pub deps: Vec<String>,
    /// Local imports Atlas expected to resolve but could not match to a file.
    #[serde(default)]
    pub unresolved_imports: Vec<String>,
    /// Imports that appear to point outside the scanned project.
    #[serde(default)]
    pub external_imports: Vec<String>,
    /// Functions/symbols exported (name only, no type info)
    pub exports: Vec<String>,
    /// Line count
    pub lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Unknown,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Language::Rust,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "py" => Language::Python,
            "go" => Language::Go,
            _ => Language::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Python => "python",
            Language::Go => "go",
            Language::Unknown => "unknown",
        }
    }
}

/// The full codebase graph.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CodeGraph {
    /// Metadata used to decide whether the graph still matches the source tree.
    #[serde(default)]
    pub metadata: Option<IndexMetadata>,
    pub nodes: HashMap<String, FileNode>,
    /// Reverse dependency index: file -> files that depend on it
    #[serde(skip)]
    pub rdeps: HashMap<String, Vec<String>>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            metadata: None,
            nodes: HashMap::new(),
            rdeps: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: FileNode) {
        self.nodes.insert(node.path.clone(), node);
    }

    /// Build the reverse dependency index from forward deps.
    pub fn build_rdeps(&mut self) {
        self.rdeps.clear();
        for (path, node) in &self.nodes {
            for dep in &node.deps {
                self.rdeps
                    .entry(dep.clone())
                    .or_default()
                    .push(path.clone());
            }
        }

        for dependents in self.rdeps.values_mut() {
            dependents.sort();
            dependents.dedup();
        }
    }

    /// Get direct reverse dependencies (who depends on this file).
    pub fn rdeps_of(&self, path: &str) -> Vec<&str> {
        self.rdeps
            .get(path)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Compute transitive blast radius via BFS with relationship tracking.
    pub fn blast_radius(&self, path: &str, max_depth: usize) -> Vec<BlastEntry> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut result: Vec<BlastEntry> = Vec::new();
        let mut queue: VecDeque<(String, usize, String)> = VecDeque::new();

        visited.insert(path.to_string());
        queue.push_back((path.to_string(), 0, String::new()));

        while let Some((current, depth, via)) = queue.pop_front() {
            if depth > 0 {
                let fan_out = self.rdeps_of(&current).len();
                result.push(BlastEntry {
                    path: current.clone(),
                    depth,
                    via,
                    fan_out,
                });
            }
            if depth >= max_depth {
                continue;
            }
            for rdep in self.rdeps_of(&current) {
                if !visited.contains(rdep) {
                    visited.insert(rdep.to_string());
                    queue.push_back((rdep.to_string(), depth + 1, current.clone()));
                }
            }
        }

        result.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
        result
    }

    /// Summary statistics.
    pub fn stats(&self) -> GraphStats {
        let mut by_language: HashMap<String, usize> = HashMap::new();
        let mut total_lines = 0;
        let mut total_deps = 0;
        let mut total_unresolved_imports = 0;
        let mut total_external_imports = 0;

        for node in self.nodes.values() {
            *by_language
                .entry(node.language.label().to_string())
                .or_default() += 1;
            total_lines += node.lines;
            total_deps += node.deps.len();
            total_unresolved_imports += node.unresolved_imports.len();
            total_external_imports += node.external_imports.len();
        }

        GraphStats {
            total_files: self.nodes.len(),
            total_lines,
            total_deps,
            total_unresolved_imports,
            total_external_imports,
            by_language,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub schema_version: u32,
    pub generated_at_unix_ms: u64,
    pub atlas_version: String,
    pub repo_path: String,
    pub indexed_files: Vec<IndexedFileMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedFileMeta {
    pub path: String,
    pub modified_unix_ms: u64,
    pub byte_len: u64,
}

/// A single entry in a blast radius result.
#[derive(Debug, Clone, Serialize)]
pub struct BlastEntry {
    /// File path affected
    pub path: String,
    /// Distance from the source file (1 = direct dependent)
    pub depth: usize,
    /// Which file caused this to be in the blast radius
    pub via: String,
    /// How many files depend on this file (downstream risk indicator)
    pub fan_out: usize,
}

#[derive(Debug, Serialize)]
pub struct GraphStats {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_deps: usize,
    pub total_unresolved_imports: usize,
    pub total_external_imports: usize,
    pub by_language: HashMap<String, usize>,
}
