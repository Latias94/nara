---
type: "Work Progress"
title: "Independent authoring capture and preview dependency stamp integration"
description: "Shared actual-demand capture admits independent Scene and open Prefab snapshots; Editor task scheduling and native publication remain open."
timestamp: 2026-10-26T19:20:56Z
record_id: "c31cf6ef3eaa457a87528419c09ea49c"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
---

# Summary

The complete foundation Goal remains active. This advances reusable authoring capture and
renderer-neutral dependency stamps, not the complete Prefab workflow or foundation rebaseline.
The large production replacement remains uncommitted on `refactor/declarative-prefab-overlays`,
based on local main `47a2000`. No merge, push or PR was performed. Existing untracked `vendor/`
remains unread and untouched; `repo-ref/` was read-only.

# Details

## Source Ownership

- Startup and independent Scene capture call one actual-demand acquisition owner in
  `src/project_host/authoring/project/capture.rs`. The Schema receipt retains a reusable
  Schema-only admission baseline; each capture owns an independent budget and asset database.
  Future Image identity-store capacity remains a startup-only reservation.
- Scene and Prefab sessions expose immutable shared documents. Prefab source publication now
  replaces its `Arc` only after accepted candidate/history admission. Held snapshots remain
  unchanged across edits, Undo/Redo, Reopen and rejection.
- `capture_scene_request` owns shared root/Prefab documents, the original root revision,
  exact shared Schema authority and source receipts. Preparation survives changes or disposal
  of the originating sessions. The returned capture exposes the original root revision; a
  future Editor task envelope must also retain workspace document identity and selection generation.
- Only actually demanded open Prefab snapshots activate Schema/source checks. A selected snapshot
  must match the admitted locator, canonical key, file identity/parent, metadata and exact Schema
  basis. Multiple open sessions claiming one demanded canonical source reject as ambiguous.
  Unrelated snapshots do not trigger file or Schema validation.
- Composition backing explicitly distinguishes File from Session content. Session rows retain
  full lineage revision and the last accepted disk receipt separately; `publish_document`
  refuses to treat a working copy as a clean file-backed source.
- Successful capture retains the shared compiler's actual expanded candidate, typed provenance
  and omissions, bound to its real containing source. It does not reinterpret expanded records
  through `ExpandedSceneCandidate::plain`.

## Independent Review Corrections

Anscombe found three concrete budget gaps; all were corrected before the final focused run.
Root input retention is reserved before compilation, including large unavailable records which
produce small previews. All held open snapshot inputs are also reserved, selected documents
transfer into composition ownership once, and temporary input reservation is released only after
the request drops its input snapshots. Independent root native receipts and metadata now count
retained bytes, encoded bytes, paths and unique files without a second parse. Final omissions
count Vec capacity and both owned ID strings' allocation overhead, plus typed provenance.

Two first-run fixture failures were corrected separately: independently reopened filesystem
capabilities have different session-scoped file identities even for the same physical files.
Tests now compare canonical keys/digests while preserving that distinction, and isolate a
foreign Schema basis using the original capability's receipt. Production identity checks were
not weakened to make the tests pass.

The first full root run (`6549febb-28d0-48fb-aac1-754ae8d79da0`) exposed two additional
failures: **499 passed / 2 failed / 3 ignored**. Mixed path/stable lookup of the same native
metadata compared the traversal depth as if it were part of the file receipt. The source owner
now compares complete file observations independently of traversal depth, for inclusion, catalogue
fencing and working-copy metadata checks; identity, parent, kind, path, digest and length remain
required. The existing external-Play receipt test exercises the real shared-sidecar route.
The exact snapshot retention fixture also compared an original Vec with spare capacity to
cloned Vecs without it. It now clones both stored copies consistently; production retention
continues accounting allocation capacity instead of equating value equality with equal storage.

## Preview Mechanism And Remaining Product Gap

Newton replaced the generic raw preview payload with explicit Authoring expanded-candidate,
TrustedCompiled and Rejected variants. Exact content stamps include closure generation and
canonical source/workspace document/full-revision participants. A rejected request retains
last-good materialization but cannot authorize that old graph under its rejected stamp.

These are qualified lower-level mechanisms. `EditorProjectSession::drive_editor_preview` still
uses the existing direct-only `authoring_preview::project_scene`, with empty dependency facts;
its constructor calls were adapted and rejection now uses the explicit rejected payload.
The actual captured closure, open-session selection and async worker coordinator are NOT wired
into that product path yet. Do not present the current direct preview as nested-preview completion.
The native presenter's `is_current` check also still relies on root revision/extent.

## Upstream Evidence And Policy

The Unity/Godot study now includes fixed Unity C# source at
`1c2d7db69c060103b6dc087af725442d1efaef9c` and pinned Godot dependency refresh. Unity Auto Save
performs an asset save; instance Apply is separate. Godot preserves old SceneState for local
differences when rebuilding dependencies. Neither inspected mechanism proves automatic
propagation of every unsaved open tab.

Nara's selected direction explicitly uses demanded open working copies for Editor authoring
preview. Preview refresh does not dirty/save its containing Scene or save dependent Prefabs.
External Play, Check and Export retain their own file-backed admission. The Editor must expose
unsaved dependencies and observe disk-to-open, close, Save and Reopen selection changes even
when the previous participant list or root revision is unchanged.

## Executed Verification

- Current Scene all features: **507 passed**, 3 existing ignored observation tests;
  run `418b9b9b-c25d-4115-9bc9-fabb185b2b0e`.
- Immutable session snapshot integration: **10 passed**, zero skipped;
  run `67644616-580e-4fcc-a905-2de595f13f51`, included in Scene507.
- Capture/retention/preview focused root cohort: **31 passed**, 473 filtered;
  run `d7d7ac8c-c9c1-43db-a765-774c090dd6a4`.
  This includes 11 actual indexed independent-document tests, 3 independent budget regressions
  and 17 preview-content tests. Earlier run `703f5793-c3ed-41d9-b0f3-9eec603bbbaf`
  had 26 passed / 2 fixture failures before the corrections.
- Final focused successor including the two full-suite regressions: **33 passed**, 471 filtered;
  run `a8184df5-a7dd-4152-b95f-0d23e43ac201`.
- Final complete root library all features: **501 passed**, 3 existing ignored observation tests;
  run `c692084f-97f3-47d7-8596-de7f21c45c0e`. This supersedes the 499/2 failed run above.
- Final `cargo +1.95.0 check -p nara --no-default-features --features tooling,project-workflow --lib --locked`
  and `cargo +1.95.0 check -p nara-editor --all-features --locked` both passed. These are
  compilation checks, not native execution. Existing dead-code/feature-specific warnings remain.
- `cargo +1.95.0 fmt --all` ran after source integration. Two changed design/research documents'
  **38** local link targets exist.
- Final local link validation covers **876** targets in five changed architecture/research/memory
  documents. `git diff --check` passes. Engineering rollup render/check/validation complete this
  continuity capture; they do not add runtime or native evidence.

# Next Action

1. Wire owned capture into the Editor's existing task facilities, with a consuming task envelope
   retaining document ID, original root revision and closure/selection generation. No filesystem
   traversal in the per-frame UI path. Retire/cancel superseded tasks through existing custody.
2. Capture current persistence Host receipts after accepted Save/Reopen; startup/indexed receipts
   alone become stale after file replacement. Retain canonical admission and metadata evidence.
3. Observe all demanded sources for open/close/reopen/Save/disk changes, including sources that
   were disk-backed in the previous capture. Fence root, selected participants, exact Schema and
   actual file evidence before publication. Never stamp an old result with the current revision.
4. Publish second-Scene assets through the existing Image ownership/identity path; expose
   unsaved dependencies; update native retained-pixel/input authorization to compare dependencies.
   The separate UI Canvas projection remains a real consumer, not implicitly qualified.
5. Complete source conflict observation/repair and convert-to-local through the source owner.
   The previously recorded aggregate descriptor-work gap also remains open.
6. Run independent/public consumer, Reference Game and appropriate native qualification before
   precise local commits and eventual local ff main. Do not push.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Unity/Godot study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Prior staged compiler checkpoint](2026-10-26T175942Z-staged-prefab-structural-demand-and-selected-value-program-qualification-19ce2446522e4c28a5fbd70d31ab827f.md)
