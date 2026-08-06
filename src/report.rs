use crate::graph::{CodeGraph, GraphStats};
use crate::store::IndexFreshness;
use crate::AtlasError;

const DOCTOR_SCHEMA_VERSION: &str = "atlas.doctor.v1";
const GRAPH_CONTRACT_VERSION: &str = "atlas.graph.v2";

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentDoctor {
    pub schema_version: String,
    pub graph_contract_version: String,
    pub status: DoctorStatus,
    pub action_level: ActionLevel,
    pub gates: AgentGates,
    pub health: DoctorHealth,
    pub index: IndexFreshness,
    pub unresolved_imports: Vec<ImportProblem>,
    pub external_imports: Vec<ImportProblem>,
    pub isolated_files: Vec<String>,
    pub hotspots: Vec<HotspotEntry>,
    pub advice: String,
    pub recommendations: Vec<String>,
    pub recommended_commands: Vec<RecommendedCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ready,
    Caution,
    Blocked,
}

impl DoctorStatus {
    fn label(self) -> &'static str {
        match self {
            DoctorStatus::Ready => "ready",
            DoctorStatus::Caution => "caution",
            DoctorStatus::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionLevel {
    None,
    Refresh,
    Review,
    Stop,
}

impl ActionLevel {
    fn label(self) -> &'static str {
        match self {
            ActionLevel::None => "none",
            ActionLevel::Refresh => "refresh",
            ActionLevel::Review => "review",
            ActionLevel::Stop => "stop",
        }
    }

    pub fn strict_exit_code(self) -> i32 {
        match self {
            ActionLevel::None => 0,
            ActionLevel::Refresh => 10,
            ActionLevel::Review | ActionLevel::Stop => 30,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentGates {
    pub files_indexed: bool,
    pub index_has_metadata: bool,
    pub schema_current: bool,
    pub source_delta_clear: bool,
    pub index_fresh: bool,
    pub unresolved_imports_clear: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorHealth {
    pub status: String,
    pub total_files: usize,
    pub total_deps: usize,
    pub total_unresolved_imports: usize,
    pub total_external_imports: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecommendedCommand {
    pub kind: RecommendationKind,
    pub command: Option<String>,
    pub argv: Option<Vec<String>>,
    pub label: String,
    pub reason: String,
    pub reason_code: String,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    Command,
    Manual,
}

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
) -> Result<AgentDoctor, AtlasError> {
    let doctor = build_doctor(graph, freshness, limit);

    if is_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "schema_version": &doctor.schema_version,
                "graph_contract_version": &doctor.graph_contract_version,
                "status": doctor.status,
                "action_level": doctor.action_level,
                "doctor": &doctor,
                // Compatibility aliases for existing consumers.
                "health": &doctor.health,
                "index": &doctor.index,
                "unresolved_imports": &doctor.unresolved_imports,
                "external_imports": &doctor.external_imports,
                "isolated_files": &doctor.isolated_files,
                "hotspots": &doctor.hotspots,
                "advice": &doctor.advice,
                "recommendations": &doctor.recommendations,
                "recommended_commands": &doctor.recommended_commands,
            }))?
        );
    } else {
        println!("atlas doctor:");
        println!();
        println!(
            "  Status: {} ({})",
            doctor.status.label(),
            doctor.action_level.label()
        );
        println!("  Health: {}", doctor.health.status);
        println!("  Files: {}", doctor.health.total_files);
        println!("  Dependencies: {}", doctor.health.total_deps);
        println!(
            "  Unresolved local imports: {}",
            doctor.health.total_unresolved_imports
        );
        println!(
            "  External imports: {}",
            doctor.health.total_external_imports
        );
        println!("  Index: {}", doctor.index.summary());
        println!();
        println!("  Gates:");
        println!("    files indexed: {}", doctor.gates.files_indexed);
        println!(
            "    index has metadata: {}",
            doctor.gates.index_has_metadata
        );
        println!("    schema current: {}", doctor.gates.schema_current);
        println!(
            "    source delta clear: {}",
            doctor.gates.source_delta_clear
        );
        println!("    index fresh: {}", doctor.gates.index_fresh);
        println!(
            "    unresolved imports clear: {}",
            doctor.gates.unresolved_imports_clear
        );

        if doctor.index.stale {
            println!();
            println!("  Index freshness warnings:");
            for file in doctor.index.changed_files.iter().take(limit) {
                println!("    changed: {file}");
            }
            for file in doctor.index.missing_files.iter().take(limit) {
                println!("    missing: {file}");
            }
            for file in doctor.index.new_files.iter().take(limit) {
                println!("    new: {file}");
            }
        }

        if doctor.unresolved_imports.is_empty() {
            println!();
            println!("  No unresolved local imports found.");
        } else {
            println!();
            println!("  Unresolved local imports:");
            for entry in &doctor.unresolved_imports {
                println!("    ? {} imports {}", entry.file, entry.import);
            }
        }

        if !doctor.hotspots.is_empty() {
            println!();
            println!("  Hotspots:");
            for entry in &doctor.hotspots {
                println!(
                    "    ! {} ({} rdeps, {} blast, {} deps)",
                    entry.path, entry.rdeps_count, entry.blast_radius_count, entry.deps_count
                );
            }
        }

        if !doctor.isolated_files.is_empty() {
            println!();
            println!("  Isolated files:");
            for file in &doctor.isolated_files {
                println!("    - {file}");
            }
        }

        println!();
        println!("  Advice: {}", doctor.advice);
        println!();
        println!("  Recommended next steps:");
        for recommendation in &doctor.recommended_commands {
            println!("    - {}", recommendation.label);
        }
    }

    Ok(doctor)
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

fn build_doctor(graph: &CodeGraph, freshness: &IndexFreshness, limit: usize) -> AgentDoctor {
    let stats = graph.stats();
    let unresolved = unresolved_entries(graph, limit);
    let external = external_entries(graph, limit);
    let isolated = isolated_files(graph, limit);
    let hotspots = hotspot_entries(graph, limit);
    let health_status = doctor_status(&stats, freshness).to_string();
    let health = DoctorHealth {
        status: health_status,
        total_files: stats.total_files,
        total_deps: stats.total_deps,
        total_unresolved_imports: stats.total_unresolved_imports,
        total_external_imports: stats.total_external_imports,
    };
    let gates = gates_for(&stats, freshness);
    let action_level = action_level_for(&gates);
    let status = status_for(action_level);
    let advice = advice_for(action_level, &gates);
    let recommended_commands = recommended_commands_for(action_level, &gates, &unresolved);
    let recommendations = recommended_commands
        .iter()
        .map(recommendation_label)
        .collect();

    AgentDoctor {
        schema_version: DOCTOR_SCHEMA_VERSION.to_string(),
        graph_contract_version: GRAPH_CONTRACT_VERSION.to_string(),
        status,
        action_level,
        gates,
        health,
        index: freshness.clone(),
        unresolved_imports: unresolved,
        external_imports: external,
        isolated_files: isolated,
        hotspots,
        advice,
        recommendations,
        recommended_commands,
    }
}

fn gates_for(stats: &GraphStats, freshness: &IndexFreshness) -> AgentGates {
    AgentGates {
        files_indexed: stats.total_files > 0,
        index_has_metadata: freshness.has_metadata,
        schema_current: !freshness.schema_mismatch,
        source_delta_clear: freshness.changed_files.is_empty()
            && freshness.missing_files.is_empty()
            && freshness.new_files.is_empty(),
        index_fresh: !freshness.stale,
        unresolved_imports_clear: stats.total_unresolved_imports == 0,
    }
}

fn action_level_for(gates: &AgentGates) -> ActionLevel {
    if !gates.files_indexed {
        ActionLevel::Stop
    } else if !gates.index_fresh || !gates.schema_current || !gates.source_delta_clear {
        ActionLevel::Refresh
    } else if !gates.unresolved_imports_clear {
        ActionLevel::Review
    } else {
        ActionLevel::None
    }
}

fn status_for(action_level: ActionLevel) -> DoctorStatus {
    match action_level {
        ActionLevel::None => DoctorStatus::Ready,
        ActionLevel::Refresh => DoctorStatus::Caution,
        ActionLevel::Review | ActionLevel::Stop => DoctorStatus::Blocked,
    }
}

fn advice_for(action_level: ActionLevel, gates: &AgentGates) -> String {
    match action_level {
        ActionLevel::None => {
            "graph is current and no unresolved local imports were found; Atlas results are ready for planning".to_string()
        }
        ActionLevel::Refresh => {
            let mut reasons = Vec::new();
            if !gates.index_has_metadata {
                reasons.push("missing metadata");
            }
            if !gates.schema_current {
                reasons.push("schema mismatch");
            }
            if !gates.source_delta_clear {
                reasons.push("source files changed since scan");
            }
            if reasons.is_empty() {
                reasons.push("stale index");
            }
            format!(
                "refresh the graph before making dependency or blast-radius claims ({})",
                reasons.join(", ")
            )
        }
        ActionLevel::Review => {
            "unresolved local imports may hide dependencies; inspect map gaps before relying on blast-radius output".to_string()
        }
        ActionLevel::Stop => {
            "no source files are indexed; scan the repo or verify Atlas supports the project layout before using graph results".to_string()
        }
    }
}

fn recommended_commands_for(
    action_level: ActionLevel,
    gates: &AgentGates,
    unresolved: &[ImportProblem],
) -> Vec<RecommendedCommand> {
    let mut commands = Vec::new();

    if !gates.files_indexed {
        commands.push(command_recommendation(
            "atlas scan --force",
            &["atlas", "scan", "--force"],
            "no_files_indexed",
            "doctor found zero indexed source files",
            true,
        ));
        commands.push(manual_recommendation(
            "Verify Atlas supports this project layout if scan still indexes zero files",
            "unsupported_project_layout",
            "zero indexed files may mean no supported source extensions were found",
            true,
        ));
        return commands;
    }

    if !gates.index_fresh || !gates.schema_current || !gates.source_delta_clear {
        commands.push(command_recommendation(
            "atlas scan --force",
            &["atlas", "scan", "--force"],
            "index_not_fresh",
            "index metadata, schema, or source fingerprints are not current",
            true,
        ));
    }

    if !gates.unresolved_imports_clear {
        let first_file = unresolved
            .first()
            .map(|entry| entry.file.as_str())
            .unwrap_or("<file>");
        commands.push(manual_recommendation(
            "Inspect unresolved local imports before trusting blast-radius output",
            "unresolved_local_imports",
            "Atlas found local-looking imports that did not resolve to indexed files",
            action_level != ActionLevel::Refresh,
        ));
        commands.push(command_recommendation(
            format!("atlas deps {first_file}").as_str(),
            &["atlas", "deps", first_file],
            "inspect_first_unresolved_file",
            "one unresolved file is enough to make graph impact incomplete",
            false,
        ));
    }

    if commands.is_empty() {
        commands.push(manual_recommendation(
            "Use atlas impact <file> before editing the target file",
            "normal_graph_workflow",
            "graph is current and no unresolved local imports were found",
            false,
        ));
        commands.push(command_recommendation(
            "atlas hotspots --limit 20",
            &["atlas", "hotspots", "--limit", "20"],
            "review_hotspots",
            "hotspots highlight files with broad downstream impact",
            false,
        ));
    }

    commands
}

fn command_recommendation(
    label: &str,
    argv: &[&str],
    reason_code: &str,
    reason: &str,
    required: bool,
) -> RecommendedCommand {
    RecommendedCommand {
        kind: RecommendationKind::Command,
        command: Some(label.to_string()),
        argv: Some(argv.iter().map(|arg| (*arg).to_string()).collect()),
        label: label.to_string(),
        reason: reason.to_string(),
        reason_code: reason_code.to_string(),
        required,
    }
}

fn manual_recommendation(
    label: &str,
    reason_code: &str,
    reason: &str,
    required: bool,
) -> RecommendedCommand {
    RecommendedCommand {
        kind: RecommendationKind::Manual,
        command: None,
        argv: None,
        label: label.to_string(),
        reason: reason.to_string(),
        reason_code: reason_code.to_string(),
        required,
    }
}

fn recommendation_label(command: &RecommendedCommand) -> String {
    if command.required {
        format!("required: {} ({})", command.label, command.reason_code)
    } else {
        format!("optional: {} ({})", command.label, command.reason_code)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportProblem {
    file: String,
    import: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HotspotEntry {
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

    #[test]
    fn doctor_action_refreshes_stale_index_before_unresolved_review() {
        let gates = AgentGates {
            files_indexed: true,
            index_has_metadata: true,
            schema_current: true,
            source_delta_clear: false,
            index_fresh: false,
            unresolved_imports_clear: false,
        };

        assert_eq!(action_level_for(&gates), ActionLevel::Refresh);
        assert_eq!(status_for(ActionLevel::Refresh), DoctorStatus::Caution);
        let recommendations = recommended_commands_for(ActionLevel::Refresh, &gates, &[]);
        assert_eq!(recommendations[0].reason_code, "index_not_fresh");
        assert!(recommendations[0].required);
    }

    #[test]
    fn doctor_action_reviews_current_unresolved_imports() {
        let gates = AgentGates {
            files_indexed: true,
            index_has_metadata: true,
            schema_current: true,
            source_delta_clear: true,
            index_fresh: true,
            unresolved_imports_clear: false,
        };
        let unresolved = vec![ImportProblem {
            file: "src/lib.rs".to_string(),
            import: "./missing".to_string(),
        }];

        assert_eq!(action_level_for(&gates), ActionLevel::Review);
        assert_eq!(status_for(ActionLevel::Review), DoctorStatus::Blocked);
        let recommendations = recommended_commands_for(ActionLevel::Review, &gates, &unresolved);
        assert!(recommendations
            .iter()
            .any(|command| command.reason_code == "unresolved_local_imports" && command.required));
        assert!(recommendations
            .iter()
            .any(|command| command.argv.as_ref().is_some_and(|argv| argv
                == &[
                    "atlas".to_string(),
                    "deps".to_string(),
                    "src/lib.rs".to_string()
                ])));
    }

    #[test]
    fn doctor_action_stops_when_no_files_are_indexed() {
        let gates = AgentGates {
            files_indexed: false,
            index_has_metadata: true,
            schema_current: true,
            source_delta_clear: true,
            index_fresh: true,
            unresolved_imports_clear: true,
        };

        assert_eq!(action_level_for(&gates), ActionLevel::Stop);
        assert_eq!(status_for(ActionLevel::Stop), DoctorStatus::Blocked);
    }

    #[test]
    fn clean_doctor_gates_are_ready() {
        let gates = AgentGates {
            files_indexed: true,
            index_has_metadata: true,
            schema_current: true,
            source_delta_clear: true,
            index_fresh: true,
            unresolved_imports_clear: true,
        };

        assert_eq!(action_level_for(&gates), ActionLevel::None);
        assert_eq!(status_for(ActionLevel::None), DoctorStatus::Ready);
    }

    #[test]
    fn strict_exit_codes_cover_atlas_actions() {
        assert_eq!(ActionLevel::None.strict_exit_code(), 0);
        assert_eq!(ActionLevel::Refresh.strict_exit_code(), 10);
        assert_eq!(ActionLevel::Review.strict_exit_code(), 30);
        assert_eq!(ActionLevel::Stop.strict_exit_code(), 30);
    }
}
