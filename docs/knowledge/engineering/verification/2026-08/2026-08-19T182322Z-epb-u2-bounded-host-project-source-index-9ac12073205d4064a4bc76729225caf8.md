---
type: "Verification Evidence"
title: "EPB-U2 bounded Host project source index"
description: "Verifies deterministic capability-bound source indexing, atomic last-good refresh, bounded lineage revalidation, and Runtime-closure isolation."
timestamp: 2026-08-19T18:23:22Z
record_id: "9ac12073205d4064a4bc76729225caf8"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8bdc4de"
verified_by: "codex-root"
---

# Verification

Verified implementation commit `8bdc4de` against EPB-U2 of the Editor Project Browser and Source
Index v1 plan. The proving scope is the Host-private Authoring source-index transaction, its bounded
Windows local-NTFS consumer path, immutable last-good snapshot, and generic Editor frame drive.

# Result

EPB-U2 passes. An accepted Authoring Project now starts one source-index attempt from the retained
project-root capability and effective manifest paths. The attempt normalizes role-bearing roots,
uses one FIFO frontier and one active move-only directory cursor, classifies portable relative
source facts deterministically, and publishes only after bounded manifest, bundle, and traversed
directory lineage revalidation.

The source index is owned by the Authoring Project session rather than the Runtime content closure,
toolkit, egui Adapter, or native platform shell. Initial scan and explicit Refresh are its only
generation triggers. Refresh preserves the exact prior `Arc<ProjectSourceIndexSnapshot>` until a
complete successor publishes; budget or identity failure exposes a structured terminal diagnostic
while retaining that last-good generation.

Isolated rejected native entries and unavailable subtrees become bounded relative diagnostics so
safe siblings remain visible. Canonical spelling ambiguity, live-identity aliasing, incomplete
enumeration, directory replacement, authority drift, and global budget overflow reject the entire
candidate. No raw native path, handle, `FsError`, asset-database mutation, Runtime registry, async
executor, watcher, or ambient directory traversal enters the transaction.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check -p nara --features runtime-2d,serde,tooling` passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features
  runtime-2d,serde,tooling -E 'test(mixed_source_tree_publishes_once_in_lexical_order_with_safe_rejections)
  | test(refresh_budget_failure_retains_the_last_good_generation) |
  test(final_lineage_revalidation_rejects_directory_replacement) |
  test(generic_editor_drives_the_initial_source_index_and_retains_it_during_refresh)'
  --test-threads=1` passed 4/4 tests.
- The mixed local-NTFS fixture proves lexical publication, independent asset and `.meta` facts,
  closure-external Scene/Prefab classification, script and unknown facts, isolated hard-link
  rejection, bounded multi-frame progress, and no partial snapshot.
- The refresh and replacement tests prove last-good `Arc` retention across entry-budget failure and
  final directory-identity drift. The Editor integration test proves initial Scanning, pending-work
  progress, terminal Ready, atomic generation replacement, and that browser-only unreferenced Scene
  and script files do not change the accepted Runtime content digest.
- `cargo fmt --all`, `git diff --check`, and the scoped searches for `RuntimePlan`,
  `ComponentRegistry`, `World`, `std::fs::read_dir`, Tokio, async-std, and watcher dependencies
  passed for the source-index owner. Production traversal remains exclusively under `nara_fs`.
- The implementation is split into a contract/state facade, one deep transaction module, an exact
  budget module, and focused native/Host tests. No speculative public trait, source-index crate, or
  generic task framework was introduced.

# Follow-up

EPB-U3 is the sole next active unit. It must derive one immutable Authoring document-admission basis
from the accepted session, resolve only exact indexed Scene/Prefab facts, reopen and revalidate them
through the project-root capability, and retarget the existing typed persistence lanes with the
existing dirty Save/Discard/Cancel protocol. It must not add Runtime, UI, importer, asset-database,
async, or project-code execution authority.

# Citations

- `8bdc4de` (`feat(editor): add bounded project source index (U2)`)
- `docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
