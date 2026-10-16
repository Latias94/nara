---
type: "Verification Evidence"
title: "First author workflow closes the prior excluded source review scope"
description: "Verification Evidence for First author workflow closes the prior excluded source review scope."
timestamp: 2026-10-16T02:33:14Z
record_id: "136220880e154191a83573e2687f217f"
status: "verified"
producer_id: "codex-root"
run_id: "nara-author-workflow-20261016"
related_plan: "docs/plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md"
git_commit: "ddb45f7"
---

# Verification

U1 of the First 2D Author Workflow closes the concrete source exclusions from the
previous tracked-only foundation review. The scope is the 37 files newly tracked
in ca3d803 plus the product_launch/root.rs rename. Research archive/evidence files
are not executable implementation scope.

# Result

Two independent Codex readers inspected 29 and 9 paths respectively. Together they
cover all 38 paths. They ran no Cargo commands and claim static review only. Root
verified their two concrete findings and applied the fixes:

- Linux store_root.rs lacked DirBuilderExt in its production module. e3e8cab adds
  the import; Windows checks do not establish native Linux compilation.
- A literal `[project-root]https://...` product log bypassed locator redaction and
  caused the output message constructor expect to panic. Root's new regression
  reproduced the exact panic before the fix (nextest f56f04be-98dd-4648-9cc9-6e5da2829587).
  The fix removes that untrusted-prefix exemption and records rejected messages as
  explicit dropped output instead of panicking. All 93 Editor library tests then
  passed, including the regression.

# Evidence

The first reader covered native handle leasing, external Play views and controls,
generation publication/retention/scavenging, handshake, owner, process output/spawn,
store root, supervisor/wake, authoring catalogue, Cargo facts, and every
product_launch source/wire/platform/child/root owner. The tail reader fully read:

- editor/exact-generation-cwd-probe
- editor/src/platform/supervised_play_canary.rs
- editor/src/platform/supervised_play_canary/fixture.rs
- editor/src/supervised_play/output_projection.rs
- editor/tests/supervised_play_clean_room.rs
- editor/tests/support/supervised_play_project.rs
- reference-game/src/bin/editor_supervised_play_canary.rs
- reference-game/tests/editor_supervised_play.rs
- src/project_host/runtime/editor/trusted_play.rs

Root additionally ran 196 Scene/tooling/egui library tests successfully, the actual
ordinary Editor author workflow (614e8f92-52fc-4d88-8b51-efb55e963a03), and the
workspace all-features/all-targets locked offline check. These are implementation
verification, not independent reviewer reruns. The probe is an intentional tracked
fixture containing an explicit product CWD marker.

# Follow-up

This successor review does not rewrite the previous review's exclusion list or
claim native Linux/macOS process/window evidence. Generic custom component fields
remain read-only. Complete native platform trajectories remain separate gates.

# Citations

- docs/plans/2026-10-16-0940-feat-first-2d-author-workflow-plan.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T202150Z-full-foundation-review-completes-with-resolved-display-coverage-and-explicit-platform-limits-2f0fee3044664b538a2590f95e61131a.md
- editor/src/supervised_play/output_projection.rs
- editor/src/supervised_play/store_root.rs
