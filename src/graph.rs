use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

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
    pub nodes: HashMap<String, FileNode>,
    /// Reverse dependency index: file -> files that depend on it
    #[serde(skip)]
    pub rdeps: HashMap<String, Vec<String>>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
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

        result.sort_by_key(|e| e.depth);
        result
    }

    /// Summary statistics.
    pub fn stats(&self) -> GraphStats {
        let mut by_language: HashMap<String, usize> = HashMap::new();
        let mut total_lines = 0;
        let mut total_deps = 0;

        for node in self.nodes.values() {
            *by_language
                .entry(node.language.label().to_string())
                .or_default() += 1;
            total_lines += node.lines;
            total_deps += node.deps.len();
        }

        GraphStats {
            total_files: self.nodes.len(),
            total_lines,
            total_deps,
            by_language,
        }
    }
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
    pub by_language: HashMap<String, usize>,
}
