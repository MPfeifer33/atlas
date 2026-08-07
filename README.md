# Atlas

Atlas is a local codebase knowledge graph for agents and humans. It scans a
repository, records source files, extracts imports/exports, resolves local
dependencies, and answers orientation questions before an edit starts.

## Suite Context

Atlas is part of a local-first agent tool suite centered on
[Switchboard](https://github.com/MPfeifer33/switchboard):

- [Probe](https://github.com/MPfeifer33/probe): project preflight and drift
  scanner
- [Latch](https://github.com/MPfeifer33/latch): repo-local coordination ledger
- [Atlas](https://github.com/MPfeifer33/atlas): codebase graph and impact map
- [Sentinel](https://github.com/MPfeifer33/sentinel): regression risk watcher
- [Witness](https://github.com/MPfeifer33/witness): reproducible command
  evidence recorder

## Quick Start

```sh
cargo run -- scan --force
cargo run -- doctor
cargo run -- hotspots
cargo run -- symbols Graph
cargo run -- impact src/graph.rs
```

The scan writes `.agent-atlas/graph.json` under the target repo. That file is
ignored by git and can be regenerated at any time.

Install the CLI from a local checkout:

```sh
cargo install --path .
atlas --help
```

Atlas records index metadata and file fingerprints. Text commands warn when the
saved graph appears stale; `doctor` reports stale, changed, missing, and new
source files in both text and JSON mode.

## Core Commands

```sh
atlas scan --force
atlas modules
atlas deps src/main.rs
atlas rdeps src/graph.rs
atlas blast src/graph.rs --depth 3
atlas stats
```

## Agent-Oriented Commands

```sh
atlas doctor
atlas doctor --strict
```

Checks graph health. The most important field is unresolved local imports: if it
is non-zero, Atlas is telling you its map is incomplete and an agent should
inspect those files before trusting blast-radius output.

`doctor` also reports index freshness. If it says the graph may be stale, run:

```sh
atlas scan --force
```

`doctor --strict` prints the same report, then exits by the reported
`action_level`: `none` exits 0, `refresh` exits 10, and `review`/`stop` exits
30. JSON doctor output exposes the suite-oriented envelope:

- `schema_version: atlas.doctor.v1`
- `status: ready | caution | blocked`
- `action_level: none | refresh | review | stop`
- `gates` for indexed files, schema freshness, source deltas, and unresolved
  import clearance
- `recommended_commands` with `kind`, `argv`, `reason`, `reason_code`, and
  `required`

```sh
atlas hotspots --limit 20
```

Ranks files by reverse dependency count and transitive blast radius. Use this to
find shared modules, risky edit targets, and files that deserve tests after a
change.

```sh
atlas symbols [query] --limit 50
```

Lists exported symbols. The optional query matches symbol names and paths.

```sh
atlas impact <file> --depth 5
```

Bundles file facts, direct dependencies, reverse dependencies, blast radius,
map warnings, and next-step hints for one edit target.

## JSON Mode

Every command supports machine-readable output:

```sh
atlas --format json doctor
atlas --format json impact src/scan.rs
```

`--format json` is intended for agents, scripts, dashboards, and Switchboard
summaries.

## Supported Languages

Atlas currently scans:

- Rust
- TypeScript / JavaScript
- Python
- Go

Resolution is intentionally local-project focused. External packages are kept as
external imports, while local imports that should resolve but do not are tracked
as unresolved imports.

## Map Confidence

Atlas distinguishes three import outcomes:

- `deps`: local imports resolved to files in the graph.
- `external_imports`: package or standard-library imports outside the scanned
  repo.
- `unresolved_imports`: local-looking imports Atlas expected to resolve but
  could not.

Treat unresolved imports as a warning that blast-radius results may be
under-counting impact.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) and
[NOTICE](NOTICE). Redistributed or derivative works must preserve the NOTICE
attribution required by the license.
