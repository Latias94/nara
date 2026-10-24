---
type: "Verification Evidence"
title: "Complete persistent UI identity and neutral conformance successor"
description: "Qualified Runtime and Canvas complete-key adoption with exact public inventory, neutral assertions, independent consumers and Windows DX12 evidence."
timestamp: 2026-10-24T18:55:35Z
record_id: "0cf84bdfd2f54766be84f3ca206f7931"
status: "qualified-scoped-successor"
producer_id: "codex"
run_id: "session-01a1629b-ui-complete-key"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "f7ab300c30aeeeb763292c05394f97082c097ce8"
supersedes: "a628828a8ff941239d79a0b8b3d60316"
---

# Verification

Implementation `f7ab300c30aeeeb763292c05394f97082c097ce8` is the tested replacement. The
LF-normalized manifest for its 21 changed production/regression Rust files is SHA-256
`ec6e0fe8627deff7b75b676e22c27980b35f6e392d4e50ebfa08b357cd08827c`.
Select `.rs` paths from `a3c6674..f7ab300`, excluding `docs/benchmarks/`, sort them, hash each
LF-normalized file, then hash the UTF-8 lines `<digest><two spaces><path><LF>`. The staged blobs
were checked against this manifest before commit; no source changed after qualification.

All Cargo work used Rust 1.95.0, the shared `target` and `CARGO_BUILD_JOBS=1`, serially:

| Scope | Actual command selection | Result and nextest run |
| --- | --- | --- |
| UI owners | `nextest run -p nara_ui_kernel -p nara_ui_widgets -p nara_ui -p nara_ui_render --all-features --locked -j 1 --no-fail-fast` | 230 passed; `9f174762-a9e4-44fa-9989-9a8205cdbd3c` |
| Independent consumer | `nextest run --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --target-dir target --locked -j 1` | 4 passed; `882de4d0-8440-4c94-b363-38be2b3c52b6` |
| Reference Game | `nextest run --manifest-path reference-game/Cargo.toml --target-dir target --locked -j 1` | 87 passed; `4247171c-a075-472c-9ab8-8bbf14ce6ec1` |
| Saved UI refresh | Same Reference manifest, explicitly `--features editor --test editor_ui_refresh` | 1 passed; `c184a354-8843-4efa-bcec-ecb9da392718` |
| Root library | `nextest run -p nara --all-features --locked --lib -j 1 --no-fail-fast` | 398 passed, 3 skipped; `be7d6b2c-64db-4815-990e-2cccff155261` |
| Actual native Editor | `nextest run -p nara-editor --lib --locked --run-ignored only --test-threads=1`, `WGPU_BACKEND=dx12`, exact filter below | 10 passed, 198 unselected; `4c143406-6078-490c-8cee-3703795014b7` |

Workspace all-feature/all-target `check --locked -j 1` passes. The four UI crates pass
all-feature/all-target Clippy with `-D clippy::correctness`; existing non-blocking warnings remain.
Their all-feature doctest command succeeds with zero executable doctests, not extra behavioral
coverage. Root, independent-consumer and Reference Game `cargo fmt --all -- --check` pass.
`git diff --check` passes. The implementation-side local-link check found 807 targets, none missing.
The documentation closure checks 988 local targets without a missing file; memory render/check
and validation pass with existing legacy advisories and no warning naming either new shard.

The exact native selection is:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `an_idle_canvas_reopens_without_unrelated_input_or_forced_frames`
- `an_unconfirmed_canvas_redisplay_cannot_grant_picking`
- `the_workspace_checkbox_reopens_an_idle_canvas_using_only_host_deadlines`
- `unpresented_canvas_keeps_its_submitted_image_without_granting_picking`
- `native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`

# Result

This is the focused ADR 0102 successor for complete persistent UI identity, not completion of
the UI toolkit, R5, a delivery wave or the foundation goal. It supersedes the predecessor shared
inventory where this replacement changes it; historical assertions are not silently re-executed
or declared unchanged. Ordinary Scene preparation and source ownership keep their separate evidence.

## Exact Interface Inventory

- Kernel exports `MAX_UI_KEY_BYTES` and validated textual `UiKeyAtom`. `UiOpaqueId` is now a bounded
  binary value, with `new(impl AsRef<[u8]>) -> Result<Self, UiKeyError>` and `as_bytes() -> &[u8]`.
- `UiPersistentIdentity::new(context, placement) -> Self` requires two complete `UiOpaqueId`
  coordinates; `context()` and `placement()` expose shared values. The old `local`, `projected`,
  `source_entity`, `projection`, `UiProjectionPath` and `UiProjectionStep` interfaces are removed.
- `UiRootKey/UiLocalKey::{persistent,runtime}` and `UiKey::{root,scoped,root_key,local_key}` remain.
  `UiKey::encoded_len()` is public. Independently constructed equal values have equal Eq/Ord/Hash
  and cost; extracting/recomposing a local does not inherit missing context from its root.
- Canonical root-only cost is `3 + R`; scoped cost is `7 + R + L - P`. Complete address lengths
  include their tags and two-byte field lengths; `P` is the content-defined common prefix. Final
  field length is trailing. Constructors, ready/rejected root envelopes and reduced limits agree.
  The hard ceiling remains 256 bytes, but eligibility changes, including runtime textual keys.
  This is not a stable wire format. Action/image/consumer/runtime text retains control rejection.
  `UiKeyError::ProjectionDepthExceeded` remains available for adapter admission.
- `UiLimits::max_roots()` exposes admission capacity. Runtime refuses excess roots before creating
  another identity/context. Node admission and independent presentation rejection remain intact.
- Runtime exposes no new public Scene encoder. Its private lowering uses canonical source kind,
  origin and raw UUID/text, explicit occurrence namespace/slot and complete ordered Prefab placement.
  `SceneEntitySource` wins over `UiRuntimeKey`; an invalid persistent address never falls back.
- Canvas `UiAuthoringCanvasCompiler::compile` requires `&SourceDocumentKey` and
  `&SceneOccurrenceKey`. It stays under `nara_ui::__private`, not a newly supported Editor SDK.
  The Host uses the admitted Prefab persistence lane and explicit `nara.editor` / `ui-canvas`
  occurrence. Its cache checks source/occurrence before reuse, alongside all existing fences.
- Ordinary `nara_ui::prelude` is authoring/action consumption. Surface lifecycle, exact receipt
  and dispatch operations belong to `advanced`. The historical UIF-U5 prelude inventory is not the
  current placement. First-party presenter/managed-driver glue remains doc-hidden and local to
  conformance programs; it is not a forgeable public presenter or an independent-consumer export.

## Named Assertions And Consumer Boundaries

`nara_ui_kernel/tests/identity.rs` proves independent construction/sharing/recomposition value
semantics, raw bytes, exact 255/256/257 costs, reduced limits and variable placement prefix
retention. Root-envelope tests include absent bodies. The new `failure_atomic` case first permits
same-context Modal fallback, then rejects equal local placement under another source or slot
context without consuming revision or destroying the original cache.

The public `source_identity` consumer produces two roots for different local IDs, equal local IDs
in distinct sources, and equal local IDs in two occurrences of one source. Its three-level repeated/
diamond Prefab case has two roots and four keyed layouts: lookup alias reconstruction preserves
keys, changes activation, and a containing-source switch changes keys. This uses real provenance.

The mixed Runtime interaction case has four roots and four complete button keys, including a
foreign Scene child reparented under a persistent root and both persistent/runtime mixing directions.
Each press checks the complete dynamic semantic-key set, exclusive focus/press and ordered action
targets. The first press also recompiles: only that target's reducer rectangle changes to the
pressed color. Remaining targets are checked for unchanged paint, not separately recompiled in
every press permutation.

The persistent rebuild case separates three authorities: compatible reconstruction preserves
logical keys, while the old Scene runtime lookup no longer resolves as its live predecessor;
an unpresented changed-action candidate cannot replace the old receipt's behavior. An intervening
input invalidates that candidate (`FrameNotLatest`), so the test recompiles before accepting it.
A source switch rejects the old target through the new receipt, and explicit retirement rejects
the old route with `RetiredSurface`. Rebuild alone is not blanket receipt invalidation.

Canvas owner tests establish compatible rebuild, source/slot separation and equal-coordinate
Runtime/Canvas keyed-layout parity. The Host cache test holds document, revision, gesture, profile
and image receipt constant; only source changes, and the invalid new projection must clear the old
candidate and geometry rather than hit the prior source's cache. It is an owner-level cache proof,
not a claim that this exact artificial source-only transition is a new native workflow.

The independent fixture has only text/kernel/widgets dependencies. Its fourth test supplies its
own binary context vocabulary, compiles two roots/buttons, checks distinct keys plus focus/actions,
and recomposes the extracted root/local handles. It does not interpret Nara Scene bytes, freshly
regenerate those cloned handles, dispatch an activation or manufacture backend submission authority.

All three neutral executables run successfully through `cargo run --locked -j 1 --all-features`:

- `layout_gallery` (`nara_ui_kernel`): 3 roots / 11 layouts / 16 paint / 7 semantics at scales
  1.0 and 1.25, including current padded content geometry and retained inert Modal envelope.
  These are the current assertions, not the older UIF-U5 output counts.
- `interaction_lab` (`nara_ui`): managed Scene plus runtime authoring, two surfaces, capture/focus/
  Modal routing, disabled-focus continuity, exact presented behavior/accessibility and three paused
  cursor actions followed by acknowledgement reclamation. Scene setup now runs at `StartupStage::Scene`
  with the actual frozen managed definition; complete entity handles publish afterward.
- `ui_presentation_gallery` (`nara`): owned solid/image/glyph packet and exact target/frame receipt,
  accessibility action, resizing and readable retired packet. Current slot/route/geometry/presentation
  generations are `1/1/1/1 -> 2/2/2/2 -> 3/3/3/3`, not the historical `1/1 -> 2/2 -> 3/2` statement.
  This runtime-key example is a presentation regression, not another canonical Scene encoder proof.

# Evidence

Earlier proof-first evidence in this unit includes the clean public equal-root counterexample
(`49a7925d-2fe0-48a0-8935-28d47a79dc94`), three complete-root budget refusals before their fix
(`f42e9bf1-d3ce-4444-ab58-b6d27d528697`) and the variable-placement/real-Prefab prefix failures
(`2220a833-bbb5-4141-b591-6e9f25c107f0`). They are not new executions of the final source.
The final owner suite covers the accepted behavior. The neutral interaction reproduction also
failed at `scene.prepared-definition-changed` before migrating its lifecycle. Its Windows fail-fast
after the Rust assertion is not independent proof of a stack-overflow or memory-safety defect.

The first new rebuild test attempted to present a candidate after input correctly invalidated it:
`454f5d2c-4898-4c2f-a3b3-c1bc4060d89f` failed with `FrameNotLatest`. The corrected fixture explicitly
asserts that rejection and refreshes without advancing its controlled source replacement. Final
targeted3 run `6022a22c-b837-42f8-8d4b-a325eeaf425c` and cache1 run
`6afa3c9f-948b-40d2-8553-e373090e2fa8` pass; broader final runs are listed above.

The committed [storage comparison](../../../../benchmarks/2026-10-25-ui-key-storage.md) includes
three independent processes and LF fixture SHA-256
`934fc875ef90a49a9ec47dffbdc579f60074a3114b668cfe7cc9f523f5e1c356`, checked against its summary.
Typed shared context wins the model's common-context case; segmented storage wins one long-tail
post-snapshot case but adds validation allocations. Neither wins universally. These are System
allocation requests from a standalone model, not engine frame time, allocator usable size or RSS.

## Review Disposition

Read-only source review `01a16f28-22e5-7000-85b3-0d7d062206e0` found no new encoding/collision issue.
Conformance audit `01a16f28-5017-7730-8cba-b64b46151db7` identified stale present-tense design text,
historical inventory/assertion drift and missing focused fallback/cache/reducer/receipt evidence;
the current docs and assertions resolve those bounded findings.

Five simplification lenses found duplicate cache lookup/context clones, temporary canonical-source
Arcs, cache-hit source copies, redundant comments and a potentially vacuous semantic loop. The
implementation uses `Entry`, temporary byte buffers and borrowed cache comparison, removes three
redundant comment blocks, and checks the complete semantic-key set. No proof test was deleted or
weakened. The non-obvious trailing-length comment remains. The P3 suggestion to group already-large
private traversal signatures into another context object is not adopted: it is optional maintenance,
not required by this workflow or a demonstrated ownership defect; keep it visible for a later
traversal change rather than adding a wrapper solely for argument count.

Fresh integration review `01a16f38-c6d9-7d12-88d8-d9f38b71c392` checked the final 21-file hash at
start/end and found no supported new defect. These agents did not run Cargo; their static evidence
is separate from the main owner's actual executions. All review handles are closed.

Code review: skipped (ce-code-review unavailable)

The fresh actual portable invocation `01a16f34-739c-7081-9c2a-8d0a20adaa7d` terminated `failed`
after two unsuccessful discoveries of nested Codex launch/terminal collection tools. It was closed;
no completed portable receipt is claimed. The main owner separately scanned the final diff and
staged source manifest and found no blocking issue. The scoped independent reviews are not
substituted for that missing portable receipt.

# Follow-up

The complete foundation goal remains active. This checkpoint does not implement declarative
Prefab overlays, Behavior/module representation, activity scopes/physical close redesign,
resource delivery, composable rendering or the remaining R2/R11 work. Imported multi-product
identity, automatic rename continuity, arbitrary 32-step key admission, editable text, full
toolkit convergence, OS accessibility/IME parity and Linux/macOS native support remain unqualified.
UI admission failure does not roll back successfully published Scenes. No migration guide,
compatibility alias, runtime epoch reset, remote publication or vendor change is introduced.

Next R2 work must remove caller-owned entity-allocation/template choreography through the
existing prepared edit owner, not introduce another document store or Undo executor. The separate
[construction survey](../../subagents/2026-10/2026-10-24T185535Z-r2-transaction-local-construction-ownership-and-allocation-alternatives-a12471c958dc470097f9b8c7337c0533.md)
records alternatives and allocator decisions that remain unadopted.

# Citations

- [Source and occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [ADR 0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Native scope](../../../../verification/native-editor-qualification.md)
- [Independent consumer](../../../../../tests/fixtures/ui-module-consumer/README.md)
