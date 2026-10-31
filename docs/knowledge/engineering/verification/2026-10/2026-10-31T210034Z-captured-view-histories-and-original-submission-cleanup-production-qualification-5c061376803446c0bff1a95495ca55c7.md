---
type: "Verification Evidence"
title: "Captured View histories and original submission cleanup production qualification"
description: "Scoped production history candidates, independent numeric and Runtime consumers, and final submitted-custody cleanup evidence."
timestamp: 2026-10-31T21:00:34Z
record_id: "5c061376803446c0bff1a95495ca55c7"
producer_id: "codex-foundation-main"
run_id: "render-history-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "9f79d51"
---

# Verification

Scope: the selected captured View/history owner replacement under Accepted ADRs0040/0094/0117,
not complete Proposed0121 or the foundation Goal. Source is the intended `refactor/foundation`
working tree above `9f79d51`; the implementation commit includes this record. Original untracked
`vendor/` and read-only reference repositories are outside the diff. No push or PR.

Windows11, Rust `1.95.0 (59807616e 2026-04-14)`, explicit `WGPU_BACKEND=dx12` and
`NARA_WGPU_FORCE_FALLBACK=0` for native selections. The machine reports an NVIDIA GeForce RTX4090,
driver32.0.16.2101, alongside virtual display devices. Actual adapter/device failure fails the
native tests; unavailable hardware is not converted to passing skips. Cargo is serial, jobs1,
using the original root `target`, including independent workspaces. The published WGPU dependency
is30.0.1. These results qualify the chosen platform/backend only.

## Result And Owning Changes

- `RenderViewId` is a non-reused process identity, distinct from Entity, target, extent or ordering.
  The same exclusive extraction call observes actual Camera membership, including filtered
  cameras. Target absence/projection failure does not retire it; removal/readdition does. Manual
  Views retain the same identity with their actual owner, and bounded packets reject duplicates.
- Logical Feature declarations retain original definitions independently of Host-native instances.
  Each preparation binds an immutable actual-owner snapshot; new membership publishes only with
  the successful candidate. Old accepted inputs never consult latest topology. Fixed active owner
  slots and candidate metadata have independent admission limits.
- `WgpuHistoryView`, source-clock lineage and code slot establish temporal ownership. Cached slot
  tokens retain Weak View/effect leases, while original topology/captures/candidates retain the
  strong owner. Destroyed Views reclaim slots even with one shared static effect. Foreign live
  temporal owners cannot adopt a predecessor by reusing a Copy ID.
- One caller-owned non-Clone candidate pins complete predecessor/empty/reset facts before graph
  construction. Refusal returns that same candidate. Actual owners, program, target facts and
  RAII budgets travel together without a Host candidate registry or retained acquired surface.
  Unbound native submission reservations stay within a serial attempt. Outputs, history pointers,
  membership and lineage watermark publish at the existing final transaction boundary.
- Reset/regroup/slot reclamation do not erase successful-capture deduplication. A replacement
  Host/epoch explicitly starts `DeviceRecreated` temporal history; it does not reconstruct missing
  old accumulated pixels. Exact typed final/slot outputs retain their original storage.
- Successful publication survives old Bindings and consumed-candidate destructor faults. Original
  prefix custody also isolates late per-entry destructor faults in reverse flat order, attempts
  remaining releases and records cleanup before `Completed`. Genuine callback abandonment retains
  its separate original-device recovery. Prefix/output observations stay readable after Host
  token retirement; one removal decision transfers stable counters without a clone-completion race.
- Hard program metadata admission clamps caller allowances before graph vectors/compilation.
  Candidate reservation covers64MiB program plus2MiB bounded auxiliary metadata; inline capture and
  staged binding checks precede owner construction. Actual final storage must satisfy declared
  shape/format/usages; incompatible initial storage leaves the unsubmitted reservation before
  normal allocation. Pure preflight refusal still preserves compatible storage.

## Actual Evidence

| Scope | Actual result and source limit |
| --- | --- |
| Neutral Render |48/48, run `55fa281e-f44d-4be4-90dd-e6926ec70486`; current View identity implementation, before later native-only corrections. |
| Sprite Render |35/35, run `19c6dc07-fe0c-4299-ad6c-33ed26dc25cd`; migrated manual View fixtures. |
| Initial WGPU library/public UI |365 passed /42 skipped, run `830a0e39-e8ba-4105-9528-fea20a9439b8`; before review corrections, not final evidence for them. |
| Root affected public consumers |160 passed /1130 skipped, run `482aeba8-f91d-4f94-b10b-272528d6582d`,86.465s after7m05s build. Game/Canvas/UI preview/images, desktop presentation, plugin composition and product capability selection; before original-prefix cleanup correction. |
| Focused flat cleanup/metadata |4/4, run `a99705a8-582f-478d-a366-544310e4955e`. Multiple independent destructor faults, hostile panic payload, reverse order, release-in-progress and monotonic observation. |
| First native publication/history |5/5, run `38219d96-f011-44c8-a04a-6d22a071563d`; synchronous old-binding/final-capture success and initial-storage usage, before late final-holder fix. |
| Submitted/native owner expansion |14/14, run `4aadc140-b433-41ea-b7d9-649a8c599ad9`,12.390s. Includes real native final-Arc Capture release, original callback abandonment/recovery, rejected/SDK prefixes and original-device retirement. |
| Broad WGPU/public UI |366 passed /1 failed /46 skipped, run `07591736-ee1e-4bd2-901f-1be968bc0542`,114.134s. The only failure was the capacity fixture described below. |
| Corrected policy cohort |23/23, run `f00ac0e4-456c-45ac-a5fc-14b16933483c`,3.679s. Includes separate single-candidate byte refusal and producer-wide active-owner reclamation. |
| Final owner/state/policy/native |62 passed /346 skipped, run `af7f8c69-30fa-4d3b-9985-fff45af00b6b`,19.310s. Current production snapshot includes cleanup counter handoff correction and actual Host observer regression. |
| Independent temporal numeric/window |7/7, run `29aeae63-ba81-4e30-9810-029f24692df4`,7.522s. Six numeric/lifetime cases plus actual Runtime window after actual destination-format support. |
| Final entire independent policy package |13/13 across6 binaries, run `b98ce895-6592-4003-b5a7-8bf900f7e58d`,12.458s, current final owner snapshot. Both scalar and temporal window examples prebuilt through the locked standalone workspace; children invoke no Cargo. |
| Final independent direct program |8/8 across3 binaries, run `0acf7adb-9f71-4f1a-beee-4041da7810fd`,3.067s. Public texture/buffer kernels, original captures and exact output custody. |
| Final independent image output |3/3 across2 binaries, run `3f0d5ea4-a3f8-4fe6-a161-97634dc8c193`,2.856s. Exact old pixels, retirement and independent producer progress. |
| Native Editor |11 passed /292 skipped, run `9bcf4314-3c36-427a-acce-17446e4d1737`,24.749s after4m52s build. Critical3, retained Scene/Prefab navigation, Canvas priority and concrete retirement6. Before final counter-transfer-only correction; final owner/native62 covers that correction. |
| Final affected Reference Game |10 passed /1 filtered, run `f854bc72-1abe-4d95-bdc0-8d2cb3ea764b`,2.939s after5m15s build. Locked `editor` feature, desktop extraction, saved UI refresh and Game input; the separate physical desktop-process test is excluded and receives no fresh qualification from this cohort. |
| Workspace correctness |`cargo +1.95.0 clippy --workspace --all-features --all-targets --locked -- -D clippy::correctness` passed4m28s on final production snapshot; warnings remain and no warning-free claim follows. |
| Minimal native |`check -p nara_render_wgpu --no-default-features --lib --locked` passed2.81s. |
| Portable compile |Same minimal check with `--target wasm32-unknown-unknown` passed2.25s; compile only, not browser execution. |
| Format and documents |Root and affected policy/output standalone `cargo fmt --all --check` pass. Changed Markdown relative link targets and staged whitespace checks pass. Engineering memory validates with existing legacy warnings; derived views are regenerated at integration. |

Native tests use original Host image-use submission/readback rather than an independent Queue
executor. The four-chain oracle evaluates `Hn = 0.25 Cn + 0.75 Hn-1` with an independent f64 CPU
model, checks every linear RGBA32Float history pixel and composed RGBA8 output, and retains old
versions across reorder/replacement/reset/refusal/A-close/B-progress. One case disposes513 View
owners while a static Feature remains. Runtime temporal presentation uses two ordinary Cameras,
real extracted IDs and three actual presented receipts as predecessor expectations. It proves
normal acquire/submit/present/retirement, not monitor scanout or a second window-pixel oracle.

### Failure And Correction History

The broad WGPU run's single failure requested1024 Feature bindings in one candidate. Newly enforced
512KiB binding metadata correctly refused before producer slot capacity. Do not raise that budget
or weaken the assertion. The corrected test fills1024 producer slots using eight separately
published128-Feature policies, retires one original declaration and admits its replacement.
Another test retains the oversized single-candidate refusal and checks no published owner or
policy reservation leaks, then successfully prepares a smaller candidate. Both pass in the final
cohorts. The original failed run remains evidence, not reported as all-green.

Independent review also corrected concrete publication/cleanup, initial-storage and envelope
defects before this qualification. The final reviewer found a remaining counter race: a public
prefix clone could complete between one accumulation pass and a second removal pass. Host now
tests completion, transfers both stable observations and removes the prefix in one `retain`
decision. The new native observer case checks before-maintenance visibility, transfer on removal,
no repeated count and retained output/prefix observation. Final read-only review reports no
remaining P1/P2 in this corrected boundary. It did not claim to run Cargo itself.

## Research Disposition And Remaining Work

The supplied October30 ZIP hash remains
`99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`; all91 manifest entries and ten
imported studies match. Independent current-source runtime/content and render/Editor reviews find
no additional unowned plan requirement. The [disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
keeps historical baseline claims separate from current successors. No reference checkout or
external release claim was refreshed by this document comparison.

Complete independently paced two-window/image composition, source/processed capture interleaving,
clipped Nara-drawn Scene/control composition, durable material declarations and the editable
multilingual Inspector remain open under the original parent plan. R5 coordinated Schema/claim/
occurrence evolution and R4c/R10 joint execution/motion remain separate foundation work. No full
ADR0121 acceptance, performance win, hot code unload, Linux/macOS/browser native qualification,
temporal pixel reconstruction after loss or full foundation completion is claimed.

## Citations

- [Current render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md).
- [Production design](../../../../architecture/render-program-production-design.md).
- [Independent policy consumer](../../../../../tests/fixtures/render-policy-consumer/README.md).
- [Native procedure](../../../../verification/native-editor-qualification.md).
- Prior integration records `adb17185d2534fb5bd768a08ed6e22dc` and
  `a0e84fabb63d49f6b893582e58a20926` retain earlier source and live-command limits.
