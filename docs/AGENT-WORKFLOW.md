# Atlas Agent Workflow

This workflow is for agents using Atlas before editing a repository.

## 1. Build Or Refresh The Map

```sh
atlas scan --force
```

Run this before serious analysis. Atlas stores the graph in
`.agent-atlas/graph.json`; stale graphs are cheap to replace.

## 2. Check Map Health

```sh
atlas doctor
atlas --format json doctor
```

Stop and inspect unresolved local imports before trusting blast-radius output.
Unresolved imports mean Atlas found a local-looking dependency but could not map
it to a file.

External imports are not failures. They are package or standard-library edges
outside the scanned repo.

Also check index freshness. If `doctor` reports changed, missing, or new source
files, run `atlas scan --force` before using `impact` or `blast` to plan work.

## 3. Find Important Files

```sh
atlas hotspots --limit 20
```

Use hotspots to identify shared modules and high-impact files. A file with many
reverse dependencies or a wide transitive blast radius deserves stronger tests
and a slower review pass.

## 4. Orient On The Edit Target

```sh
atlas impact path/to/file.rs --depth 5
```

`impact` is the preferred pre-edit command. It combines:

- direct dependencies
- direct reverse dependencies
- transitive blast radius
- unresolved/external import warnings
- next-step hints

## 5. Search Exports

```sh
atlas symbols Memory
atlas --format json symbols Memory
```

Use symbol search when the user names a concept but not a file path.

## 6. Use With Latch And Switchboard

For a coordinated project run:

```sh
switchboard --transport store project bind project.name /path/to/repo --latch-bin /path/to/latch
switchboard --transport store project actor-map project.name agent.bjarn bjarn
switchboard --transport store send project.name --from agent.bjarn --type status "Atlas scan started."
latch --actor bjarn claim acquire src --ttl 2h --intent "Atlas-guided implementation"
```

Keep Switchboard messages for the conversational trail and Latch records for
claims, tasks, decisions, and completion receipts.

## Interpretation Rules

- If `doctor` reports unresolved local imports, call that out in the work plan.
- If `doctor` reports a stale index, refresh before making graph-based claims.
- If `impact` reports a broad blast radius, test direct dependents first.
- If a target file is a hotspot, avoid broad refactors unless the user asked for
  them.
- If `symbols` cannot find a named concept, search the source directly before
  assuming the concept is absent.
