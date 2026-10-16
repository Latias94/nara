---
type: "Verification Evidence"
title: "Editor U6 qualifies direct Dear ImGui on Windows with explicit backend and cleanup limits"
description: "U6 native presentation, input, text, lifecycle, comparative workload and independent review follow-up; U7 full migration and U8 external acceptance remain pending."
timestamp: 2026-10-16T16:39:33Z
record_id: "e42a0dbfec6643e380f6e9275f29b0d2"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bb6314e"
---

# Verification

Scope: ADR 0118, active Editor Foundation plan U6, R7-R10 and AE4-AE5 at the bounded
integration-probe scope. This verifies the working tree on parent `bb6314e`; it is not a
committed-source external acceptance record. U9 and U1-U5 remain committed prerequisites.

# Result

Adopt the direct Dear ImGui 0.20.0 frontend for U7 implementation on the qualified Windows
profile. The probe uses the existing Nara application, one native event loop, one engine-created
device, revision-bound document actions, independent Scene/Game images, exact backend-produced
sample/present observations, and original Runtime completion authority. It has passed the U6
Windows feasibility gate. The ordinary production Editor remains egui until U7 parity is proven.
U7 full frontend migration and U8 clean external acceptance/removal are not complete.

The fully exercised final cleanup/recovery profile is Windows 11 / DX12, with native monitor DPI
144 and 120 and an explicit trusted SimHei font merge for Chinese text. Vulkan normal interaction
also has earlier native evidence. On this installation, default Vulkan implicit-layer interaction
can abort pre-first-present teardown even in a bare Nara primary without any Dear initialization.
Filtered child diagnostics establish that this failure depends on the layer combination, not that
one particular layer is independently defective. Neither production nor system settings filter
layers automatically. Native X11/macOS and complete accessibility remain unqualified; Wayland
source policy retains docking and refuses detachment with a reason.

# Evidence

- Vendor actual-GPU library/integration run `e5ff46d3-7456-4dd8-b9e7-ec4e82be1516`: 100 pass.
- Latest integrated Editor/WGPU/UI-render libraries: `f0ac7f6a-032c-4144-91fe-d3c071dbfbd7`,
  233 pass, two interactive native tests separately exercised. One test thread, DX12.
- Reference Game input/recovery: `36ac3cd2-aba2-44b0-9bc8-8466b290b325`, four pass, including
  pending dirty Exit preservation and exactly one queued Step across source retirement.
- Actual secondary rejection/acceptance and bare-primary tests: two pass on unfiltered DX12
  `f262213c-5b0d-4dc1-93e2-e5f328f222f1`, and two pass on Vulkan with the child excluding only
  GamePP `39df5edc-37ba-4c16-bf7f-94ef1ee3460e`. The bridge checks Device and Queue independently,
  replay, an alias TextureView of the same Texture, overwritten image receipts, fresh positive
  content, and secondary presentation with zero primary acquisitions. No public report constructor
  or fake successful report is available.
- Final native `238b14`: eight four-frame primary/secondary/minimized/returned/closed/reopened/
  recovered phases, actual `device.destroy()`, new namespace on the same primary window, paused
  image publication, Resume, Stop, stale-content rejection and renderer-before-platform shutdown.
  The expected old lost-device retirement diagnostic is preserved. Final normal close exits zero.
- Final early-construction probe `315d60`: expected exit one after platform attachment and after
  renderer creation on DX12. `238b14` verifies expected failure after first actual present without
  a false secondary cleanup error. An initial controller string mismatch was an observer error.
- Final physical text/layout run `51913` / output `f4b034`: detached DPI-120 Chinese Pinyin
  preedit/commit, sentinel-checked paste/copy/cut, composition cancellation, Tab/ShiftTab, Restore
  and Reset; old secondary destroyed automatically, exactly one primary remains.
- Actual mouse Game docking/detachment/redocking, DPI 144/120 move/resize, secondary close/reopen
  while Play stays Running, capture release, live CJK atlas update and secondary Stop: `51827`.
- Ordinary idle Scene close/reopen with a fresh view, independent Scene resize and paused Game
  resize with actual presented images: `4de631`. Idle follow-up observation `d17c4b`: zero CPU
  seconds in 5.019 seconds, four total launch redraws. No permanent repaint loop was introduced.
- Same-consumer egui/Dear debug first-present, redraw timing, idle CPU, memory, GDI/User objects
  and retained WGPU handles: `39262`, with finite measurements and limits in adoption evidence.
- Latest workspace all-target/all-feature locked check `62349`: exit zero, 1m36s.
- Latest affected Editor/WGPU all-target/all-feature locked Clippy `70101`: exit zero, 1m11s,
  with existing warnings; this is not a warning-free claim. The earlier full-workspace Clippy
  also passed before the final recovery and initialization deltas.

## Independent review and dispositions

Full nine-persona receipt `20261016-153024-b9f25bd9` is complete, with a frozen-snapshot
`Not ready` verdict and three independently validated findings. Its original artifacts remain
unchanged. Explicit prohibition on external providers was honored; no cross-model independence
is claimed. Final report rendering used the non-dispatch parent after the platform thread limit
prevented a separate report leaf; dispatch, merge and validation stayed separate.

1. Recovery continuation: application deadlines survive renderer absence, including expired timer
   rearming. A Session-owned recovery Pause preserves ordinary close admission and existing control
   tickets. The application directly advances the published/retained Session, clears a newly formed
   Step presentation wait without another Runtime tick, and both native Hosts reuse it. Generic
   external Play does not wait for an embedded-GPU Pause. Reference four-test run and final native
   recovery pass; independent delta review confirmed the retained-owner correction.
2. Direct secondary bridge rejection coverage: the two real native tests pass on both explicit
   profiles above. The initial fixture exposed a separate upstream `dear-imgui-winit` 0.20.0
   combined-shutdown RefCell defect. Nara's explicit renderer -> disable-viewports -> platform
   sequence avoids it; no Winit fork was added. Aborted stale frames complete the public platform
   update contract before another frame begins.
3. Idle Scene reopen: extent/lifetime changes schedule one successor paint. Ordinary non-canary
   run `4de631` verifies fresh presentation and resize. Inactive Dear viewports receive only the
   finite follow-up frames required by the pinned core's native-window retirement timing;
   Restore `47191` and final DX12 `51913` verify physical destruction without injected repaint.

Subsequent independent delta reviews found no remaining blocking defect. The repeated drain
suggestion was applied by giving renderer-creation failure and post-creation rollback one drain
each. Large-file size alone was rejected as an unsupported review requirement. Simplification
completed quality/efficiency/test-value/comment passes; the reuse worker returned no usable
result and root performed that rubric inline. Proof tests were preserved.

# Follow-up

Begin U7 only after the precise U6 commit. Migrate Project Manager/Open/Recovery/replacement,
browser/import, generic Scene/Prefab/custom editing, specialized UI Canvas, external and embedded
Play, registered panel companions, shortcuts and preferences through the existing semantic owners.
The probe's finite hard-coded widgets are not the product panel architecture. U8 must validate a
clean external project from committed source, prove Play repair/Check/Export, and remove egui plus
abandoned qualification paths. Preserve the complete original Goal.

Keep the upstream shutdown defect and the Vulkan-layer environment diagnosis separate. Future
Dear upgrades must recheck internal viewport-retirement state access. No native Linux/macOS,
physical GPU-byte benchmark, long leak soak or full accessibility completion follows from U6.

# Citations

- `docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md`
- `docs/architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md`
- `docs/research/2026-10-16-editor-foundation/adoption-evidence.md`
- `vendor/dear-imgui-wgpu/NARA_PATCH.md`
- `C:/Users/Frankorz/AppData/Local/Temp/compound-engineering/ce-code-review/20261016-153024-b9f25bd9/review.json`
