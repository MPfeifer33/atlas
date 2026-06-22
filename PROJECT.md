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
| parse.rs | Nix | Done |
| scan.rs | Nix | Done |
| store.rs | Nix | Done |
| report.rs | Nix | Done (Bjarn enhancing) |

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

## Last Updated

2026-06-22 — Initial skeleton with scan/modules/deps/rdeps/blast/stats working.
