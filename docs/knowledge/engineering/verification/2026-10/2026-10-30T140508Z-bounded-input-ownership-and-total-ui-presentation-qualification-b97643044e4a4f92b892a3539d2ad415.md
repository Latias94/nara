---
type: "Verification Evidence"
title: "Bounded Input ownership and total UI presentation qualification"
description: "Qualifies finite ingress/member ownership, exact endpoint claims, total UI completion, explicit candidate queries and the real Reference desktop observation path."
timestamp: 2026-10-30T14:05:08Z
record_id: "b97643044e4a4f92b892a3539d2ad415"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Qualify the bounded Input endpoint/member replacement and its coupled UI presentation contract
under ADRs 0041/0102/0108. This is one complete foundation unit; the overall fearless rebaseline
Goal remains active. All Cargo runs are serial with Rust1.95, one build job and root target.

# Result

Finite Input-owned topology, one physical/synthetic ingress path, reserved terminal credits and
read-only projections replace historical endpoint maps and independent button mutation queues.
Accepted member identity survives release, context/rebind cancellation, paused staging, frame
copies and retirement. Ordinary refusal does not fabricate edges or evict accepted holds.

UI completion has a total committed/invalidated outcome. Exact endpoint claims and the live
interaction basis prevent late snapshots from reviving cancelled focus/capture/reducers or
affecting a same-label replacement. Successful C1 consumes candidates compiled against its old
basis. Candidate lookup is explicitly named; real consumers are migrated without aliases.
The changed ADR0102 inventory is refrozen for this scope after public/neutral/native qualification.

Independent review repaired the original four lifetime/completion findings and one additional
same-label restore focus leak. Its final limited recheck has no unresolved actionable P1/P2.
A separate consumer audit reviewed the real Reference production observation defect: post-commit
observation cannot require an unpresented candidate. Its Runtime/session/frame fence passes the
actual formal desktop native path, not only the component fixture or render probe.

## Changed Inventory And Assertions

| Surface | Qualified contract |
| --- | --- |
| `InputLimits` / `InputTimeline::with_limits` | Explicit finite event/endpoint/pointer/member storage, checked admission and bounded projections. Pools include the once-connected default synthetic endpoint/pointer. Physical `_at` ingress requires an explicitly connected endpoint. |
| Connection/retirement | Public managed helpers connect endpoints/pointers and retire pointers through the Input driver port; direct-App operations use the same owner. Terminal release/leave/control records convert reserved credits into retained records. Actual consumer completion permits recycling. |
| Button/action observations | `ButtonInput` and `PointerState` are observations. Direct button writers/port and parallel synthetic resolver are removed. Live `ActionOutcomes` is move-only without public queue writer/Default/Clone/Serde. Per-value recording remains. Admitted provenance has a sequence and Platform/Synthetic origin; release names its original press member. |
| Retention/contribution | Finite member identity and M*K links preserve aggregation/context/mapping ownership. Frame pins rearm when late events actually form next-frame copies. Cleanup follows those copies. Game View retains its first refusal while settling the real terminal tail. |
| Private `InputEndpointClaim` | Nominal original Input owner plus endpoint lifetime, independent of labels/route presence. It pins no retired mutable slot. Prepare/retry cannot adopt a replacement; old unbind/retire cannot affect it. Foreign owner refuses. |
| UI Surface association | Host connects logical Input before install. Geometry/suspension retain the claim; explicit handoff/restore captures the destination. A new same-label incarnation clears predecessor session/reducers; restoring the original lifetime preserves intended focus. |
| Presentation integration | `UiPresentationCommit::{Committed, Invalidated}` and Input/UI causes replace unconditional receipt return. Basis checks precede route publication. Invalidated completion consumes exact authority, preserves live state and publishes no new interactive observation/accessibility success. Backend pixel acceptance remains factual. |
| Candidate/presented data | `ui_candidate_frame` / `ui_candidate_frames` directly replace `latest_ui_frame(s)`. Accepted frames remain in the presentation owner's exact packet/receipt. Reference capture/resize inspect those facts; its production observer checks Runtime generation, current per-surface receipt and resolved frame revision. |

Owner tests cover historical churn, live saturation after log drain, exact/one-short admission,
terminal unions, generation/sequence exhaustion, paused/late-copy retention and same-label reuse.
UI tests cover ordinary release/activation, outside-content motion, accessibility focus, C1/C2,
invalidated completion/abort/defer and both restore policies. Independent public Input-to-Gameplay
tests observe actual commands and effects through 4,097 reuse cycles.

# Evidence

The final integrated source set has 52 changed/new Rust/Python/manifest/lock files relative to
`c624673`, including standalone smoke correction `305a664`. Its LF-normalized aggregate SHA-256 is
`3bdc2df0b0c44964b979a655cf1299c7b72bdcb30a68e98ffd3218fb3210051a`.
Each file hashes normalized bytes; sorted `<digest><two spaces><path><LF>` entries form the
aggregate. Documentation is outside this fingerprint. Earlier44/45/50-file freezes retain their
historical scopes. The last production Rust addition after the actual Reference native run is
Rustdoc only; final52 adds the smoke consumer/policy and that diagnostic-field documentation.

| Scope | Actual result | Nextest run |
| --- | --- | --- |
| Final Input, unchanged by later UI naming | 145 passed, zero skipped | `de613f61-6ff9-4007-a039-cdeb292eb00c` |
| Final UI with explicit candidate API | 121 passed, zero skipped | `4c2f4117-f123-42ba-9974-65f1d2d51790` |
| Independent source and bounded gameplay workflows | 8 passed | `b64ee0b4-99c6-4945-8c1c-31209d1ec7de` |
| Prebuilt processed Player after source removal | One test, seven actual child cases, passed | `6bb5e378-33f6-4099-a3b5-19a74838b376` |
| Broad root library/four integrations before consumer corrections | 1,089 passed, 4 failed, 4 ignored; 1,093 executed | `c9d895c6-2060-464a-b26c-5b1f7f627e46` |
| Corrected affected root UI/GameView/feature boundary before mechanical rename | 28 passed | `b7a239d7-02ae-4deb-ab33-fb06bcb6cd0d` |
| Broad Reference Editor-feature consumer | 152 passed, 7 failed; 159 executed | `5a583d81-b7eb-433c-82c0-c37977d11cfa` |
| Final affected Reference UI/authoring/performance binaries | 12 passed | `ebe02aa8-2505-43e2-bbad-09319d047712` |
| Artifact/smoke policy, including independent projection IDs and wrong-source refusal | 11 passed | `7444955c-7246-4c39-a54e-52b8d8065e58` |
| Final selected Windows/DX12 Editor trajectories | 4 passed, 282 unselected | `095648ff-3769-4fdb-afff-3325b31cec7a` |

The four root failures were an obsolete post-commit candidate read, two unsynchronized independent
presentation locks and a feature tree omitting the adopted typed Image CPU dependency. The
passing28 successor includes every failed case. Reference's seven assertions had the same
candidate/presented fixture assumption; its12-test successor preserves layout, resize, activation
and authored-edit checks. These are scoped successors, not fabricated single green broad runs.
Prior owning405/4-ignored and intermediate results remain in the linked progress records.

The four final actual native tests are:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit`

Additional final gates:

- Workspace all-feature/all-target correctness Clippy passed after API replacement in1m08s;
  existing suggestion warnings remain. Its pre-rename3m34 run keeps the original scope.
- Reference `--features editor --all-targets` check passed in2m38s. The desktop binary was built
  into root target; final affected Reference tests passed in7.073s after1m15 build.
- Minimal Input and root runtime-ui checks passed again after the candidate rename.
- Actual layout_gallery, interaction_lab and headless ui_presentation_gallery passed again with
  the new API. The last proves retained solid/image/glyph packets, receipt/action and retirement;
  it is not another native-window result.
- Formatting checks for root, Reference, module-consumer and renamed Runtime-runner workspaces
  passed. Final diff/document/memory checks and precise staging accompany the local source commit.

## Formal Reference Native Observation

The actual `target/debug/desktop.exe --candidate-smoke` runs with DX12,
`NARA_REFERENCE_UI_OBSERVATION=1`, isolated process home/cwd/temp and no stdin protocol. Its formal
path is Render -> Cleanup successful UI commit/consume candidate -> Last product observer. It
exits zero, emits UI/GPU records and `desktop_candidate_smoke: ok`. Existing smoke validators
accept the captured UI, GPU completion window and their exact identity correlation.

The local log is
`target/review/bounded-input-native-7301000d359343e5a6632db0094b0206/desktop-stdout.log` with an
adjacent verification receipt; it is not committed. The first runner inherited a closed parent
stdin pipe and was refused during bootstrap inspection. Standalone smoke now binds DEVNULL;
the engine's refusal remains unchanged. The next native process completed normally but exposed
the old oracle's hardcoded expanded ID. Source document/entity and control facts are the authoring
comparison; expanded Scene IDs remain diagnostic, not a cross-Host allocation contract. Captured
native records pass the corrected validator. The policy regression uses distinct headless/desktop
projection IDs and still rejects different authored sources. No native rerun is invented for this
validation-only correction.

This is a source-based development native run. It does not claim a new packaged archive,
source-free release, no-checkout consumer or hosted platform release.

# Follow-up

Native evidence is Windows/DX12. The field case constructs Winit IME events; it does not qualify
OS candidate delivery/placement, full custom Editor text, monitor scanout or Linux/macOS. The
Reference smoke is a positive production observation path; foreign/stale/retired refusal was
independently source-reviewed, not represented as extra native cases.

The finite Input proof covers selected raw-adapter keyboard/mouse and existing UI navigation.
Asynchronous event-origin recovery, seat-scoped maps, gamepad/axis holds, replay, allocator
isolation and P99 performance remain separate. Caller-owned captures are outside live Input
accounting. Default finite capacities are not a measured throughput profile. Immutable source/
processed definitions retain preceding qualification.

Continue the complete foundation Goal with valid-reference versus current load-demand semantics
before target/package delivery freezes the Scene dependency model. Immutable backing/residency
is separate. Schema/claim evolution, joint motion/executor selection, render graph, editable
Nara-drawn Editor and other delivery waves remain open. The same-label focus lesson is carried
into the first text-composition lifetime/cancellation design in R11.

# Citations

- [Input owner plan](../../../../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR0041](../../../../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
- [ADR0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- [ADR0108](../../../../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Last API/consumer progress](../../progress/2026-10/2026-10-30T133904Z-explicit-ui-candidate-api-and-runtime-bound-reference-presentation-observation-4ac94530888142ae8d956f4172287842.md)
