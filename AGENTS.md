# AGENTS.md

## Bootstrap

Before editing:

1. Run `git status --short --branch` and preserve concurrent changes.
2. Read [the strategy](STRATEGY.md) for product scope and [the architecture overview](docs/architecture/overview.md)
   for the accepted v2 ADRs. Read the ADRs that govern the boundaries you change.
3. For v2 refactor work, read the current phase and its gates in [the master plan](docs/plans/v2-refactor-master-plan.md).
   Consult the `legacy/v0-imgui-reference` tag for v0 history.

## Architecture

- Build from a `reference-game` need or a real editor workflow. Respect the ownership boundaries in
  the v2 ADRs and keep future capabilities behind the documented extension seams.
- Retire superseded pre-1.0 code outright. Use Git tags for history and remove obsolete APIs,
  frontends, and fixtures when their replacements land. Keep `legacy-*` out of the feature graph.
- Fix crate boundaries in the owning code. Forbid `pub mod __private`, first-party anti-tamper hooks,
  manual byte-accounting ledgers, fixed-slot fault routing, and thread-local quarantine.
- Return errors and clean up owned resources. Forbid library `std::process::abort()` and GPU leaks
  through `mem::forget`.
- Keep games free of editor and Open-GPUI dependencies. Keep `nara_editor_core` UI-framework independent.
  Use one resolved `wgpu` version and source revision in both repositories.
- Keep the root facade within 1,500 lines, treat 30,000 lines per crate as a review threshold, and
  target cold CI below 15 minutes. Keep one active roadmap and at most 25 active documents.

## Verification and delivery

- Preserve work you did not create. Keep `repo-ref/` read-only and out of commits. Use `rg` for
  discovery and `apply_patch` for manual edits.
- Run one Cargo command at a time in this checkout. Reuse `target`, set `CARGO_BUILD_JOBS=1` for
  substantial builds, prefer focused `cargo nextest run`, and format with `cargo fmt --all`.
- Verify the current phase's exit gates before moving on. Record actual results and platform limits.
  Review diffs before formatting and staging, and use precise Conventional Commits.
- Write project artifacts in English and communicate with the user in Chinese.
