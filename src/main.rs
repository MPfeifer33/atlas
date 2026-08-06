mod cli;
mod graph;
mod parse;
mod report;
mod scan;
mod store;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    let result = run(&cli);
    match result {
        Ok(()) => {}
        Err(e) => {
            let code = e.exit_code();
            if cli.is_json() {
                let err_json = serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": e.error_code(),
                        "message": e.to_string(),
                    }
                });
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&err_json).unwrap_or_else(|_| format!(
                        "{{\"ok\":false,\"error\":{{\"message\":\"{e}\"}}}}"
                    ))
                );
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(code);
        }
    }
}

fn run(cli: &Cli) -> Result<(), AtlasError> {
    let repo = cli.resolve_repo()?;

    match &cli.command {
        Command::Scan { force } => {
            if !force && store::has_index(&repo) {
                eprintln!("Index already exists. Use --force to rescan.");
                return Ok(());
            }
            let graph = scan::scan_project(&repo)?;
            let stats = graph.stats();
            store::save(&repo, &graph)?;

            if cli.is_json() {
                let index = graph.metadata.as_ref().map(|metadata| {
                    serde_json::json!({
                        "schema_version": metadata.schema_version,
                        "generated_at_unix_ms": metadata.generated_at_unix_ms,
                        "atlas_version": metadata.atlas_version,
                        "repo_path": metadata.repo_path,
                        "indexed_files": metadata.indexed_files.len(),
                    })
                });
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
                        },
                        "index": index,
                        "index_path": ".agent-atlas/graph.json",
                    }))?
                );
            } else {
                println!(
                    "Scanned {} files ({} lines, {} deps)",
                    stats.total_files, stats.total_lines, stats.total_deps
                );
                println!("Index saved to .agent-atlas/graph.json");
            }
            Ok(())
        }
        Command::Modules => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_modules(&graph, cli.is_json())
        }
        Command::Deps { file } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_deps(&graph, file, cli.is_json())
        }
        Command::Rdeps { file } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_rdeps(&graph, file, cli.is_json())
        }
        Command::Blast { file, depth } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_blast(&graph, file, *depth, cli.is_json())
        }
        Command::Doctor { limit, strict } => {
            let graph = load_or_scan(&repo, cli)?;
            let freshness = store::freshness(&repo, &graph)?;
            let doctor = report::print_doctor(&graph, &freshness, *limit, cli.is_json())?;
            if *strict {
                std::process::exit(doctor.action_level.strict_exit_code());
            }
            Ok(())
        }
        Command::Hotspots { limit } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_hotspots(&graph, *limit, cli.is_json())
        }
        Command::Symbols { query, limit } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_symbols(&graph, query.as_deref(), *limit, cli.is_json())
        }
        Command::Impact { file, depth } => {
            let graph = load_or_scan(&repo, cli)?;
            report::print_impact(&graph, file, *depth, cli.is_json())
        }
        Command::Stats => {
            let graph = load_or_scan(&repo, cli)?;
            let stats = graph.stats();
            report::print_stats(&stats, cli.is_json())
        }
    }
}

/// Load existing index or scan if none exists.
fn load_or_scan(repo: &std::path::Path, cli: &Cli) -> Result<graph::CodeGraph, AtlasError> {
    if let Some(graph) = store::load(repo)? {
        let freshness = store::freshness(repo, &graph)?;
        if !cli.is_json() && freshness.stale {
            eprintln!("warning: {}", freshness.summary());
            eprintln!("warning: run atlas scan --force to refresh the graph");
        }
        Ok(graph)
    } else {
        if !cli.is_json() {
            eprintln!("No index found, scanning...");
        }
        let graph = scan::scan_project(repo)?;
        store::save(repo, &graph)?;
        Ok(graph)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("{0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl AtlasError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AtlasError::Validation(_) => 1,
            AtlasError::NotFound(_) => 3,
            AtlasError::Io(_) => 2,
            AtlasError::Json(_) => 1,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AtlasError::Validation(_) => "validation_error",
            AtlasError::NotFound(_) => "not_found",
            AtlasError::Io(_) => "io_error",
            AtlasError::Json(_) => "json_error",
        }
    }
}
