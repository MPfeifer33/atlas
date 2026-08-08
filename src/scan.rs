use std::collections::BTreeMap;
use std::path::Path;
use walkdir::WalkDir;

use crate::graph::{
    CodeGraph, FileNode, IndexMetadata, IndexedFileMeta, Language, INDEX_SCHEMA_VERSION,
};
use crate::parse;
use crate::AtlasError;

/// Scan a project directory and build the code graph.
pub fn scan_project(repo: &Path) -> Result<CodeGraph, AtlasError> {
    let mut graph = CodeGraph::new();
    let mut indexed_files = Vec::new();
    let mut source_contents = BTreeMap::new();

    for source in source_files(repo)? {
        let path = source.path.as_path();
        let rel_path = source.rel_path;
        let language = source.language;

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // Skip binary or unreadable files
        };

        let lines = content.lines().count();
        let imports = parse::extract_imports(&content, language);
        let exports = parse::extract_exports(&content, language);

        let node = FileNode {
            path: rel_path.clone(),
            language,
            imports,
            deps: Vec::new(), // Resolved after all files scanned
            unresolved_imports: Vec::new(),
            external_imports: Vec::new(),
            exports,
            lines,
        };

        graph.add_node(node);
        indexed_files.push(source.meta);
        source_contents.insert(rel_path, content);
    }

    // Resolve imports to actual file paths in the graph
    resolve_deps(&mut graph, repo, &source_contents);

    // Build reverse index
    graph.build_rdeps();

    graph.metadata = Some(IndexMetadata {
        schema_version: INDEX_SCHEMA_VERSION,
        generated_at_unix_ms: unix_time_ms(std::time::SystemTime::now()),
        atlas_version: env!("CARGO_PKG_VERSION").to_string(),
        repo_path: repo.to_string_lossy().to_string(),
        indexed_files,
    });

    Ok(graph)
}

pub fn current_source_file_metadata(repo: &Path) -> Result<Vec<IndexedFileMeta>, AtlasError> {
    let mut files: Vec<IndexedFileMeta> = source_files(repo)?
        .into_iter()
        .map(|source| source.meta)
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

struct SourceFile {
    path: std::path::PathBuf,
    rel_path: String,
    language: Language,
    meta: IndexedFileMeta,
}

fn source_files(repo: &Path) -> Result<Vec<SourceFile>, AtlasError> {
    let mut files = Vec::new();

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
        let metadata = std::fs::metadata(path)?;

        files.push(SourceFile {
            path: path.to_path_buf(),
            rel_path: rel_path.clone(),
            language,
            meta: IndexedFileMeta {
                path: rel_path,
                modified_unix_ms: metadata.modified().map(unix_time_ms).unwrap_or_default(),
                byte_len: metadata.len(),
            },
        });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(files)
}

fn unix_time_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

/// Resolve raw import strings to actual file paths in the graph.
fn resolve_deps(graph: &mut CodeGraph, repo: &Path, source_contents: &BTreeMap<String, String>) {
    let all_paths: Vec<String> = graph.nodes.keys().cloned().collect();

    for path in &all_paths {
        let node = graph.nodes.get(path).unwrap();
        let language = node.language;
        let imports = node.imports.clone();
        let file_path = path.clone();
        let source_content = source_contents.get(path).map(String::as_str);

        let mut deps = Vec::new();
        let mut unresolved_imports = Vec::new();
        let mut external_imports = Vec::new();
        for import in &imports {
            let resolution = resolve_import(
                import,
                &file_path,
                language,
                &all_paths,
                repo,
                source_content,
            );
            for resolved in resolution.deps {
                if resolved != file_path {
                    deps.push(resolved);
                }
            }
            if resolution.unresolved_local {
                unresolved_imports.push(import.clone());
            }
            if resolution.external {
                external_imports.push(import.clone());
            }
        }

        deps.sort();
        deps.dedup();
        unresolved_imports.sort();
        unresolved_imports.dedup();
        external_imports.sort();
        external_imports.dedup();

        if let Some(node) = graph.nodes.get_mut(path) {
            node.deps = deps;
            node.unresolved_imports = unresolved_imports;
            node.external_imports = external_imports;
        }
    }
}

#[derive(Debug, Default)]
struct ImportResolution {
    deps: Vec<String>,
    unresolved_local: bool,
    external: bool,
}

/// Try to resolve a single import to a file path in the graph.
fn resolve_import(
    import: &str,
    source_file: &str,
    language: Language,
    all_paths: &[String],
    _repo: &Path,
    source_content: Option<&str>,
) -> ImportResolution {
    match language {
        Language::Rust => {
            let deps = resolve_rust_import(import, source_file, all_paths, source_content);
            let is_local = is_rust_local_import(import);
            ImportResolution {
                unresolved_local: is_local && deps.is_empty(),
                external: !is_local && deps.is_empty(),
                deps,
            }
        }
        Language::TypeScript | Language::JavaScript => {
            if !import.starts_with('.') {
                return ImportResolution {
                    deps: Vec::new(),
                    unresolved_local: false,
                    external: true,
                };
            }
            let deps: Vec<String> = resolve_js_import(import, source_file, all_paths)
                .into_iter()
                .collect();
            ImportResolution {
                unresolved_local: deps.is_empty(),
                deps,
                external: false,
            }
        }
        Language::Python => {
            let deps: Vec<String> = resolve_python_import(import, source_file, all_paths)
                .into_iter()
                .collect();
            ImportResolution {
                unresolved_local: deps.is_empty() && import.starts_with('.'),
                external: deps.is_empty() && !import.starts_with('.'),
                deps,
            }
        }
        Language::Go => {
            let deps: Vec<String> = resolve_go_import(import, all_paths).into_iter().collect();
            ImportResolution {
                unresolved_local: false,
                external: deps.is_empty(),
                deps,
            }
        }
        Language::Unknown => ImportResolution::default(),
    }
}

fn resolve_rust_import(
    import: &str,
    source_file: &str,
    all_paths: &[String],
    source_content: Option<&str>,
) -> Vec<String> {
    let src_root = rust_src_root(source_file);
    let source_module = rust_module_segments(source_file, &src_root);
    let mut resolved = Vec::new();

    for expanded in expand_rust_import(import) {
        let Some(segments) = rust_import_segments(&expanded, &source_module) else {
            continue;
        };

        if let Some(path) = resolve_rust_segments(&src_root, &segments, all_paths) {
            resolved.push(path);
            continue;
        }

        if rust_import_appears_inside_inline_test_module(&expanded, source_content) {
            let mut inline_module = source_module.clone();
            inline_module.push("tests".to_string());
            if let Some(segments) = rust_import_segments(&expanded, &inline_module) {
                if let Some(path) = resolve_rust_segments(&src_root, &segments, all_paths) {
                    resolved.push(path);
                }
            }
        }
    }

    resolved.sort();
    resolved.dedup();
    resolved
}

fn is_rust_local_import(import: &str) -> bool {
    import.starts_with("crate::") || import.starts_with("super::") || import.starts_with("self::")
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

fn rust_import_appears_inside_inline_test_module(
    import: &str,
    source_content: Option<&str>,
) -> bool {
    let Some(content) = source_content else {
        return false;
    };
    let Some(module_start) = content.find("mod tests") else {
        return false;
    };
    let test_module = &content[module_start..];
    test_module.contains(&format!("use {import}"))
        || test_module.contains(&format!("use {import}::"))
        || test_module.contains(&format!("use {import};"))
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
        _ => Some(parts.iter().map(|part| (*part).to_string()).collect()),
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

    if segments.len() == 1 {
        return [
            format!("{src_root}/lib.rs"),
            format!("{src_root}/main.rs"),
            format!("{src_root}/mod.rs"),
        ]
        .into_iter()
        .find(|candidate| all_paths.contains(candidate));
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

fn resolve_js_import(import: &str, source_file: &str, all_paths: &[String]) -> Option<String> {
    // Relative imports: "./foo" or "../bar"
    if !import.starts_with('.') {
        return None; // Skip node_modules
    }

    let base_dir = source_file
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let cleaned = normalize_relative_path(base_dir, import);
    let extensions = ["", ".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.js"];

    for ext in &extensions {
        let candidate = format!("{}{}", cleaned, ext);
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_python_import(import: &str, source_file: &str, all_paths: &[String]) -> Option<String> {
    let as_path = if import.starts_with('.') {
        python_relative_import_path(import, source_file)?
    } else {
        import.replace('.', "/")
    };

    let candidates = vec![
        format!("{}.py", as_path),
        format!("{}/__init__.py", as_path),
        format!("src/{}.py", as_path),
        format!("src/{}/__init__.py", as_path),
    ];

    candidates
        .into_iter()
        .find(|candidate| all_paths.contains(candidate))
}

fn resolve_go_import(import: &str, all_paths: &[String]) -> Option<String> {
    // Go imports are package paths, hard to resolve without go.mod
    // Best effort: match last segment
    let last_segment = import.rsplit('/').next()?;
    all_paths
        .iter()
        .find(|p| {
            p.rsplit_once('/')
                .map(|(parent, _)| {
                    parent == last_segment || parent.ends_with(&format!("/{last_segment}"))
                })
                .unwrap_or(false)
                || p.contains(&format!("/{}.go", last_segment))
        })
        .cloned()
}

fn normalize_relative_path(base_dir: &str, import: &str) -> String {
    let mut parts: Vec<&str> = base_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    for part in import.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            segment => parts.push(segment),
        }
    }

    parts.join("/")
}

fn python_relative_import_path(import: &str, source_file: &str) -> Option<String> {
    let leading_dots = import.chars().take_while(|ch| *ch == '.').count();
    if leading_dots == 0 {
        return None;
    }

    let mut parts: Vec<&str> = source_file
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("")
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    for _ in 1..leading_dots {
        parts.pop();
    }

    let tail = import.trim_start_matches('.');
    if !tail.is_empty() {
        parts.extend(tail.split('.').filter(|part| !part.is_empty()));
    }

    Some(parts.join("/"))
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
            | ".svelte-kit"
            | ".nuxt"
            | ".output"
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

    #[test]
    fn unresolved_rust_import_does_not_fall_back_to_crate_root() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        fs::write(src.join("lib.rs"), "pub mod real;\npub struct RootThing;\n").unwrap();
        fs::write(
            src.join("real.rs"),
            "use crate::missing::Thing;\nuse crate::RootThing;\nuse serde::Serialize;\npub fn real() {}\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/real.rs").unwrap();

        assert_eq!(node.deps, vec!["src/lib.rs".to_string()]);
        assert_eq!(
            node.unresolved_imports,
            vec!["crate::missing::Thing".to_string()]
        );
        assert_eq!(node.external_imports, vec!["serde::Serialize".to_string()]);
    }

    #[test]
    fn unqualified_rust_import_resolves_local_module_or_external() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        fs::write(
            src.join("main.rs"),
            "mod cli;\nuse anyhow::Result;\nuse cli::Cli;\nfn main() {}\n",
        )
        .unwrap();
        fs::write(src.join("cli.rs"), "pub struct Cli;\n").unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/main.rs").unwrap();

        assert_eq!(node.deps, vec!["src/cli.rs".to_string()]);
        assert_eq!(node.external_imports, vec!["anyhow::Result".to_string()]);
        assert!(node.unresolved_imports.is_empty());
    }

    #[test]
    fn resolves_js_imports_relative_to_source_file() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("features/panel")).unwrap();
        fs::create_dir_all(src.join("shared")).unwrap();

        fs::write(
            src.join("features/panel/view.ts"),
            "import { button } from './button';\nimport { fmt } from '../../shared/format';\n",
        )
        .unwrap();
        fs::write(
            src.join("features/panel/button.ts"),
            "export const button = 1;\n",
        )
        .unwrap();
        fs::write(src.join("shared/format.ts"), "export const fmt = 1;\n").unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/features/panel/view.ts").unwrap();

        assert_eq!(
            node.deps,
            vec![
                "src/features/panel/button.ts".to_string(),
                "src/shared/format.ts".to_string()
            ]
        );
        assert!(node.unresolved_imports.is_empty());
    }

    #[test]
    fn ignores_sveltekit_generated_output() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("app/.svelte-kit/generated")).unwrap();
        fs::create_dir_all(temp.path().join("app/src/lib")).unwrap();

        fs::write(
            temp.path().join("app/.svelte-kit/generated/root.js"),
            "import missing from './root.svelte';\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("app/src/main.ts"),
            "import { helper } from './lib/helper';\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("app/src/lib/helper.ts"),
            "export const helper = 1;\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();

        assert!(!graph
            .nodes
            .contains_key("app/.svelte-kit/generated/root.js"));
        let node = graph.nodes.get("app/src/main.ts").unwrap();
        assert_eq!(node.deps, vec!["app/src/lib/helper.ts".to_string()]);
        assert!(node.unresolved_imports.is_empty());
    }

    #[test]
    fn tracks_unresolved_local_and_external_imports() {
        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        fs::write(
            src.join("app.ts"),
            "import react from 'react';\nimport missing from './missing';\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/app.ts").unwrap();

        assert_eq!(node.external_imports, vec!["react".to_string()]);
        assert_eq!(node.unresolved_imports, vec!["./missing".to_string()]);
        let stats = graph.stats();
        assert_eq!(stats.total_external_imports, 1);
        assert_eq!(stats.total_unresolved_imports, 1);
    }

    #[test]
    fn resolves_python_relative_imports() {
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("src/pkg");
        fs::create_dir_all(&pkg).unwrap();

        fs::write(pkg.join("__init__.py"), "").unwrap();
        fs::write(pkg.join("util.py"), "def helper():\n    pass\n").unwrap();
        fs::write(pkg.join("models.py"), "class Model:\n    pass\n").unwrap();
        fs::write(
            pkg.join("feature.py"),
            "from .util import helper\nfrom . import models\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/pkg/feature.py").unwrap();

        assert_eq!(
            node.deps,
            vec![
                "src/pkg/models.py".to_string(),
                "src/pkg/util.py".to_string()
            ]
        );
        assert!(node.unresolved_imports.is_empty());
    }

    #[test]
    fn resolves_go_module_path_to_top_level_package_dir() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("cmd/app")).unwrap();
        fs::create_dir_all(temp.path().join("bar")).unwrap();

        fs::write(
            temp.path().join("cmd/app/main.go"),
            "package main\nimport \"github.com/example/project/bar\"\nfunc main() {}\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("bar/foo.go"),
            "package bar\nfunc Run() {}\n",
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("cmd/app/main.go").unwrap();

        assert_eq!(node.deps, vec!["bar/foo.go".to_string()]);
        assert!(node.external_imports.is_empty());
    }

    #[test]
    fn resolves_rust_imports_from_inline_test_modules() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("src/plugin");
        fs::create_dir_all(&plugin).unwrap();

        fs::write(temp.path().join("src/lib.rs"), "pub mod plugin;\n").unwrap();
        fs::write(
            plugin.join("mod.rs"),
            "pub mod transport;\npub mod types;\n",
        )
        .unwrap();
        fs::write(
            plugin.join("types.rs"),
            "pub const MAX_IPC_PAYLOAD_BYTES: usize = 100;\n",
        )
        .unwrap();
        fs::write(
            plugin.join("transport.rs"),
            r#"
pub struct PluginProcess;

#[cfg(test)]
mod tests {
    use super::super::types::MAX_IPC_PAYLOAD_BYTES;
    use super::*;

    #[test]
    fn smoke() {
        assert_eq!(MAX_IPC_PAYLOAD_BYTES, 100);
    }
}
"#,
        )
        .unwrap();

        let graph = scan_project(temp.path()).unwrap();
        let node = graph.nodes.get("src/plugin/transport.rs").unwrap();

        assert!(node.deps.contains(&"src/plugin/types.rs".to_string()));
        assert!(node.unresolved_imports.is_empty());
    }
}
