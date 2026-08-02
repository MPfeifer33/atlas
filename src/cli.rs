use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::AtlasError;

#[derive(Parser, Debug)]
#[command(name = "atlas", version, about = "Codebase knowledge graph for agents")]
pub struct Cli {
    /// Project root override
    #[arg(long, global = true)]
    pub repo: Option<PathBuf>,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn resolve_repo(&self) -> Result<PathBuf, AtlasError> {
        if let Some(ref repo) = self.repo {
            return Ok(repo.clone());
        }
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return Ok(PathBuf::from(path));
            }
        }
        std::env::current_dir().map_err(AtlasError::Io)
    }

    pub fn is_json(&self) -> bool {
        matches!(self.format, OutputFormat::Json)
    }
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scan project and build the knowledge graph
    Scan {
        /// Force rescan even if index exists
        #[arg(long)]
        force: bool,
    },
    /// List all modules/files in the graph
    Modules,
    /// Show what a specific file depends on
    Deps {
        /// File path (relative to repo root)
        file: String,
    },
    /// Show what depends on a specific file (reverse deps)
    Rdeps {
        /// File path (relative to repo root)
        file: String,
    },
    /// Show blast radius — transitive reverse dependencies
    Blast {
        /// File path (relative to repo root)
        file: String,
        /// Max depth for transitive traversal
        #[arg(long, default_value = "5")]
        depth: usize,
    },
    /// Check graph health and map confidence
    Doctor {
        /// Max rows to show in each section
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Show high fan-in / high blast-radius files
    Hotspots {
        /// Max hotspots to show
        #[arg(long, default_value = "15")]
        limit: usize,
    },
    /// Search exported symbols
    Symbols {
        /// Optional case-insensitive symbol/path search
        query: Option<String>,
        /// Max symbols to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Bundle deps, rdeps, blast radius, and hints for a file
    Impact {
        /// File path (relative to repo root)
        file: String,
        /// Max depth for transitive traversal
        #[arg(long, default_value = "5")]
        depth: usize,
    },
    /// Show graph statistics
    Stats,
}
