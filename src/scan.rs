use std::path::Path;
use walkdir::WalkDir;

use crate::graph::{CodeGraph, FileNode, Language};
use crate::parse;
use crate::AtlasError;

/// Scan a project directory and build the code graph.
pub fn scan_project(repo: &Path) -> Result<CodeGraph, AtlasError> {
    let mut graph = CodeGraph::new();

    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|e| !is_ignored(e.path(), repo))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let language = Language::from_extension(ext);
        if matches!(language, Language::Unknown) {
            continue;
        }

        let rel_path = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary or unreadable files
        };

        let lines = content.lines().count();
        let imports = parse::extract_imports(&content, language);
        let exports = parse::extract_exports(&content, language);

        let node = FileNode {
            path: rel_path,
            language,
            imports,
            deps: Vec::new(), // Resolved after all files scanned
            exports,
            lines,
        };

        graph.add_node(node);
    }

    // Resolve imports to actual file paths in the graph
    resolve_deps(&mut graph, repo);

    // Build reverse index
    graph.build_rdeps();

    Ok(graph)
}

/// Resolve raw import strings to actual file paths in the graph.
fn resolve_deps(graph: &mut CodeGraph, repo: &Path) {
    let all_paths: Vec<String> = graph.nodes.keys().cloned().collect();

    for path in &all_paths {
        let node = graph.nodes.get(path).unwrap();
        let language = node.language;
        let imports = node.imports.clone();
        let file_path = path.clone();

        let mut deps = Vec::new();
        for import in &imports {
            if let Some(resolved) = resolve_import(import, &file_path, language, &all_paths, repo) {
                if resolved != file_path {
                    deps.push(resolved);
                }
            }
        }

        deps.sort();
        deps.dedup();

        if let Some(node) = graph.nodes.get_mut(path) {
            node.deps = deps;
        }
    }
}

/// Try to resolve a single import to a file path in the graph.
fn resolve_import(
    import: &str,
    _source_file: &str,
    language: Language,
    all_paths: &[String],
    _repo: &Path,
) -> Option<String> {
    match language {
        Language::Rust => resolve_rust_import(import, all_paths),
        Language::TypeScript | Language::JavaScript => resolve_js_import(import, all_paths),
        Language::Python => resolve_python_import(import, all_paths),
        Language::Go => resolve_go_import(import, all_paths),
        Language::Unknown => None,
    }
}

fn resolve_rust_import(import: &str, all_paths: &[String]) -> Option<String> {
    // "crate::foo::bar" -> try src/foo/bar.rs, src/foo/bar/mod.rs, src/foo.rs
    let cleaned = import
        .trim_start_matches("crate::")
        .trim_start_matches("super::")
        .replace("::", "/");

    let candidates = vec![
        format!("src/{}.rs", cleaned),
        format!("src/{}/mod.rs", cleaned),
        // Also try just the first segment for "use foo::bar::Baz"
        format!("src/{}.rs", cleaned.split('/').next().unwrap_or("")),
    ];

    for candidate in candidates {
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_js_import(import: &str, all_paths: &[String]) -> Option<String> {
    // Relative imports: "./foo" or "../bar"
    if !import.starts_with('.') {
        return None; // Skip node_modules
    }

    let cleaned = import.trim_start_matches("./").trim_start_matches("../");
    let extensions = ["", ".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.js"];

    for ext in &extensions {
        let candidate = format!("src/{}{}", cleaned, ext);
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
        // Also try without src/ prefix
        let candidate = format!("{}{}", cleaned, ext);
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_python_import(import: &str, all_paths: &[String]) -> Option<String> {
    let as_path = import.replace('.', "/");
    let candidates = vec![
        format!("{}.py", as_path),
        format!("{}/__init__.py", as_path),
        format!("src/{}.py", as_path),
    ];

    for candidate in candidates {
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_go_import(import: &str, all_paths: &[String]) -> Option<String> {
    // Go imports are package paths, hard to resolve without go.mod
    // Best effort: match last segment
    let last_segment = import.rsplit('/').next()?;
    all_paths.iter().find(|p| {
        p.ends_with(&format!("{}/", last_segment)) || p.contains(&format!("/{}.go", last_segment))
    }).cloned()
}

/// Directories/patterns to skip during scanning.
fn is_ignored(path: &Path, repo: &Path) -> bool {
    let rel = path.strip_prefix(repo).unwrap_or(path);
    let name = rel
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    matches!(
        name,
        ".git" | "target" | "node_modules" | "__pycache__"
        | ".venv" | "venv" | "dist" | "build" | ".next"
        | ".agent-witness" | ".agent-atlas" | "vendor"
    )
}
