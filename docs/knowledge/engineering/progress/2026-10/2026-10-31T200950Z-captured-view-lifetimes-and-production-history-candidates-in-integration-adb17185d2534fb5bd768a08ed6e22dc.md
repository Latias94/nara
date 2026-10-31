---
type: "Work Progress"
title: "Captured View lifetimes and production history candidates in integration"
description: "Neutral View ownership verified; original logical declarations, move-owned candidates and complete Host history publication are integrating."
timestamp: 2026-10-31T20:09:50Z
record_id: "adb17185d2534fb5bd768a08ed6e22dc"
producer_id: "codex-foundation-main"
run_id: "render-history-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "9f79d51"
---

# Summary

Continue the full active foundation Goal. `main` and `refactor/foundation` were locally fast-forwarded
to `9f79d51` after typed-policy production commit `6e16c7c`. The current View/history replacement is
uncommitted integration work; it is not complete, independently qualified or ready to merge. Preserve
untracked `vendor/`; do not push/PR or update reference checkouts. Shared root target, serial Cargo,
`CARGO_BUILD_JOBS=1`, Rust1.95.0 and serial nextest remain mandatory.

# Details

- ADRs0040/0094/0117 reconcile original temporal lineage, move-owned candidates, explicit View
  lifetimes and atomic output/history publication. Full0121 remains Proposed.
- `RenderViewId` is runtime-only, with non-reused roots and code-owned subviews. Camera membership
  is observed in the same exclusive call as extraction, independent of target/projection/default
  query visibility. Persistent Camera fields/schema/hooks are unchanged. Duplicate IDs reject.
- `WgpuFeatureDefinition::declare` creates an original logical incarnation. Each policy `bind`
  resolves capture declarations into fresh immutable `Bindings` holding actual Arc instance owners.
  Weak unpublished reservations let sibling candidates reuse exact actual owners; complete
  publication updates cached memberships. Existing Feature Drop-close semantics remain.
- New private `history/state.rs` stages complete successors, pins empty/reset facts, checks a global
  lineage revision, commits exact key/output sets and retains successful capture watermarks across
  reset/retire. All old rows escape in retirement custody after coherent pointer updates.
- Public lineage, texture/buffer slot tokens and output bindings live in `history.rs`. A lineage
  fixes its source clock; its realization and per-slot declaration Weak leases permit reclamation.
  New Host realization explicitly reports temporal `DeviceRecreated`, without claiming old temporal
  pixel reconstruction. Immutable CPU captures remain the non-temporal reconstruction authority.
- `WgpuRenderCandidate` owns original input, exact program/bindings/basis, target/Host/epoch facts and
  conservative pending count/metadata guards. `prepare` pins once; `submit_candidate` consumes on
  publication or returns that same candidate. `WgpuPublishedFrame` retains the full output group and
  supports exact typed history lookup; ordinary `submit` remains a one-shot image convenience.
  The display retry variant returns both candidate and original UI sidecar. No native reservation
  or acquired surface escapes preparation; existing serial native transaction publishes all state.
- Global history metadata reserves full live/reset successor/lease/scratch capacity before creating
  a realization. Candidate metadata has its own conservative65MiB envelope and active-count budget.
  Temporary candidate pressure differs from deterministic history declaration capacity.

Actual results at capture time:

| Check | Result |
| --- | --- |
| Neutral Render all-features |48/48 passed, run `55fa281e-f44d-4be4-90dd-e6926ec70486`,3.993s. |
| Sprite Render all-features library |35/35 passed, run `19c6dc07-fe0c-4299-ad6c-33ed26dc25cd`,3.145s. |
| History state and policy binding tests |42 passed /356 skipped, run `78c6691b-2469-4956-aeb0-b67870a87267`,5.402s. |
| WGPU all-feature library |Passed after history/published-frame integration; no-default/wasm remains pending. |
| Root all-feature library |Passed22.75s, before final publication-result helper and metadata-maintenance refinements. |
| Actual DX12 history Host |2 passed /398 skipped, run `dee1536f-6f7d-4a34-9a4b-99dcb8cbc1f0`,1.858s. Public budget refusal/same candidate retry, sibling/reset rejection, real submit failure without advancement and actual loss/new-Host reset. |
| Complete WGPU ordinary owner + public UI |Live exec55186, log `nara-render-history-all-owner-tests.log`; no terminal result claimed. |

The initial interim check intentionally sampled incomplete policy wiring and found missing new
fields/exports; it was not a final snapshot. Final owner checking found and fixed one immutable/
mutable policy borrow overlap. The first focused test build found a removed test-only lease accessor;
its pointer assertions now use the existing Weak identity API. The first native fixture build used
two wrong existing API names; corrected Attachment and render_transaction_frame_index match owning
APIs. Production admission was not weakened to repair fixtures.

Kuhn's read-only neutral lifecycle review found no P1/P2, including same-Entity remove/readd,
disabling filters, exclusive deferred boundaries and publication preservation. It is source evidence;
the actual48-test result above owns execution qualification. Mill, Volta and Hubble are source-ready
and closed. Descartes is finishing consumer migration. Peirce writes new independent accumulation
consumer files. Laplace reviews actual candidate/history production read-only. No agent may run
Cargo/fmt/commit; parent owns integration and serial verification.

# Next Action

1. Poll exec55186 before any further Cargo. Fix actual ordinary owner regressions, keeping exact
   field/metadata/panic and UI sidecar oracles. No full fmt while the external consumer writer runs.
2. Collect Descartes final consumer source-ready. External policy now uses Bindings/context/declarations;
   old InstanceContext/Instances/create_instances aliases are removed. Qualify neutral/manual image
   helpers and stale accessors after these signature changes.
3. Collect Laplace concrete review findings and owning corrections. Check post-publication destructor
   behavior, original declaration lease reclamation, source/target ABA and exact metadata before
   claiming the whole transaction qualified.
4. Integrate Peirce's new standalone two-View/four-accumulator consumer. Use actual float readback and
   independent CPU oracle, reordered declarations, one effect replacement, stale/reset siblings,
   public budget pressure, A close/B progress and retained outputs. It has no result yet.
5. Complete actual Runtime/Game/Scene/Canvas temporal workflow as required; first native2 only uses
   initialized history outputs, not numerical accumulation. Run affected root/Editor/native scopes,
   minimal+wasm, correctness Clippy, final formatting and independent review. Record limitations,
   precisely commit and locally ff main only after the complete selected unit passes.

The full render unit still includes independent streams, actual two-window/image composition,
source/processed interleaving, retained exact display/UI/Input join and self-drawn editable Inspector.
Do not shrink the foundation Goal to this unit or treat Accepted design as implementation evidence.

# Citations

- `docs/architecture/render-program-production-design.md`
- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md`
- Predecessor qualification `df7f9377f51142b78060b53617c800a1`; previous registration
  `5f44bbe538f14c67aaaa5b8d5d5422f2`.
