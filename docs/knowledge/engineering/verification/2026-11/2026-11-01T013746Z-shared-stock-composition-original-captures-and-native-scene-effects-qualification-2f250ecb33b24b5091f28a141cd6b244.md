---
type: "Verification Evidence"
title: "Shared stock composition original captures and native Scene effects qualification"
description: "Production stock-stage composition and exact rendered resource ownership qualified through independent Runtime Editor and offscreen consumers."
timestamp: 2026-11-01T01:37:46Z
record_id: "2f250ecb33b24b5091f28a141cd6b244"
producer_id: "codex-foundation-main"
run_id: "stock-composition-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "c836fdb"
---

# Verification

Implementation: `c836fdb`, based on `3d7eb7b` on `refactor/foundation`. Tests executed the
corresponding uncommitted Rust source before the implementation commit. The separate registry
ownership plan is `a54495b`; it introduces no registry implementation. Subsequent documentation
closure does not change the tested product code.

Windows 11, Rust 1.95.0, locked dependencies, the root `target`, `CARGO_BUILD_JOBS=1`, serial Cargo
and one test thread. Native selections use `WGPU_BACKEND=dx12`,
`NARA_WGPU_FORCE_FALLBACK=0`; Editor selections also set `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.
Hardware inventory reports an NVIDIA GeForce RTX 4090, driver 32.0.16.2101. The actual native
consumers independently require DX12 and reject a CPU adapter; this is not a performance result.

# Result

The shared stock-composition unit is qualified in the scopes below. `WgpuStockStages` supplies
world drawing, explicit optional Canvas resolution and final UI through the existing typed policy
and finite graph. The default policy invokes those same helpers. External blur and grade packages
select world-raster or resolved-target inputs and preserve an untouched output branch. The Host
retains allocation, submission, presentation, epoch and retirement authority.

Policy-independent stock collection hands the original snapshot and move-only UI sidecar to the
exact selected available composer. Refusal returns original responsibility; logical Close settles
unsubmitted drafts. UI admission failure without a sidecar clears its draft so later collection
can recover. The Runtime driver defers only the three specified capacity/backpressure cases;
owner replacement, missing/closed owners and other faults remain fatal.

One canonical recipe configuration pairs the Runtime plugin and fresh Scene/Canvas capture
adapters. Normalized Headless/Check profiles retain recipe membership while omitting renderer
activation. Final-definition preflight rejects an outer edit that would separate the Runtime
configuration from its Editor adapter. Scene picking retains its exact batches and provenance
independently of a default-policy downcast. Content refresh creates a fresh installation while
old accepted captures keep their original inputs.

An image now retains its selected `ProgramResourceStorage`, including original semantic,
descriptor, epoch and allocation custody. Its typed texture lease shares that exact resource;
wrong markers reject. Keeping an image does not retain the whole program, reports, scratch or
CPU capture. The actual native Scene test reads the displayed final storage and joins its image,
source revision, producer, epoch, display occurrence and native composite receipt.

# Evidence

| Executed scope | Actual result | Run identity or command |
| --- | --- | --- |
| Render-neutral, WGPU and Editor ordinary all-feature libraries | 701 passed, 87 skipped;188.806s | `10b3b5e7-5ced-4df9-9e5c-9ba030e35f61` |
| Final affected Root product/composition/Scene/Canvas/refresh and public Check/runtime configuration | 39 passed,1175 skipped;6.816s | `73ce100a-18f5-4056-b031-17e74b9dff7f` |
| Final public plugin/UI capture, shared stock helpers and actual native Editor Scene effect | 79 passed,1932 skipped;15.491s | `04e7026b-ab1a-49a8-82eb-fd3b4d03414d` |
| Final independent stock/effects workspace, all ordinary and explicitly ignored native cases | 9 passed,0 skipped;8.817s | `7adfbeb5-e6b2-4d4f-b465-9512041a935c` |
| Native Editor three critical trajectories plus retained Scene/Canvas navigation | 5 passed,299 skipped;13.874s | `216985ec-58ad-4acb-8396-656ba45ec4fb` |
| Reference Game product composition, UI Prefab refresh/Restart and Game input/recovery | 10 passed,0 skipped;3.137s | `2f6c93c8-28a9-4eb2-a261-add1130ddf7a` |
| Root/render/WGPU/Editor all-feature/all-target correctness Clippy | Passed;3m15s; advisory warnings retained | `cargo clippy -p nara -p nara_render -p nara_render_wgpu -p nara-editor --all-features --all-targets --locked -- -D clippy::correctness` |
| Final renderer library correctness Clippy | Passed;21.06s; advisory warnings retained | `cargo clippy -p nara_render_wgpu --all-features --lib --locked -- -D clippy::correctness` |
| Independent fixture/effects all-feature/all-target strict Clippy | Passed;5.62s | `cargo clippy --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --workspace --all-features --all-targets --locked -- -D warnings` |
| Minimal WGPU owner, native | Passed;3.54s | `cargo check -p nara_render_wgpu --no-default-features --lib --locked` |
| Minimal code-first Root desktop/WGPU | Passed;5.05s | `cargo check -p nara --no-default-features --features desktop-winit,render-wgpu --lib --locked` |
| Root workflow/Tooling/2D/UI without WGPU | Passed;14.09s | `cargo check -p nara --no-default-features --features project-workflow-cli,tooling,presentation-2d,presentation-ui --lib --locked` |
| Portable neutral Root2D/UI/Tooling, wasm32 | Passed, compile only;1m48s | `cargo check -p nara --no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --target wasm32-unknown-unknown --lib --locked` |
| Minimal WGPU owner, wasm32 | Passed, compile only;3.64s | `cargo check -p nara_render_wgpu --no-default-features --target wasm32-unknown-unknown --lib --locked` |
| Root and independent workspace formatting; staged whitespace | Passed | `cargo fmt --all -- --check`, corresponding manifest check, `git diff --cached --check` |

Counts overlap and are not additive. The selection of 79 tests contains one actual native Editor
trajectory alongside public/CPU tests. The ordinary run of 701 tests preceded the final equivalent
last-clear iterator simplification and fixture-only lint/import cleanup. The final selections of
79 and 9 tests execute those final sources, including the actual displayed Scene and original
Runtime replay. The earlier 20 exact-image/native-owner and 10 Runtime recovery results remain scoped predecessor
evidence in the [integration capture](../../progress/2026-11/2026-11-01T005125Z-actual-stock-runtime-and-native-scene-qualification-with-final-owner-gates-in-progress-097be6576e5c42949c40e1af15329253.md).

The independent native run covers actual RGBA8/BGRA8 linear/sRGB ports, half-float private blur
scratch, spatially varying premultiplied alpha, exact alpha-zero regions and retained original
pixels. Effect tolerance is 3 encoded values, fixed before native results; the source UNORM
rounding bound is 1. HUD pixels and opaque unchanged source controls remain exact. The Runtime
project uses 33x24 with a matching authored UI surface, exercises half-open pixel-center coverage
and derives its Schema configuration from the same written manifest. Its original HUD sidecar
completes through the ordinary product bridge. After source, Runtime and native Host retirement,
the exact original capture replays blur/grade/HUD with numerical readback on an offscreen Host.

# Review And Failure Corrections

Three independent code reviews cover Root/profile/definition pairing, external effects/actual
consumers, and stage/adapter/exact-resource ownership. Their final narrow reviews leave no P1/P2.
The reported pixel-center oracle P2 is fixed with inclusive top/left and exclusive bottom/right
coverage and the odd-width native case. The registry plan has a separate source audit and
adversarial review; its owner-presence P2 is resolved only as a planned contract and acceptance.

Earlier broad Root testing returned 1208/1211, with three fixture failures. The missing common
capture plugin and precise missing-default edit error were corrected; the final selection of 39 tests
executes those cases plus the final profile/definition corrections. Windows abort text in those
fixtures accompanied a reported missing-plugin panic, not evidence of another stack-overflow bug.

The independent native trajectory progressed from 5/9 to 8/9 before the final passing run. Fixture
repairs registered the real HUD hierarchy, moved its stripe off the fill-sized root, declared the
missing primary UI surface and correct sampled usage, and removed a contradictory exact 128 alpha
tie assertion while retaining the predetermined source rounding bound. No native timeout or
effect tolerance was widened. The initial Editor fixture queried an unfrozen registry and used
Result-style handling on an Option; both were corrected before actual native execution. Strict
fixture lint also required explicit Option matching and a standard range expression; neither
suppresses a lint or changes the numerical oracle.

# Limits And Follow-Up

The complete foundation Goal remains active and full ADR0121 remains Proposed. Durable shader/
material generations, broader captured View/time facts, measured geometry and reproducible cost
attribution, Schema/claim/occurrence evolution, joint motion/execution, and the editable Nara-drawn
Inspector retain their existing gates. This qualifies the working Dear frontend's shared render
path, not the future frontend. Linux/macOS, browser execution, monitor scanout, editable-text/IME
and assistive-technology support are not established by these Windows/DX12 and compile-only runs.

The source audit selects the [registry ownership sub-unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
as the next bounded R12 replacement. Reconcile ADR0081, use a mutable builder to publish the sole
immutable snapshot, return failed candidates for repair, and distinguish absent owners from
installed-but-unadmitted ones. No builder implementation or new Schema support is claimed here.

The supplied research ZIP still matches all 91 manifest members and all 10 imported documents;
its hash and receiving-baseline limits remain in the [research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md).
No roadmap reset is needed. Locks add only three local fixture/effect packages, with no external
version changes; independent external versions stay within the root lock. No remote publication,
new migration guide, compatibility alias or vendor/reference edit was performed.

# Citations

- [Render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Production design](../../../../architecture/render-program-production-design.md)
- [Independent consumer and exact commands](../../../../../tests/fixtures/stock-composition-consumer/README.md)
- [Native Editor protocol](../../../../verification/native-editor-qualification.md)

# Result

# Evidence

# Follow-up

# Citations
