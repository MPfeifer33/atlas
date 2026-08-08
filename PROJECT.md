# PROJECT.md — atlas

**What:** Codebase knowledge graph. Scans source files, extracts imports/exports, resolves dependencies, and answers structural queries: deps, reverse deps, blast radius.

**Status:** Agent-usable graph tool. Scan, modules, deps, rdeps, blast, stats, doctor, hotspots, symbols, and impact all working. Multi-language (Rust, TS/JS, Python, Go).

**Tech:** Rust 2021, clap 4, serde/serde_json, regex, walkdir, thiserror.

**Storage:** `.agent-atlas/graph.json` under repo root, gitignored.

## Module Ownership

| Module | Owner | Status |
|--------|-------|--------|
| cli.rs | Nix | Done |
| main.rs | Nix | Done |
| graph.rs | Nix | Done |
| parse.rs | Nix + Bjarn | Enhanced (brace groups, pub(crate), super::) |
| scan.rs | Bjarn | Rewritten (nested crate roots, full module resolution) |
| store.rs | Nix | Done |
| report.rs | Nix | Enhanced (blast ergonomics: via, risk, hints) |

## Usage

```sh
atlas scan                          # build the knowledge graph
atlas scan --force                  # rebuild from scratch
atlas modules                       # list all indexed files
atlas deps src/main.rs              # what does this file depend on?
atlas rdeps src/graph.rs            # what depends on this file?
atlas blast src/graph.rs            # transitive blast radius
atlas blast src/graph.rs --depth 3  # limit traversal depth
atlas doctor                        # graph health and map confidence
atlas hotspots                      # high fan-in / high blast-radius files
atlas symbols Graph                 # search exported symbols
atlas impact src/graph.rs           # pre-edit orientation bundle
atlas stats                         # graph summary
```

## Recent Changes

- **2026-08-06** — Suite doctor parity (Helix):
  - Added `atlas.doctor.v1` JSON envelope with top-level `status`,
    `action_level`, `gates`, `advice`, `recommendations`, and
    `recommended_commands`.
  - Kept compatibility aliases for existing `health`, `index`,
    `unresolved_imports`, `external_imports`, `isolated_files`, and `hotspots`
    consumers.
  - Added `doctor --strict` gate exits: `none` = 0, `refresh` = 10,
    `review`/`stop` = 30.
  - Added explicit action precedence: refresh stale maps before reviewing
    unresolved import warnings.
  - 26 tests (up from 21).
- **2026-08-08** — Holt suite follow-up (Helix):
  - Skipped generated frontend output directories such as `.svelte-kit`,
    `.nuxt`, and `.output` during source scans.
  - Resolved Rust `super::super::*` imports that appear inside inline
    `#[cfg(test)] mod tests` modules by accounting for the extra inline module
    frame.
  - 28 tests (up from 26).
- **2026-08-02** — Agent-facing hardening and workflow expansion (Bjarn, with Helix review lane opened):
  - Added unresolved local import tracking per file.
  - Added external import tracking per file.
  - Added stats totals for unresolved and external imports.
  - Fixed TS/JS relative import resolution to resolve from the importing file's directory.
  - Added JS side-effect import and dynamic `import()` extraction.
  - Added Python relative `from .module import ...` extraction/resolution.
  - Added Python `from . import module` expansion.
  - Improved Go package best-effort matching by parent directory.
  - Added Rust external import capture.
  - Fixed Rust unresolved multi-segment imports so they do not fall back to crate root.
  - Added `doctor`, `hotspots`, `symbols`, and `impact` commands.
  - Fixed JSON blast `high_risk` threshold to match text output.
  - Added README and `docs/AGENT-WORKFLOW.md`.
  - Added index metadata and stale detection for changed, missing, and new source files.
  - 21 tests (up from 12).
- **2026-06-27** — Major Rust dependency resolution overhaul (Nix + Bjarn collaborative):
  - Fixed: nested crate root detection (e.g. `app/src-tauri/src/`)
  - Fixed: `super::`, `self::`, chained `super::super::` resolution
  - Fixed: brace group imports (`use super::types::{A, B}`)
  - Fixed: symbol tail stripping (right-to-left segment walking)
  - Added: `BlastEntry` with relationship chains (`via`), fan-out risk, next-step hints
  - Added: high-risk file detection in blast output
  - Result: Meridian-Hillock scan went from 0 deps to 614 deps
  - 12 tests (up from 6)
- **2026-06-22** — Initial skeleton with scan/modules/deps/rdeps/blast/stats working.
