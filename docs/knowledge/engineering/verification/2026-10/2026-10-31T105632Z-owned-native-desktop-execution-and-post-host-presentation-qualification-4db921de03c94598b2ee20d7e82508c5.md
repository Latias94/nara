---
type: "Verification Evidence"
title: "Owned native desktop execution and post-Host presentation qualification"
description: "Verification Evidence for Owned native desktop execution and post-Host presentation qualification."
timestamp: 2026-10-31T10:56:32Z
record_id: "4db921de03c94598b2ee20d7e82508c5"
producer_id: "codex-foundation-main"
run_id: "render-owned-native-session-2026-10-31"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "37ca8bb149c9945f2022912d78e644a8df9ffa3a"
---

# Verification

Implementation **`37ca8bb149c9945f2022912d78e644a8df9ffa3a`**, based on `cd35da0`, on
`refactor/foundation`. Windows11, Rust1.95.0 MSVC, serial Cargo/jobs1 with the root target;
independent and supervised builds also use it. Native checks select DX12, and the Editor requires
a real adapter. No push or PR. The complete foundation Goal stays active.

# Result

The stock Runtime GPU executor lives outside gameplay World in the retained native session with
Runtime, original target registry, registered IDs, windows and the original Winit pump. Early pump
return, close-budget expiry and retry retain this aggregate. Actual queue drain, surface/provider
retirement and native Destroyed differ from logical Runtime Stopped. Resource absence proves none
of these physical facts.

World owns a finite original CPU capture/result connection with issued connection/Runtime/sequence
identity. Original UI result storage is admitted before execution. Refused input returns intact;
consumed output with a post-operation fault is not replayed. Original buffered/in-flight custody
survives resource replacement. Direct Drop returns original permits before releasing the close
barrier. Cancellation invents no native observation/state. Authority checks precede collection as
well as Take/Return. Original Device/Queue custody survives namespace cleanup without telemetry.

One Root DesktopRun supports minimal code-first products and conditional Source/Processed startup.
The latter transfers original content/configuration context once. UI, first-present/READY and
startup markers join exact returned results after native execution. Wait maintenance advances
deadlines/publication without gameplay/Last or consuming the frame delta clock. Finite telemetry
sealing stops submissions, drains original counters/samples before cleanup and preserves final
consumer vetoes. Examples and actual Reference measurement/probe consumers use this owner.

The obsolete World retirement operation and error are deleted. WindowPresentationRequirement is
only CPU product intent, not retirement or no-render fallback. No borrowed runner, GPU Resource,
scheduled submit, compatibility alias or new migration guide remains.

# Evidence

| Check | Actual result and coverage |
| --- | --- |
| Final owning libraries | **227 passed**,17 ignored; run `4b6cc6d1-4f86-40fd-9d21-6352fc83713c`, build59.41s/run74.578s. Complete ordinary Window/Winit/WGPU libraries after deleting the obsolete World operation. |
| Original transport | Earlier final focused **50 passed**, run `a0cad60a-a870-4bbc-b59a-fe99676b3f60`, including controlled cross-thread actual-permit Drop, refused/missing/replaced ingress, retry, original logical close, cancellation and consumed-output/postfault. Final227 includes its ordinary cases and supersedes the deleted legacy-operation assertion. |
| Final Root | **58 passed**, run `12b768ab-b084-4734-9b44-4b800372d61f`, build1m07s/run7.217s. Desktop/UI/action/Game selections, five actual post-Host port cases and two repeated/final seal-veto regressions. Before marker-only cleanup; final native/public checks cover that replacement. |
| Final native owners | **7 passed**, run `c97c03e2-1bb0-4c3c-8a92-7009250cacd2`, build14.06s/run3.884s. Real Windows pump early Exit, budget retry/Destroyed, construction failure, Drop fail-stop child, ordinary window and maintenance-only wake; actual GPU work drains after namespace cleanup with telemetry disabled and healthy/destroyed device branches. |
| Native surface smoke | Baseline plus **four mutation modes passed** through minimal desktop/backend: remove ingress, replace ingress, replace CPU observation, replace World window registry. Every mode requires real initial/resized presentation followed by original surface/provider retirement and NativeDestroyed without a target fault. |
| Final native Editor | **4 passed**, run `0bd40c9c-e0e5-478f-91b8-f0756cbfe938`, build1m43s/run105.789s. Critical paused cadence, Game input/refocus/resize/Step, independent View close/reopen/recovery/retirement, actual supervised Cargo/READY/quiet maintenance/child-exit presentation. External process exit does not qualify graceful child teardown. |
| Reference cohort | **57 passed**, zero skipped across eight binaries; run `49475908-5749-44bd-9957-4c869f978686`, build4m26s/run13.488s. Measurement/CLI/probe, desktop flow/parity, composition, Prefab/content startup and actual desktop_render_probe. Before marker-only cleanup; final actual candidate is rebuilt through it. |
| Actual candidate seal | Ordinary `desktop --candidate-smoke` passes with UI observation enabled after final cleanup. Four warmups/16 measurement samples; sample loss0, end pending/lost retirement0. UI/backend namespace, app/render/admission/presentation identities agree, existing v2/v3 wire shapes and success marker remain. No comparative speed claim. |
| Public boundary | **3 passed**, run `b363359c-8bc2-484d-88cc-dcf64e3f827c`, build1m56s/run0.167s. Thread-bound native/executable owners, consuming Winit signature and CPU-only plugin installation with a negative GPU Resource assertion. |
| Minimal/independent | Minimal Root library/examples and actual surface smoke pass without serde. Independently locked renamed-root 2D and UI desktop consumers pass final checks without Scene serialization. Offline 2D lock extension adds103 package versions, removes/upgrades none. |
| Build/static | Root all-feature library/examples/tests check passed; final added regressions compile in Root58. Root/Window/Winit/WGPU all-feature lib/tests correctness Clippy passes `-D clippy::correctness` in1m26s with non-correctness warnings. Root/Reference fmt, whitespace and local architecture links pass. No full workspace test claim. |

Raw logs remain outside Git in the OS temporary directory. The native smoke executes
`target/debug/examples/window_surface_retirement_smoke.exe`; the candidate uses the ordinary
locked Reference manifest and root target. Native7 excludes exclusive/borderless qualification and
the internal fail-stop child as a direct test; its parent invokes that child explicitly.

Initial new transport fixtures incorrectly expected CloseIncomplete on the first Stop drive and
expected faulted retirement input refusal. They were corrected to the owning Runtime contract.
The first native maintenance fixture counted only ResumeTimeReached, missing a valid WaitCancelled
exit. Its correction checks every no-demand boundary while retaining six timer wakes, actual
redraw/clock, exit and Destroyed assertions. Final native7 is acceptance evidence. A setup-only
Editor run was deliberately stopped during compilation to add inherited shared-target configuration
before tests or child builds; that cancellation is not a product result.

Independent reviews found and closed the Drop-order race, pre-collection authentication gap,
cancelled native-state overwrite, premature Destroyed success, maintenance-driven simulation and
observer-veto loss. All changes were rereviewed at their owners. Workers ran no Cargo independently;
all agents are closed.

# Follow-up

This qualifies the single-Runtime, serial, window-backed native ownership replacement. It does not
qualify complete R8/R9a graph/Features, simultaneous streams, immutable generated outputs,
producer-prefix retirement, public window-free rendering, material evolution or the editable
Nara-drawn Editor. Proposed0121 stays Proposed. Continue immutable outputs, original reconstruction
inputs and producer-local submission custody with complete public consumers, then stock/external
Features and alternate policy. The full foundation Goal stays active.

No fresh Linux/macOS/browser, monitor scanout, OS IME/accessibility parity, broad Editor suite or
comparative performance claim follows. Earlier evidence retains its actual source/scope.

# Citations

- `docs/plans/2026-10-31-render-composition-and-host-execution-plan.md`
- `docs/architecture/render-host-ownership-design.md`
- `docs/architecture/adr/0032-render-backend-integration-boundary.md`
- `docs/architecture/adr/0042-runtime-service-and-backend-boundary.md`
- `docs/architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md`
- `docs/verification/native-editor-qualification.md`
- `docs/knowledge/engineering/progress/2026-10/2026-10-31T104400Z-owned-native-desktop-replacement-reaches-final-consumer-qualification-c8cad19571564013b118d99b9ac1a82c.md`
