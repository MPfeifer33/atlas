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
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

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
            for resolved in resolve_import(import, &file_path, language, &all_paths, repo) {
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
    source_file: &str,
    language: Language,
    all_paths: &[String],
    _repo: &Path,
) -> Vec<String> {
    match language {
        Language::Rust => resolve_rust_import(import, source_file, all_paths),
        Language::TypeScript | Language::JavaScript => {
            resolve_js_import(import, all_paths).into_iter().collect()
        }
        Language::Python => resolve_python_import(import, all_paths)
            .into_iter()
            .collect(),
        Language::Go => resolve_go_import(import, all_paths).into_iter().collect(),
        Language::Unknown => Vec::new(),
    }
}

fn resolve_rust_import(import: &str, source_file: &str, all_paths: &[String]) -> Vec<String> {
    let src_root = rust_src_root(source_file);
    let source_module = rust_module_segments(source_file, &src_root);
    let mut resolved = Vec::new();

    for expanded in expand_rust_import(import) {
        let Some(segments) = rust_import_segments(&expanded, &source_module) else {
            continue;
        };

        if let Some(path) = resolve_rust_segments(&src_root, &segments, all_paths) {
            resolved.push(path);
        }
    }

    resolved.sort();
    resolved.dedup();
    resolved
}

fn expand_rust_import(import: &str) -> Vec<String> {
    let import = strip_rust_alias(import.trim());

    let Some((prefix, group)) = import.split_once("::{") else {
        return vec![import.to_string()];
    };

    let group = group.trim_end_matches('}');
    group
        .split(',')
        .filter_map(|item| {
            let item = strip_rust_alias(item.trim());
            if item.is_empty() {
                None
            } else if item == "self" {
                Some(prefix.to_string())
            } else {
                Some(format!("{prefix}::{item}"))
            }
        })
        .collect()
}

fn strip_rust_alias(import: &str) -> &str {
    import.split(" as ").next().unwrap_or(import).trim()
}

fn rust_import_segments(import: &str, source_module: &[String]) -> Option<Vec<String>> {
    let parts: Vec<&str> = import.split("::").filter(|part| !part.is_empty()).collect();
    let (head, tail) = parts.split_first()?;

    match *head {
        "crate" => Some(tail.iter().map(|part| (*part).to_string()).collect()),
        "self" => {
            let mut segments = source_module.to_vec();
            segments.extend(tail.iter().map(|part| (*part).to_string()));
            Some(segments)
        }
        "super" => {
            let mut segments = source_module.to_vec();
            let mut tail_start = 0;
            while tail_start < tail.len() && tail[tail_start] == "super" {
                segments.pop();
                tail_start += 1;
            }
            segments.pop();
            segments.extend(tail[tail_start..].iter().map(|part| (*part).to_string()));
            Some(segments)
        }
        _ => None,
    }
}

fn resolve_rust_segments(
    src_root: &str,
    segments: &[String],
    all_paths: &[String],
) -> Option<String> {
    for len in (1..=segments.len()).rev() {
        let module_path = segments[..len].join("/");
        let file_candidate = format!("{src_root}/{module_path}.rs");
        if all_paths.contains(&file_candidate) {
            return Some(file_candidate);
        }

        let mod_candidate = format!("{src_root}/{module_path}/mod.rs");
        if all_paths.contains(&mod_candidate) {
            return Some(mod_candidate);
        }
    }

    for candidate in [
        format!("{src_root}/lib.rs"),
        format!("{src_root}/main.rs"),
        format!("{src_root}/mod.rs"),
    ] {
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn rust_src_root(source_file: &str) -> String {
    let parts: Vec<&str> = source_file.split('/').collect();
    if let Some(index) = parts.iter().rposition(|part| *part == "src") {
        parts[..=index].join("/")
    } else {
        "src".to_string()
    }
}

fn rust_module_segments(source_file: &str, src_root: &str) -> Vec<String> {
    let src_prefix = format!("{src_root}/");
    let rel_path = source_file
        .strip_prefix(&src_prefix)
        .unwrap_or(source_file)
        .strip_suffix(".rs")
        .unwrap_or(source_file);

    let mut segments: Vec<String> = rel_path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();

    match segments.last().map(String::as_str) {
        Some("mod") => {
            segments.pop();
        }
        Some("lib" | "main") if segments.len() == 1 => {
            segments.pop();
        }
        _ => {}
    }

    segments
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
    all_paths
        .iter()
        .find(|p| {
            p.ends_with(&format!("{}/", last_segment))
                || p.contains(&format!("/{}.go", last_segment))
        })
        .cloned()
}

/// Directories/patterns to skip during scanning.
fn is_ignored(path: &Path, repo: &Path) -> bool {
    let rel = path.strip_prefix(repo).unwrap_or(path);
    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");

    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".venv"
            | "venv"
            | "dist"
            | "build"
            | ".next"
            | ".agent-witness"
            | ".agent-atlas"
            | "vendor"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_rust_deps_in_nested_crate_root() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("app/src-tauri/src");
        fs::create_dir_all(src.join("memory")).unwrap();

        fs::write(src.join("lib.rs"), "pub mod memory;\n").unwrap();
        fs::write(
            src.join("memory/mod.rs"),
            "pub mod hillock_engine;\npub mod store;\npub mod types;\npub enum MemoryError {}\n",
        )
        .unwrap();
        fs::write(src.join("memory/types.rs"), "pub struct ClusterSummary;\n").unwrap();
        fs::write(src.join("memory/store.rs"), "pub struct MemoryCategory;\n").unwrap();
        fs::write(
            src.join("memory/hillock_engine.rs"),
            "use super::types::{ClusterSummary};\nuse super::MemoryError;\nuse crate::memory::store::MemoryCategory;\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph
            .nodes
            .get("app/src-tauri/src/memory/hillock_engine.rs")
            .unwrap();

        assert!(node
            .deps
            .contains(&"app/src-tauri/src/memory/types.rs".to_string()));
        assert!(node
            .deps
            .contains(&"app/src-tauri/src/memory/store.rs".to_string()));
        assert!(node
            .deps
            .contains(&"app/src-tauri/src/memory/mod.rs".to_string()));
    }

    #[test]
    fn resolves_mod_declarations_relative_to_parent_module() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("memory")).unwrap();

        fs::write(src.join("lib.rs"), "pub mod memory;\n").unwrap();
        fs::write(src.join("memory/mod.rs"), "pub mod types;\n").unwrap();
        fs::write(src.join("memory/types.rs"), "pub struct ClusterSummary;\n").unwrap();

        let graph = scan_project(temp.path()).unwrap();

        assert_eq!(
            graph.nodes["src/lib.rs"].deps,
            vec!["src/memory/mod.rs".to_string()]
        );
        assert_eq!(
            graph.nodes["src/memory/mod.rs"].deps,
            vec!["src/memory/types.rs".to_string()]
        );
    }
}
