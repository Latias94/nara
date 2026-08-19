---
type: "Verification Evidence"
title: "EPB-U5 standalone Project Browser product closure"
description: "Verifies one generic Editor browse, filter, Scene/Prefab Open, equivalent Refresh, and last-good failure trajectory on Windows."
timestamp: 2026-08-19T21:35:00Z
record_id: "2acde0398a8b467b93fe714241a5e216"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "1a832df"
verified_by: "codex-root"
---

# Verification

Verified EPB-U5 implementation commit `1a832df` against the final standalone Project Browser
trajectory. The proving product is the same generic `nara-editor` binary used by Project Manager
and Recovery; it does not compile or load the fixture project's Rust crate.

# Result

The standalone Editor now presents one capability-bound, read-only Project Browser over
manifest-declared roots. It can filter and open closure-external Scene and Prefab documents through
the existing document owners, preserve the earlier Scene while the Prefab becomes active, publish
an equivalent successor snapshot on Refresh, and retain the complete last-good snapshot when final
directory-lineage revalidation fails.

The product boundary remains narrow. Browser navigation is local UI state; Refresh and Open are
exact-frame Host actions. The egui adapter has no filesystem authority, the Host retains native
identity and scan budgets, and `nara_fs` remains the only native directory-enumeration owner.
External Play, import/reimport, watchers, source mutation, animation/shader authoring, and extension
hosting are not activated.

# Evidence

- `cargo fmt --all` passed before the implementation commit.
- The focused Editor command below passed 2/2 tests:

  ```text
  cargo nextest run -p nara-editor --test generic_open_recovery -E 'test(standalone_project_browser_opens_external_documents_and_retains_last_good_refresh) | test(ordinary_generic_open_preserves_degraded_source_through_reopen_and_recovery)' --test-threads=1
  ```

- The focused Host command below passed 1/1 test and proves exact equivalent snapshot facts,
  monotonic generation, and complete last-good retention:

  ```text
  cargo nextest run -p nara --lib --features 'tooling,serde,runtime-2d' -E 'test(generic_editor_drives_the_initial_source_index_and_retains_it_during_refresh)' --test-threads=1
  ```

- The prior EPB-U1 through EPB-U4 records remain the unit-level evidence for bounded Windows
  handle enumeration, the Host source-index transaction, closure-external document admission, and
  the UI-neutral/egui Project Browser. This final record does not duplicate those matrices.
- A multi-reviewer code review found one valid snapshot-comparison gap. The fix now compares status,
  ordered entries, diagnostics, usage, root facts, and source-basis identities instead of comparing
  only entries and generation. The focused Editor and Host tests above and the native trajectory
  passed after that correction.
- The real Windows native command was:

  ```powershell
  $env:CARGO_BUILD_JOBS='1'
  New-Item -ItemType Directory -Force 'target\codex-temp' | Out-Null
  $env:TEMP=(Resolve-Path 'target\codex-temp').Path
  $env:TMP=$env:TEMP
  $env:NARA_EDITOR_GENERIC_OPEN_CANARY='1'
  try {
      cargo run -p nara-editor -- .\tests\fixtures\generic-editor\known-unbound --profile desktop
  } finally {
      Remove-Item Env:NARA_EDITOR_GENERIC_OPEN_CANARY -ErrorAction SilentlyContinue
  }
  ```

  It exited with code `0` and emitted version-2 evidence for one window, presented Project Manager
  and Recovery, an `authoring_degraded` workspace, Project sidebar presentation, exact external
  Scene and Prefab document opens, equivalent Refresh generation `2`, retained last-good generation
  `2`, failure code `project.source-index.directory-lineage-changed`, safe diagnostics, unavailable
  external Runtime, no installed project content, empty Play, and `result: ok`.
- The guarded project includes project Rust `compile_error!` sentinels. Successful Generic Open and
  native browsing therefore prove that the browser path does not compile the project crate.
- `git diff --check` passed for the implementation scope. User-owned `.codex-research/` content was
  untouched and unstaged.

# Follow-up

No successor is active automatically. External supervised Play remains owned by OQ-039. Import,
reimport, watchers, source mutation, richer asset inspection, animation/shader authoring, and
extension hosting each require a focused plan, registration, and product tracer rather than an
expansion of this read-only browser transaction.

# Citations

- `1a832df` (`feat(editor): prove standalone project browser workflow`)
- `docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-19T180000Z-epb-u1-bounded-handle-directory-enumeration.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-19T182322Z-epb-u2-bounded-host-project-source-index-9ac12073205d4064a4bc76729225caf8.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-19T192016Z-epb-u3-indexed-authoring-document-admission-684770f357d14a0184e3cdbec4751355.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-19T204830Z-epb-u4-bounded-editor-project-browser-495c3c9cb92b4f94aa0895dfbce04ebf.md`
