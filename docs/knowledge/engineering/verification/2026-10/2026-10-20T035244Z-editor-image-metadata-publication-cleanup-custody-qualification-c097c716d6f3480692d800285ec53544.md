---
type: "Verification Evidence"
title: "Editor Image metadata publication cleanup custody qualification"
description: "Register metadata publication before writes and retain cleanup through terminal failure, close, unwind and abandonment."
timestamp: 2026-10-20T03:52:44Z
record_id: "c097c716d6f3480692d800285ec53544"
producer_id: "codex-root"
run_id: "editor-image-custody-20261020"
related_plan: "docs/plans/2026-10-20-editor-image-publication-custody-plan.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "bae0440"
---

# Verification

Baseline: `f3bc338`, following startup source `eb1ee98` and Scene/Prefab lifetime source
`fe3ad96`. Windows 11, Rust 1.95.0. Root runs Cargo serially with CARGO_BUILD_JOBS=1 and the
existing target; unrelated concurrent files are preserved.

## Result

The concrete Image metadata FilePublicationHost is now registered with the private runtime
retirement owner before mutation. The session's import transaction retains a handle before
borrowing that Host. Failed temporary cleanup and unwind after publication therefore retain
the original capability and retry authority independently of the operation result or session.
The runtime cannot complete close while registered metadata cleanup remains pending.

Successful publication retires the mutable Host after extracting the existing exact parent/name
read source and accepted binding. Pending image completion still opens metadata afresh and checks
its identity and digest. Refresh, source conflict, uncertain receipt and last-good pixel policies
are preserved. The existing finite custody adds one concrete publication variant, not a new job
framework or a lifetime owner for the whole image catalog.

## Evidence

- Focused engine/public cohort: **116 passed**, 81 excluded;
  `512ef194-c21b-4782-a3cc-f29dc9677ed5`. Lower persistence, Editor runtime, private
  Open/persistence and public `editor_persistence` coverage includes the three new compound tests.
- Editor/application cohort: **186 passed**, 19 native tests excluded;
  `0396b396-932e-4ee4-9806-bd205f563d32`. Library plus `application_contract`,
  `first_game_workflow` and `generic_open_recovery` preserve actual consuming flows.
- Selected actual Windows/DX12 native cohort: **2 passed**;
  `52239d96-a875-48bd-917c-9dd534eee556`. Ordinary Scene/Prefab Save/Reopen and Scene image
  reimport preserve source and present changed pixels without document/extent changes.
- Workspace locked all-features/all-targets check passed in **56.28 seconds**. Formatting and
  diff checks pass. Existing unused-code warnings remain; no new release claim follows.

Commands, executed serially against source `bae0440`:

```text
cargo +1.95.0 nextest run --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --lib --test editor_persistence --locked -E '(test(project_host::persistence::) | test(project_host::runtime::tests::) | test(project_host::runtime::editor::open::) | test(project_host::runtime::editor::persistence::) | binary(editor_persistence)) & not test(persistence_authority_surface_is_opaque_and_linear)' --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara-editor --lib --test application_contract --test first_game_workflow --test generic_open_recovery --locked --test-threads=2 --no-fail-fast
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes) | test(native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source)' --test-threads=1 --no-fail-fast
cargo +1.95.0 check --workspace --all-features --all-targets --locked
cargo +1.95.0 fmt --all -- --check
git diff --check
```

Native execution sets WGPU_BACKEND=dx12; all Cargo runs set CARGO_BUILD_JOBS=1 and reuse target.
Counts overlap and are selected development coverage, not full product or platform acceptance.

New compound tests use a shared private atomic-only CleanupTestControl that does not retain a
Host. They check nonempty actual temporary files, exact directory entries, PNG/metadata/Scene
bytes and Workspace state before abandonment. Drop must add one retained runtime; repeated
quarantine drives keep cleanup and files until external release, then delete the temporary and
return to the original count. Cases cover missing/existing metadata terminal failure, task
shutdown completed while file close remains incomplete, and an injected panic after actual
publication but before cleanup handling. That panic unwinds through the real Host mutex; retry
must recover its poisoned guard and retain its existing cleanup authority.

Two read-only integration reviews find no current reachable blocking defect. The transaction
author distinguishes self-review from the independent retirement/control/test review, and the
retirement author independently challenges the transaction and compound tests. Existing metadata
replacement-after-preparation tests retain fresh identity/digest rejection and last-good pixels.
Image imports run in the ordinary Authoring lane while content-runtime replacement runs in the
trusted compiled lane; no nonexistent native intersection is claimed. The finite custody transfer
continues to cover all registered concrete variants by construction.

## Limits And Follow-up

The fault control is deterministic test instrumentation, not an exhaustive OS-failure campaign.
This repair covers cleanup already transferred into the publication Host, including unwind after
publication returns. TemporaryFile's own earlier unwind retains its existing best-effort Drop
semantics; no general lower-filesystem panic guarantee is added. OS calls remain non-preemptible.
CLI schema/repair paths explicitly report manual cleanup obligations and retain their separate
contracts. No latency improvement, new measurement envelope or cross-platform release claim follows.

After this registered correctness repair, unmeasured Browser/source-receipt/compiled-content,
capture-disposal, GPU-hold and aggregate-worker costs require their concrete consumer evidence.
Multiple same-kind documents, code freshness and Tile tooling remain separate capability work.

## Citations

- [Plan](../../../../plans/2026-10-20-editor-image-publication-custody-plan.md)
- [Publication owner](../../../../../src/project_host/runtime/editor/image_import/transaction/publication.rs)
- [Compound tests](../../../../../src/project_host/runtime/tests/image_publication_custody.rs)
- [ADR 0115](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)
