use crate::graph::{CodeGraph, GraphStats};
use crate::AtlasError;

pub fn print_modules(graph: &CodeGraph, is_json: bool) -> Result<(), AtlasError> {
    if is_json {
        let mut paths: Vec<&String> = graph.nodes.keys().collect();
        paths.sort();
        let modules: Vec<serde_json::Value> = paths
            .iter()
            .map(|path| &graph.nodes[*path])
            .map(|n| {
                serde_json::json!({
                    "path": n.path,
                    "language": n.language,
                    "lines": n.lines,
                    "deps_count": n.deps.len(),
                    "exports_count": n.exports.len(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "modules": modules,
            }))?
        );
    } else {
        let mut paths: Vec<&String> = graph.nodes.keys().collect();
        paths.sort();
        println!("atlas: {} files indexed", paths.len());
        println!();
        for path in &paths {
            let node = &graph.nodes[*path];
            let dep_count = node.deps.len();
            let export_count = node.exports.len();
            println!(
                "  {} ({}, {}L, {} deps, {} exports)",
                path,
                node.language.label(),
                node.lines,
                dep_count,
                export_count
            );
        }
    }
    Ok(())
}

pub fn print_deps(graph: &CodeGraph, file: &str, is_json: bool) -> Result<(), AtlasError> {
    let node = graph
        .nodes
        .get(file)
        .ok_or_else(|| AtlasError::NotFound(format!("File not in graph: {file}")))?;

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "file": file,
                "deps": node.deps,
                "imports": node.imports,
            }))?
        );
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
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "file": file,
                "rdeps": rdeps,
            }))?
        );
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

pub fn print_blast(
    graph: &CodeGraph,
    file: &str,
    depth: usize,
    is_json: bool,
) -> Result<(), AtlasError> {
    if !graph.nodes.contains_key(file) {
        return Err(AtlasError::NotFound(format!("File not in graph: {file}")));
    }

    let blast = graph.blast_radius(file, depth);

    if is_json {
        let entries: Vec<serde_json::Value> = blast
            .iter()
            .map(|e| {
                serde_json::json!({
                    "path": e.path,
                    "depth": e.depth,
                    "via": e.via,
                    "fan_out": e.fan_out,
                })
            })
            .collect();

        // High-risk files: direct dependents (depth 1) with high fan_out
        let high_risk: Vec<&str> = blast
            .iter()
            .filter(|e| e.depth == 1 && e.fan_out > 0)
            .map(|e| e.path.as_str())
            .collect();

        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "file": file,
                "max_depth": depth,
                "total_affected": blast.len(),
                "blast_radius": entries,
                "high_risk": high_risk,
                "hints": blast_hints(file, &blast),
            }))?
        );
    } else {
        println!("atlas blast: {file} (max depth {depth})");
        println!();
        if blast.is_empty() {
            println!("  No transitive dependents found.");
            println!();
            println!("  This file is a leaf — changes here won't ripple.");
        } else {
            println!("  {} file(s) in blast radius:", blast.len());
            println!();
            for entry in &blast {
                let indent = "  ".repeat(entry.depth);
                let risk = if entry.fan_out > 3 {
                    format!(" [high risk: {} downstream]", entry.fan_out)
                } else if entry.fan_out > 0 {
                    format!(" [{} downstream]", entry.fan_out)
                } else {
                    String::new()
                };
                println!(
                    "  {indent}<- {} (via {}){}",
                    entry.path,
                    short_path(&entry.via),
                    risk
                );
            }

            // Summary section
            let direct = blast.iter().filter(|e| e.depth == 1).count();
            let indirect = blast.len() - direct;
            println!();
            println!("  Summary: {} direct, {} indirect", direct, indirect);

            let high_risk: Vec<&str> = blast
                .iter()
                .filter(|e| e.depth == 1 && e.fan_out > 3)
                .map(|e| e.path.as_str())
                .collect();
            if !high_risk.is_empty() {
                println!();
                println!("  High-risk files (direct dependents with wide downstream):");
                for path in &high_risk {
                    println!("    ! {}", path);
                }
            }

            // Next-step hints
            let hints = blast_hints(file, &blast);
            if !hints.is_empty() {
                println!();
                println!("  Suggested next steps:");
                for hint in &hints {
                    println!("    - {hint}");
                }
            }
        }
    }
    Ok(())
}

/// Generate actionable next-step hints based on blast analysis.
fn blast_hints(file: &str, blast: &[crate::graph::BlastEntry]) -> Vec<String> {
    let mut hints = Vec::new();

    let direct_count = blast.iter().filter(|e| e.depth == 1).count();
    let total = blast.len();

    if total == 0 {
        return hints;
    }

    // If there are high-fan-out direct dependents, flag them for review
    let high_risk: Vec<&str> = blast
        .iter()
        .filter(|e| e.depth == 1 && e.fan_out > 3)
        .map(|e| e.path.as_str())
        .collect();

    if !high_risk.is_empty() {
        hints.push(format!(
            "Review {} high-risk direct dependent(s) first — changes there cascade further",
            high_risk.len()
        ));
    }

    // Test suggestion
    if direct_count <= 5 {
        hints.push(format!(
            "Run tests covering {} and its {} direct dependent(s)",
            short_path(file),
            direct_count
        ));
    } else {
        hints.push(format!(
            "Run full test suite — {} direct dependents is broad impact",
            direct_count
        ));
    }

    // If blast is deep, warn about cascade
    let max_depth = blast.iter().map(|e| e.depth).max().unwrap_or(0);
    if max_depth >= 3 {
        hints.push(format!(
            "Cascade reaches depth {} — consider whether the API boundary at depth 1 can absorb this change",
            max_depth
        ));
    }

    hints
}

/// Shorten a path for display (last 2 segments).
fn short_path(path: &str) -> &str {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        path
    } else {
        let start = path.len() - parts[parts.len() - 2..].join("/").len();
        &path[start..]
    }
}

pub fn print_stats(stats: &GraphStats, is_json: bool) -> Result<(), AtlasError> {
    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "stats": {
                    "total_files": stats.total_files,
                    "total_lines": stats.total_lines,
                    "total_deps": stats.total_deps,
                    "by_language": stats.by_language,
                }
            }))?
        );
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
