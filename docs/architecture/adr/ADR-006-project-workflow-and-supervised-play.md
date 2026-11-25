# ADR-006 Project workflow and supervised Play

**Accepted on 2026-11-24**

Put `nara.toml`, project creation, Cargo build orchestration, Schema discovery, supervised Play,
Cook, and Export in headless `nara_project`. The `nara` CLI and desktop editor use the same project
workflow. Leave project orchestration outside the shipping game graph.

- Run a small project-owned Schema export entry point on demand. Load its `SchemaCatalog` from
  `.nara/schema_cache.json` and keep the last successful catalog available after a build failure.
  Replace the cache only after a successful exporter exits with a bounded UTF-8 JSON catalog.
  Cancelling a refresh stops the build or exporter and preserves the last cache before publication.
  Preserve unknown components when the cached catalog cannot describe them.
- Build the project incrementally before Play. Parse Cargo's structured diagnostics and stream
  the game's `stdout` and `stderr` into the editor Console. Leave the project's Rust code in the
  external game process.
- Supervise the process tree with a kill-on-close Job Object on Windows and a managed process group
  on Unix. Stop and restart must terminate and reap the previous game process. Use development
  asset watching for live data changes.
- Export a self-contained executable and `.npak` with no source or editor dependencies. Verify the
  packaged game from a clean directory before claiming standalone delivery.
