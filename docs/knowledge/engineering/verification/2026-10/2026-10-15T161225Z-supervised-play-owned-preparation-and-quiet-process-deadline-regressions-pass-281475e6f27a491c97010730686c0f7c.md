---
type: "Verification Evidence"
title: "Supervised Play owned preparation and quiet process deadline regressions pass"
description: "Windows Editor libraries and locked UI retry pass; native product and remaining integration checks are pending."
timestamp: 2026-10-15T16:12:25Z
record_id: "281475e6f27a491c97010730686c0f7c"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
---

# Verification

U12 Windows focused tests run serially with one Cargo build job and reused target artifacts.
The private generation preparation worker and the native wake/deadline routing compiled as part
of the actual Editor package. No new native Linux/macOS process evidence is claimed here.

# Result

- Editor library suite: 93 passed, no skips, run 8a025303-1fc4-42e2-9d96-eea9a07669c8.
- Locked close retry plus ordinary authoring lock: two passed, run
  43056b77-4770-4145-8406-8790304a9363. The first fix put the retry control after the full content
  region and the real pointer-click test still failed; it now appears before locked content and
  replaces the disabled Play control while ordinary toolbar actions remain disabled.
- Two process deadline tests failed before their production correction, run
  a6524422-05b3-49c9-89a2-0ada6526a8b6: a silent Running child had no exit observation deadline, and
  ForceKilling returned the expired terminate deadline. Both pass in the complete 93-test suite.
- Root Host's duplicate-observation regression first failed in
  0fe7038c-afa7-4ff7-b847-0f7b026f0bc6. Its corrected observation and close-retry projection are in
  the current 198-test root/CLI integration run and are not claimed passing by this record.

# Evidence

The controlled publication test holds a real owned generation store in the worker, requests
Cancel and Close, and verifies the UI-side advance returns without collecting or declaring close
complete. Releasing the worker permits a physical join and exact store retirement. It uses no
wall-clock responsiveness threshold. A separate real generation cleanup failure retains its
identity until RetryClose succeeds. The actual Reference Game authoring-open test verifies that
a quiet effects-only wake retains the same published frame Arc.

Running processes now expose one stable 250ms observation deadline, using the existing nonblocking
platform exit check and single reaper. Cleanup exposes only the current phase's relevant deadlines;
explicit incomplete cleanup waits for retry. Native repaint deadlines remain independent of owner
effect deadlines. Effects-only timeouts update the retained owner and draw only for a visible
change; close-wait branches no longer invent actions_observed on every call.

Publication hash/copy/sync and generation launch revalidation execute off the UI thread. Copy and
filesystem phases check cancellation; an OS sync cannot be forcibly interrupted. Normal close
polls and retains the worker/store until physical completion. Abnormal abandonment during
error/unwind synchronously cancels/joins as fail-safe cleanup, does not issue a close receipt, and
can leave an inactive unremovable store for the existing scavenger. This limitation is explicit.

# Follow-up

Collect the root Host/project-tool integration run, verify Editor application/authoring/recovery
integrations and the existing native supervised-Play canaries, then run affected static checks.
Preserve original staged SPL work and precisely commit only owned paths/hunks. The focused U12
review is not the final full-plan ce-code-review receipt; U6-U13 completion remains unproven.

# Citations

- editor/src/supervised_play/preparation.rs
- editor/src/supervised_play/owner.rs
- editor/src/supervised_play/process/mod.rs
- editor/src/platform.rs
- editor/src/app.rs
- crates/nara_tooling_egui/src/tests.rs
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T160316Z-quiet-supervised-play-needs-effects-only-exit-observation-and-current-phase-deadlines-db7102914b24438b9767824c3cf7ebe4.md
