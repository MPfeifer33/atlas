# PROJECT.md — atlas

**What:** Codebase knowledge graph. Scans source files, extracts imports/exports, resolves dependencies, and answers structural queries: deps, reverse deps, blast radius.

**Status:** MVP complete. Scan, modules, deps, rdeps, blast, stats all working. Multi-language (Rust, TS/JS, Python, Go).

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
atlas stats                         # graph summary
```

## Recent Changes

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
