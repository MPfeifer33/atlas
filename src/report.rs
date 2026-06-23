use crate::graph::{CodeGraph, GraphStats};
use crate::AtlasError;

pub fn print_modules(graph: &CodeGraph, is_json: bool) -> Result<(), AtlasError> {
    if is_json {
        let mut paths: Vec<&String> = graph.nodes.keys().collect();
        paths.sort();
        let modules: Vec<serde_json::Value> = paths.iter()
            .map(|path| &graph.nodes[*path])
            .map(|n| serde_json::json!({
                "path": n.path,
                "language": n.language,
                "lines": n.lines,
                "deps_count": n.deps.len(),
                "exports_count": n.exports.len(),
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "modules": modules,
        }))?);
    } else {
        let mut paths: Vec<&String> = graph.nodes.keys().collect();
        paths.sort();
        println!("atlas: {} files indexed", paths.len());
        println!();
        for path in &paths {
            let node = &graph.nodes[*path];
            let dep_count = node.deps.len();
            let export_count = node.exports.len();
            println!("  {} ({}, {}L, {} deps, {} exports)",
                path, node.language.label(), node.lines, dep_count, export_count);
        }
    }
    Ok(())
}

pub fn print_deps(graph: &CodeGraph, file: &str, is_json: bool) -> Result<(), AtlasError> {
    let node = graph.nodes.get(file)
        .ok_or_else(|| AtlasError::NotFound(format!("File not in graph: {file}")))?;

    if is_json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "file": file,
            "deps": node.deps,
            "imports": node.imports,
        }))?);
    } else {
        println!("atlas deps: {file}");
        println!();
        if node.deps.is_empty() {
            println!("  No dependencies in the graph.");
        } else {
            for dep in &node.deps {
                println!("  -> {dep}");
            }
        }
        if !node.imports.is_empty() {
            println!();
            println!("  Raw imports:");
            for imp in &node.imports {
                println!("    {imp}");
            }
        }
    }
    Ok(())
}

pub fn print_rdeps(graph: &CodeGraph, file: &str, is_json: bool) -> Result<(), AtlasError> {
    if !graph.nodes.contains_key(file) {
        return Err(AtlasError::NotFound(format!("File not in graph: {file}")));
    }

    let rdeps = graph.rdeps_of(file);

    if is_json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "file": file,
            "rdeps": rdeps,
        }))?);
    } else {
        println!("atlas rdeps: {file}");
        println!();
        if rdeps.is_empty() {
            println!("  Nothing depends on this file.");
        } else {
            println!("  {} file(s) depend on this:", rdeps.len());
            for rdep in &rdeps {
                println!("  <- {rdep}");
            }
        }
    }
    Ok(())
}

pub fn print_blast(graph: &CodeGraph, file: &str, depth: usize, is_json: bool) -> Result<(), AtlasError> {
    if !graph.nodes.contains_key(file) {
        return Err(AtlasError::NotFound(format!("File not in graph: {file}")));
    }

    let blast = graph.blast_radius(file, depth);

    if is_json {
        let entries: Vec<serde_json::Value> = blast.iter()
            .map(|(path, d)| serde_json::json!({ "path": path, "depth": d }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "file": file,
            "max_depth": depth,
            "blast_radius": entries,
            "total_affected": blast.len(),
        }))?);
    } else {
        println!("atlas blast: {file} (max depth {depth})");
        println!();
        if blast.is_empty() {
            println!("  No transitive dependents found.");
        } else {
            println!("  {} file(s) in blast radius:", blast.len());
            for (path, d) in &blast {
                let indent = "  ".repeat(*d);
                println!("  {indent}<- {path} (depth {d})");
            }
        }
    }
    Ok(())
}

pub fn print_stats(stats: &GraphStats, is_json: bool) -> Result<(), AtlasError> {
    if is_json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "stats": {
                "total_files": stats.total_files,
                "total_lines": stats.total_lines,
                "total_deps": stats.total_deps,
                "by_language": stats.by_language,
            }
        }))?);
    } else {
        println!("atlas stats:");
        println!();
        println!("  Files: {}", stats.total_files);
        println!("  Lines: {}", stats.total_lines);
        println!("  Dependencies: {}", stats.total_deps);
        println!();
        println!("  By language:");
        let mut langs: Vec<_> = stats.by_language.iter().collect();
        langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in langs {
            println!("    {lang}: {count}");
        }
    }
    Ok(())
}
