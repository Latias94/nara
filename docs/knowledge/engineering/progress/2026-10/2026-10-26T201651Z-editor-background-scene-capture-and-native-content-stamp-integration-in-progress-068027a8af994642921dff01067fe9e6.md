---
type: "Work Progress"
title: "Editor background Scene capture and native content stamp integration in progress"
description: "Actual Editor capture, persistence receipts, Image hydration and native stamp fencing are being integrated; review found publication and Image races, with fresh product verification pending."
timestamp: 2026-10-26T20:16:51Z
record_id: "068027a8af994642921dff01067fe9e6"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
---

# Summary

The preceding turn made progress and qualified lower-level capture. This turn is actual
Editor product integration, still in progress. The complete foundation Goal remains active;
the giant replacement is uncommitted on `refactor/declarative-prefab-overlays`, based on
local main `47a2000`. No commit, merge, push or PR has been made. Preserve existing
untracked `vendor/` unread/untouched and keep `repo-ref/` read-only. Cargo remains serial
with `CARGO_BUILD_JOBS=1`, root `target`, Rust `+1.95.0`.

# Details

## New Production Source, Not Yet Qualified

- `src/project_host/runtime/editor/scene_capture.rs` replaces the product's direct-only Scene
  projection with owned IO tasks, exact root/source/session selection, immutable captured
  expanded candidates and actual-demand dependency stamps. Unrelated open Prefabs do not
  invalidate a successful capture. Disk-to-open, close, edit, Save and Reopen do.
- Scene/Prefab persistence lanes now hold optional admitted authoring source receipts. Pure
  `accepted_source_observation()` reads identity/parent/digest from the actual publication Host,
  returning None for missing/uncertain binding. Source facts can advance from accepted
  persistence evidence and refresh equivalent canonical native admission after Reopen.
- Shared Schema and source-fact arrays now use Arc, including startup preparation. The reusable
  `AuthoringSourceValidation` owns exact closure and Schema receipts, uses bounded IO tasks and
  reserves the held capture's retained usage while revalidating. It does not read files on UI.
- Startup preview also requires an initial async validation. A completed Compose result remains
  owned through a separate Candidate validation before first materialization. Selected source
  receipts/revisions are checked again on the owner. Actual raw closure stays held until
  materialization succeeds; periodic validation then detects disk changes.
- Cancellation keeps the old Scene task until physical return before admitting replacement.
  Test-only `scene_capture.next_completion_gate: Option<Box<dyn FnOnce() + Send>>` runs AFTER
  preparation and before returning the result, for deterministic stale-result tests.
- Content publication has a terminal generation distinct from Pending. Pending is an empty
  rejected projection used to revoke authorization while retaining last-good; it is not a
  user error. Source/format/Schema/budget failures retain bounded diagnostic facts.
- Runtime retains a valid complete content stamp independently of the takeable candidate.
  Session/App expose it only while actual source selection remains current. Native display
  reuse, composite promotion and final picking check that same stamp. Invalidating it retires
  input tokens while leaving the target's last-good pixels available.
- Frame maintenance facts now include capture work, next recheck delay and unsaved dependency
  count. Maintenance alone does not cause redraw; actual stamp/unsaved changes do. Lightweight
  background source validation currently uses a private 500ms recheck interval while a Scene
  viewport is enabled. No performance or watcher-equivalence claim is made.

## Review Findings And Current Disposition

Chandrasekhar independently found:

1. P1: Pending and success with no open participants shared a stamp. Parent separated terminal
   generation; real disk-only second-Scene tests must verify it.
2. P1: validating only 500ms after acceptance allowed mutations after Compose to activate stale
   output. Parent added pre-publication Candidate validation and startup validation.
3. P1: an older Source Index catalogue could revoke a newly captured Image, with no requeue.
4. P2: replacing hydration dropped logical task ownership before physical return, so retained
   decoder budget could reject and permanently lose its successor.
5. P2: Image completion newly performed whole-file hash IO on the main thread.

Findings 3-5 are assigned back to the Image owning worker below. Do not count their initial
enqueue implementation as final or rely on its current_source_matches main-thread read.
Later genuine catalogue revocation must still work; permanent exemption is not sufficient.

## Agent Continuity And Write Sets

- Hilbert `01a179b8-d475-7831-bbcf-264d755b149d`: source-ready and CLOSED. Source receipt APIs
  in `authoring/document.rs`, `native_source.rs`, `persistence.rs`, `file_publication.rs`;
  tests `document/source_receipts.rs`, `persistence/tests/source_observation.rs`.
- Galileo `01a179b9-1702-70b1-884f-3d1d695fc953`: initial Image integration source-ready and
  CLOSED. It is being corrected for the independent findings, not accepted unchanged.
- Pauli `01a179b9-48ec-71a3-8e4b-f3c62b49508c`: native/root stamp integration source-ready
  and CLOSED. Own former files: preview.rs, authoring_runtime.rs, epoch tests, editor app.rs,
  platform/imgui.rs, scene.rs and scene_image_tests.rs. Runtime methods
  `preview_extent`, `current_preview_content_stamp`, `invalidate_preview_content` exist;
  `EditorImageRuntimeTask::{cancel,return_observer}` and test getters
  `test_preview_world`, `test_preview_candidate` are implemented.
- Popper `01a179c5-daa1-7820-a9f1-17ce69f5002e`: ACTIVE on only
  `src/project_host/runtime/editor/scene_capture/tests.rs`. Initial 7 actual indexed workflows
  are source-ready; parent requested a deterministic after-Compose disk/Schema mutation test.
  Tests cover nested native Transform/provenance, unsaved edit/Undo, physical cancellation,
  Save/Reopen, disk validation, unrelated sources and close/reopen. No Cargo/fmt/commit.
- Chandrasekhar `01a179cd-5f3c-78d2-9394-e3736e8c749a`: ACTIVE Image fix worker, owns only
  `image_import/{hydration,transaction,store,capture_hydration}.rs` and related new private
  tests. No Cargo/fmt/commit. Parent requested production freeze before test expansion.

## Verification Boundaries

The predecessor's Scene507/root501 pass remains evidence only for that earlier source.
This turn's no-presentation root check passed before subsequent publication/validation fixes.
`cargo +1.95.0 check -p nara_tooling --all-features --locked` passed after new maintenance
fields (exec session `92143`, terminal 0). No Cargo process is live at this capture.
New source receipts, Image tests, actual Editor Scene tests and native bridge are not yet
compiled or executed together. No format, full root, native, independent consumer or
Reference Game qualification is claimed for this new product source.

# Next Action

1. Receive Image production freeze and Popper's final test handoff; preserve their write sets.
2. Compile root all-features library, fix integration errors; inspect diff before formatting
   after writers freeze. Then focused serial nextest for source_receipts/source_observation,
   capture_hydration and scene_capture::tests, plus preview epoch tests.
3. Verify the five review findings against real interleavings; re-review corrections. Review
   actual captured assets against stale source-index/catalogue publication, not just queues.
4. Run root regression, affected native Editor qualification and relevant external consumer;
   migrate old synchronous preview fixtures to explicit pending/drive observation where needed.
5. Update ADR/design/implementation status with actual product scope and limits. Structured
   Prefab conflict observation, convert-to-local, UI Canvas capture integration and the prior
   descriptor-work gap still remain. Full foundation completion is not implied by this slice.

# Citations

- [Predecessor qualified capture](2026-10-26T192056Z-independent-authoring-capture-and-preview-dependency-stamp-integration-c31cf6ef3eaa457a87528419c09ea49c.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Overlay design](../../../../architecture/prefab-overlay-design.md)
