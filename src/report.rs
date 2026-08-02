use crate::graph::{CodeGraph, GraphStats};
use crate::store::IndexFreshness;
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
                    "unresolved_imports_count": n.unresolved_imports.len(),
                    "external_imports_count": n.external_imports.len(),
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
                "  {} ({}, {}L, {} deps, {} unresolved, {} external, {} exports)",
                path,
                node.language.label(),
                node.lines,
                dep_count,
                node.unresolved_imports.len(),
                node.external_imports.len(),
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
                "unresolved_imports": node.unresolved_imports,
                "external_imports": node.external_imports,
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
        if !node.unresolved_imports.is_empty() {
            println!();
            println!("  Unresolved local imports:");
            for imp in &node.unresolved_imports {
                println!("    ? {imp}");
            }
        }
        if !node.external_imports.is_empty() {
            println!();
            println!("  External imports:");
            for imp in &node.external_imports {
                println!("    * {imp}");
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
            .filter(|e| e.depth == 1 && e.fan_out > 3)
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

pub fn print_doctor(
    graph: &CodeGraph,
    freshness: &IndexFreshness,
    limit: usize,
    is_json: bool,
) -> Result<(), AtlasError> {
    let stats = graph.stats();
    let unresolved = unresolved_entries(graph, limit);
    let external = external_entries(graph, limit);
    let isolated = isolated_files(graph, limit);
    let hotspots = hotspot_entries(graph, limit);
    let status = doctor_status(&stats, freshness);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "health": {
                    "status": status,
                    "total_files": stats.total_files,
                    "total_deps": stats.total_deps,
                    "total_unresolved_imports": stats.total_unresolved_imports,
                    "total_external_imports": stats.total_external_imports,
                },
                "index": freshness,
                "unresolved_imports": unresolved,
                "external_imports": external,
                "isolated_files": isolated,
                "hotspots": hotspots,
            }))?
        );
    } else {
        println!("atlas doctor:");
        println!();
        println!("  Health: {status}");
        println!("  Files: {}", stats.total_files);
        println!("  Dependencies: {}", stats.total_deps);
        println!(
            "  Unresolved local imports: {}",
            stats.total_unresolved_imports
        );
        println!("  External imports: {}", stats.total_external_imports);
        println!("  Index: {}", freshness.summary());

        if freshness.stale {
            println!();
            println!("  Index freshness warnings:");
            for file in freshness.changed_files.iter().take(limit) {
                println!("    changed: {file}");
            }
            for file in freshness.missing_files.iter().take(limit) {
                println!("    missing: {file}");
            }
            for file in freshness.new_files.iter().take(limit) {
                println!("    new: {file}");
            }
        }

        if unresolved.is_empty() {
            println!();
            println!("  No unresolved local imports found.");
        } else {
            println!();
            println!("  Unresolved local imports:");
            for entry in &unresolved {
                println!("    ? {} imports {}", entry.file, entry.import);
            }
        }

        if !hotspots.is_empty() {
            println!();
            println!("  Hotspots:");
            for entry in &hotspots {
                println!(
                    "    ! {} ({} rdeps, {} blast, {} deps)",
                    entry.path, entry.rdeps_count, entry.blast_radius_count, entry.deps_count
                );
            }
        }

        if !isolated.is_empty() {
            println!();
            println!("  Isolated files:");
            for file in &isolated {
                println!("    - {file}");
            }
        }
    }

    Ok(())
}

pub fn print_hotspots(graph: &CodeGraph, limit: usize, is_json: bool) -> Result<(), AtlasError> {
    let hotspots = hotspot_entries(graph, limit);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "hotspots": hotspots,
            }))?
        );
    } else {
        println!("atlas hotspots:");
        println!();
        if hotspots.is_empty() {
            println!("  No dependency hotspots found.");
        } else {
            for entry in &hotspots {
                println!(
                    "  {} ({}, {}L): {} rdeps, {} blast, {} deps, {} unresolved",
                    entry.path,
                    entry.language,
                    entry.lines,
                    entry.rdeps_count,
                    entry.blast_radius_count,
                    entry.deps_count,
                    entry.unresolved_imports_count
                );
            }
        }
    }

    Ok(())
}

pub fn print_symbols(
    graph: &CodeGraph,
    query: Option<&str>,
    limit: usize,
    is_json: bool,
) -> Result<(), AtlasError> {
    let symbols = symbol_entries(graph, query, limit);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "query": query,
                "symbols": symbols,
            }))?
        );
    } else {
        println!("atlas symbols:");
        if let Some(query) = query {
            println!("  query: {query}");
        }
        println!();
        if symbols.is_empty() {
            println!("  No exported symbols found.");
        } else {
            for entry in &symbols {
                println!("  {}  {} ({})", entry.symbol, entry.path, entry.language);
            }
        }
    }

    Ok(())
}

pub fn print_impact(
    graph: &CodeGraph,
    file: &str,
    depth: usize,
    is_json: bool,
) -> Result<(), AtlasError> {
    let node = graph
        .nodes
        .get(file)
        .ok_or_else(|| AtlasError::NotFound(format!("File not in graph: {file}")))?;
    let rdeps: Vec<String> = graph
        .rdeps_of(file)
        .into_iter()
        .map(str::to_string)
        .collect();
    let blast = graph.blast_radius(file, depth);
    let high_risk: Vec<&str> = blast
        .iter()
        .filter(|e| e.depth == 1 && e.fan_out > 3)
        .map(|e| e.path.as_str())
        .collect();
    let hints = blast_hints(file, &blast);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "file": file,
                "language": node.language,
                "lines": node.lines,
                "exports": node.exports,
                "deps": node.deps,
                "rdeps": rdeps,
                "unresolved_imports": node.unresolved_imports,
                "external_imports": node.external_imports,
                "max_depth": depth,
                "total_affected": blast.len(),
                "blast_radius": blast,
                "high_risk": high_risk,
                "hints": hints,
            }))?
        );
    } else {
        println!("atlas impact: {file} (max depth {depth})");
        println!();
        println!("  Language: {}", node.language.label());
        println!("  Lines: {}", node.lines);
        println!("  Exports: {}", node.exports.len());
        println!("  Dependencies: {}", node.deps.len());
        println!("  Direct dependents: {}", rdeps.len());
        println!("  Blast radius: {}", blast.len());

        if !node.unresolved_imports.is_empty() {
            println!();
            println!("  Map warning: unresolved local imports");
            for import in &node.unresolved_imports {
                println!("    ? {import}");
            }
        }

        if !node.deps.is_empty() {
            println!();
            println!("  Direct dependencies:");
            for dep in &node.deps {
                println!("    -> {dep}");
            }
        }

        if !rdeps.is_empty() {
            println!();
            println!("  Direct dependents:");
            for rdep in &rdeps {
                println!("    <- {rdep}");
            }
        }

        if !blast.is_empty() {
            println!();
            println!("  Blast radius:");
            for entry in &blast {
                let indent = "  ".repeat(entry.depth);
                println!(
                    "  {indent}<- {} (via {}, fan-out {})",
                    entry.path,
                    short_path(&entry.via),
                    entry.fan_out
                );
            }
        }

        if !hints.is_empty() {
            println!();
            println!("  Suggested next steps:");
            for hint in &hints {
                println!("    - {hint}");
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
                    "total_unresolved_imports": stats.total_unresolved_imports,
                    "total_external_imports": stats.total_external_imports,
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
        println!(
            "  Unresolved local imports: {}",
            stats.total_unresolved_imports
        );
        println!("  External imports: {}", stats.total_external_imports);
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

#[derive(Debug, serde::Serialize)]
struct ImportProblem {
    file: String,
    import: String,
}

#[derive(Debug, serde::Serialize)]
struct HotspotEntry {
    path: String,
    language: String,
    lines: usize,
    deps_count: usize,
    rdeps_count: usize,
    blast_radius_count: usize,
    unresolved_imports_count: usize,
}

#[derive(Debug, serde::Serialize)]
struct SymbolEntry {
    symbol: String,
    path: String,
    language: String,
}

fn unresolved_entries(graph: &CodeGraph, limit: usize) -> Vec<ImportProblem> {
    let mut entries = Vec::new();
    let mut paths: Vec<&String> = graph.nodes.keys().collect();
    paths.sort();
    for path in paths {
        let node = &graph.nodes[path];
        for import in &node.unresolved_imports {
            entries.push(ImportProblem {
                file: path.clone(),
                import: import.clone(),
            });
        }
    }
    entries.truncate(limit);
    entries
}

fn external_entries(graph: &CodeGraph, limit: usize) -> Vec<ImportProblem> {
    let mut entries = Vec::new();
    let mut paths: Vec<&String> = graph.nodes.keys().collect();
    paths.sort();
    for path in paths {
        let node = &graph.nodes[path];
        for import in &node.external_imports {
            entries.push(ImportProblem {
                file: path.clone(),
                import: import.clone(),
            });
        }
    }
    entries.truncate(limit);
    entries
}

fn isolated_files(graph: &CodeGraph, limit: usize) -> Vec<String> {
    let mut files: Vec<String> = graph
        .nodes
        .iter()
        .filter(|(path, node)| node.deps.is_empty() && graph.rdeps_of(path).is_empty())
        .map(|(path, _)| path.clone())
        .collect();
    files.sort();
    files.truncate(limit);
    files
}

fn hotspot_entries(graph: &CodeGraph, limit: usize) -> Vec<HotspotEntry> {
    let mut entries: Vec<HotspotEntry> = graph
        .nodes
        .iter()
        .map(|(path, node)| {
            let rdeps_count = graph.rdeps_of(path).len();
            HotspotEntry {
                path: path.clone(),
                language: node.language.label().to_string(),
                lines: node.lines,
                deps_count: node.deps.len(),
                rdeps_count,
                blast_radius_count: graph.blast_radius(path, 5).len(),
                unresolved_imports_count: node.unresolved_imports.len(),
            }
        })
        .filter(|entry| entry.rdeps_count > 0 || entry.blast_radius_count > 0)
        .collect();

    entries.sort_by(|a, b| {
        b.rdeps_count
            .cmp(&a.rdeps_count)
            .then_with(|| b.blast_radius_count.cmp(&a.blast_radius_count))
            .then_with(|| b.deps_count.cmp(&a.deps_count))
            .then_with(|| a.path.cmp(&b.path))
    });
    entries.truncate(limit);
    entries
}

fn symbol_entries(graph: &CodeGraph, query: Option<&str>, limit: usize) -> Vec<SymbolEntry> {
    let query = query.map(|value| value.to_lowercase());
    let mut entries = Vec::new();
    let mut paths: Vec<&String> = graph.nodes.keys().collect();
    paths.sort();

    for path in paths {
        let node = &graph.nodes[path];
        for symbol in &node.exports {
            let matches = query.as_ref().is_none_or(|needle| {
                symbol.to_lowercase().contains(needle) || path.to_lowercase().contains(needle)
            });
            if matches {
                entries.push(SymbolEntry {
                    symbol: symbol.clone(),
                    path: path.clone(),
                    language: node.language.label().to_string(),
                });
            }
        }
    }

    entries.truncate(limit);
    entries
}

fn doctor_status(stats: &GraphStats, freshness: &IndexFreshness) -> &'static str {
    if stats.total_unresolved_imports == 0 && !freshness.stale {
        "healthy"
    } else {
        "warning"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn doctor_status_warns_when_index_is_stale() {
        let stats = GraphStats {
            total_files: 1,
            total_lines: 1,
            total_deps: 0,
            total_unresolved_imports: 0,
            total_external_imports: 0,
            by_language: HashMap::new(),
        };
        let freshness = IndexFreshness {
            has_metadata: true,
            schema_version: Some(crate::graph::INDEX_SCHEMA_VERSION),
            expected_schema_version: crate::graph::INDEX_SCHEMA_VERSION,
            generated_at_unix_ms: Some(1),
            stale: true,
            schema_mismatch: false,
            changed_files: vec!["src/lib.rs".to_string()],
            missing_files: Vec::new(),
            new_files: Vec::new(),
        };

        assert_eq!(doctor_status(&stats, &freshness), "warning");
    }
}
