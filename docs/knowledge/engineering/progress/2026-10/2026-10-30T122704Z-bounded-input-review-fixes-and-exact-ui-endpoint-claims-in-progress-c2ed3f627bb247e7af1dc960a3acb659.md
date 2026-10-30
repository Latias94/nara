---
type: "Work Progress"
title: "Bounded Input review fixes and exact UI endpoint claims in progress"
description: "Independent review corrections, current owning test evidence, and active UI connection-claim integration; replacement remains uncommitted."
timestamp: 2026-10-30T12:27:04Z
record_id: "c2ed3f627bb247e7af1dc960a3acb659"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md"
git_branch: "refactor/foundation"
git_commit: "c624673"
---

# Summary

Continue the complete active foundation Goal. Local `main = refactor/foundation = c624673` before
this uncommitted production replacement. This successor follows the
[first Input integration record](2026-10-30T113920Z-bounded-input-owner-replacement-and-prepared-ui-integration-in-progress-ecdeb5811f044c95a0653ac744b58002.md).
Do not undo the large authorized changes, read/change `vendor/`, update reference checkouts,
run parallel Cargo, push or infer whole-Goal completion.

# Details

## Additional Implementation And Review Corrections

The parent added a real public `retire_input_pointer` driver helper, removed the remaining
unbounded `ActionOutcomes` writer and live-resource Default/Clone/Serde, and retained per-value
observation Clone/Serde. Semantic apply now consumes the actual admitted `InputTimelineEntry`
rather than separately supplied member/input/phase/provenance facts. The current Input state
allowance is the single canonical log maximum plus eight snapshot-equivalent units; Raman's
read-only lifetime analysis confirmed held actions retain at most one current mapping and one
identity per admitted context, not one new identity allocation per contribution. Tree allocations
remain bounded by actual M*K links, with row deletion on last release/cancellation.

Epicurus added total prepared UI completion with Input, InteractionChanged and SurfaceUnavailable
causes. The UI interaction basis is captured before submit and checked before Input route commit;
ordinary release/activation, outside-content cancellation and accessibility changes cannot be
overwritten by a late prepared snapshot. A newer unpresented authored candidate alone does not
invalidate an outstanding presentation. Successful presentation now discards candidates compiled
against the previous live interaction basis, preventing C1/C2 basis laundering. Neutral presenters
and root publication tails use the total outcome directly; no compatibility receipt alias.

Russell's independent review found four concrete issues beyond the initial passing suite:

1. Late Update events lost frame pins at cleanup before next-frame copies existed. The parent
   rearms pins when projection, observation or delivered action copies actually form. An actual
   Update -> next First/PreUpdate -> fixed Prepare Runtime regression passes.
2. Game View batch short-circuit dropped real terminal events after ordinary refusal. Sartre
   preserves the first error, skips later ordinary activity and settles the original admitted
   batch's release/leave/focus-loss/retirement tail; post-scope fault keeps precedence. Three actual
   Runtime/Input regressions pass. This is truthful partial success, not an atomic batch promise.
3. UI C2 could adopt a new live basis after C1 committed and restore old capture. The successful
   commit discards that old candidate; Epicurus added the exact action-A/B/C1/C2/release test.
4. UI retirement failed after physical endpoint reclamation, and a suspended Surface with no
   active stamp could retire a same-label replacement. The parent initially added absent-endpoint
   idempotence, then selected a complete identity fix instead of a None/stamp workaround.

## Exact Input/UI Endpoint Association — Active Integration

Input now issues `__private::InputEndpointClaim`: original Input owner identity, private non-reused
EndpointId and raw labels. It retains no mutable slot. Internal functions are:

```text
claim_input_endpoint(&InputTimeline, window, seat) -> Result<InputEndpointClaim, InputIngressError>
prepare_claimed_route(&mut InputTimeline, &claim, InputPresentedRoute) -> Result<PreparedInputRoutePublication, InputIngressError>
unbind_claimed_endpoint(&mut InputTimeline, &claim, Option<InputRouteStamp>) -> Result<(), InputIngressError>
retire_claimed_endpoint(&mut InputTimeline, &claim, Option<InputRouteStamp>) -> Result<InputEndpointRetirement, InputIngressError>
```

Foreign owner refuses. Original incarnation absent/different means coordinated cleanup is already
settled and must not touch the replacement. Claimed prepare refuses an expired incarnation, so
an old active Surface cannot publish onto a newly looked-up connection. Same original connection
still checks conflicting live routes. Raw native ingress remains unchanged; this is not a public
OS event-generation handle and does not recover missing event origin.

UI captures the claim at successful Surface installation, keeps it through geometry/suspension,
uses the old claim for unbind/retire and actual destination claim for explicit handoff/restore.
Hosts must explicitly connect their logical Input endpoint before installing a Surface. This can
precede native creation: first-party Product composition connects its configured logical primary
endpoint and pointer; Winit's later connect is idempotent for that same connection. It does not
reconnect after retirement per callback. Parent migrated `src/product/ui_surface.rs`, root bridge
tests, both gallery modes/recovery, product-default fixture and WGPU runtime measurement. Epicurus
owns remaining `nara_ui` fixture/canvas/source-identity/interaction-lab connections and exact
claim regression tests. Their current source may still be transitional; collect source-ready.

ADRs 0041/0102/0108 and the endpoint plan reconcile late-copy retention, terminal batch tails,
UI basis and original endpoint-claim semantics. Input guide documents finite limits and public
connection/retirement. No claim of final qualification/refreeze/commit has been made.

## Executed Evidence Since Predecessor

These runs are serial, reuse root target, use Rust1.95/CARGO_BUILD_JOBS=1, and have exact scopes.

| Run | Result and coverage limit |
| --- | --- |
| `c9c93b62-6e6b-4f5f-9f3f-690be95c527f` | Independent source Input fixture 5/5 passed; 1m04s build, 0.508s run. Before private claim integration. |
| `133405f1-1794-41dd-a298-8019c1d865ed` | New independent public bounded Input-to-Gameplay workflow 3/3 passed, including 4,097 keyboard/pointer endpoint reuse cycles, physical/synthetic aggregation, saturation refusal, contexts, rebind, pause/exact-step and actual commands. 47.14s build, 2.153s run. |
| `253afcab-2e20-458e-97ca-7857813e5a07` | Root no-default selected processed Input/GameView owners 20/20 passed, 973 unselected. Initial build found one remaining test-only ButtonInput writer, migrated to timeline before run. |
| `4b4d7cc3-4043-4479-bf44-2b9f15b84134` | UI prepared-loss/interaction basis nine tests passed; before C1/C2 and endpoint-claim successor. |
| `87e7f09e-154a-41ed-afcf-885f1b0bc1b0` | Broad Input/UI/render 292 pass / 3 old-contract fixture failures. Migrated source-isolated release, retiring-label refusal and reserved-terminal capacity faithfully, without raising limits. |
| `555215a8-f1af-491f-bf82-2cd5c04f2be1` | Broad owning successor 295/295 passed, zero skipped, 41.37s build/12.754s run. Precedes Russell's four corrections and endpoint claims. |
| `cf3d067b-e64e-4f3d-8856-e6c0ed0a506d` | Root all-feature selected UI bridge/GameView/processed Input 44/44 passed, 1,017 unselected, 1m58s build/5.790s run. Includes held backend pixel acceptance without observation/accessibility success. Pre-claim and pre-C1/C2. |
| `6870a803-3a6b-4e95-aed0-0393788c4848` | Late frame custody Runtime regression 1/1 passed, 5.88s build/0.035s run. |
| `477325dd-bd6c-42fc-bed4-15a5423b0f4a` | Full Input after frame-custody correction 143/143 passed, zero skipped, 14.27s build/4.981s run; before latest private claims/semantic signature. |
| `1940793d-086e-46fd-ab18-e56ad3a77910` | Two original endpoint-claim tests plus late-frame custody 3/3 passed, 7s build/0.090s run; before the claimed-prepare extension. |
| `d10dad34-2d3a-43d4-99ab-0d68d393983e` | Actual Game View terminal-tail tests 3/3 passed, 993 unselected, 1m32s build/0.358s run. |

Root all-feature lib check passed in 10.49s before claim integration. Input/Gameplay all-feature,
all-target correctness Clippy passed in 12.81s with advisory warnings; parent simplified the
semantic signature and two boolean/if warnings afterward. Input no-default lib check passed
in 1.34s after that simplification. No new actual native/DX12, Reference Game, prebuilt processed
child process, full workspace/all-target or final neutral examples have run for this replacement.
Do not replace these limits with a broad green claim. TEMP logs use `nara-bounded-input-*` names.

## Live Work Ownership

- Epicurus `01a18c7b-fc5e-7a90-8ad8-d92e64ce068a`: active UI claim integration, existing runtime/state,
  test helper, prepared-loss tests and assigned manual/reference consumers. Also `nara_ui` Canvas,
  source-identity and interaction-lab fixtures for explicit pre-install connection. No Cargo/fmt.
- Sartre `01a18c81-c4ca-70f0-9c8e-1570f9188a05`: all native/GameView/source/processed migrations and
  terminal-tail fix frozen; no current writes.
- Russell `01a18ca7-d9bf-7e61-ade3-1e7247d28282`: independent read-only correctness review; all four
  findings accepted, current exact-claim follow-up waits final UI source. No writes/Cargo.
- Raman and Pasteur are completed/closed. Pasteur's NEW
  `tests/fixtures/module-consumer/tests/bounded_input_workflow.rs` passed actual three tests above.
- Parent owns Input core/types/semantic integration, root UI bridge, Product primary Surface,
  external fixture changes, docs/memory and all serial Cargo.

# Next Action

No known Nara Cargo is running at this record's capture; previous actual sessions58306 (Clippy)
and71786 (root batch tests) are terminal0. Other projects run Cargo on this machine; do not kill them.
Collect Epicurus final source-ready, run focused claim/UI prepared tests, fix real failures, freeze
writers before global fmt, then full Input/UI/render owners and actual root/independent/neutral/
Reference/native checks. Preserve old tests' useful behavior, remove stale helpers/aliases and
keep all changed public ownership contracts qualified together. The processed-source-free fixture
is `tests/fixtures/runtime-runner/renamed-root/Cargo.toml`; its prebuilt process test must use root
target and serial Cargo. Final complete owner/API review, actual evidence and precise commit/ff-main
remain outstanding. Full foundation Goal stays active afterward.

# Citations

- [Endpoint plan](../../../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [UI ownership ADR](../../../../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md)
