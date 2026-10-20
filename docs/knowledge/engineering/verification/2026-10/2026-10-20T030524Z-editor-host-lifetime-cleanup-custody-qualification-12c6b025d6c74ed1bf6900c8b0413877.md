---
type: "Verification Evidence"
title: "Editor Host lifetime cleanup custody qualification"
description: "Scene and Prefab cleanup survives terminal Save, Reopen, content runtime replacement, close and session abandonment."
timestamp: 2026-10-20T03:05:24Z
record_id: "12c6b025d6c74ed1bf6900c8b0413877"
producer_id: "codex-root"
run_id: "editor-host-custody-20261020"
related_plan: "docs/plans/2026-10-20-editor-host-lifetime-custody-plan.md"
git_branch: "refactor/foundation-contract-hardening"
git_commit: "fe3ad96"
---

# Verification

Windows 11, Rust `1.95.0`, shared `target`, `CARGO_BUILD_JOBS=1`. Baseline `6071425`.
Cargo commands ran serially. Unrelated `vendor/` and `myeasylog.log` were preserved.

## Result

Each Scene/Prefab lane now references one typed retained Host. The runtime's private close
participant owns its retirement registration for the complete lane lifetime, including idle periods
after a failed Save. Save borrows the Host; task terminals contain no cleanup authority. A successful
content-runtime replacement transfers registrations before dropping the predecessor. Failed
replacement candidates cannot retire live Hosts. First physical close closes new-work admission
before finishing previously admitted persistence; a stopped session cannot create new file duties.

## Evidence

- Initial focused engine/public cohort: **124 passed**, 81 excluded;
  `cf11fc85-6ec7-4b24-88a1-1d6c966ef5e8`.
- Final focused engine/public cohort after the runtime-replacement regression: **125 passed**,
  81 excluded; `93bfa06e-186a-4cb2-987c-e9f85822dea2`. Includes lower persistence,
  Editor runtime and background persistence, `editor_persistence`, `workspace_play_runtime`.
- Editor/application cohort: **189 passed**, 20 native tests excluded;
  `428d54fa-5311-419e-9d76-ff46b061c536`. Library plus `application_contract`, `authoring_loop`,
  `first_game_workflow`, `generic_component_composition`, `generic_open_recovery`.
- Three selected actual Windows/DX12 native tests: **3 passed**;
  `e17fff9e-31fc-45e3-a5b5-42c1b8d901ee`: ordinary Scene/Prefab Save/Reopen,
  Canvas pick/move/resize/Save/Reopen, and paused-resume redraw.
- `cargo +1.95.0 check --workspace --all-features --all-targets --locked`: passed.
- `cargo +1.95.0 fmt --all --check` and `git diff --check`: passed.

The new compound tests use an independent private atomic cleanup release control, which does not
retain the Host. They assert actual target bytes, temporary-directory entry counts and runtime
quarantine retention, then release cleanup and prove retirement returns to the original count:

- Scene and Prefab terminal failed Save -> direct Drop or close attempt -> Drop.
- Failed and stale Reopen after failed Save -> Drop, preserving document and saved state.
- Browser document replacement rejects blocked cleanup, then succeeds after release.
- Trusted compiled Prefab Save rebuilds the authoring runtime while Scene cleanup is blocked;
  the original Scene still Reopens and edits, then Drop retains cleanup until release.
- Held Reopen -> first close rejects new Save/Reopen/Play/Open before the read returns; the admitted
  read completes, then the stopped session continues rejecting new work.
- Queued Save whose Host has already retired returns Busy without changing target bytes.

Existing held publication, logical cancellation, worker panic, delayed acknowledgement, preview
continuity and explicit cleanup retry tests remain in the passing cohort. The panic test exercises
the SaveWork lock; no injected panic inside the Host mutex is claimed.

Two independent read-only integration reviews found no current reachable blocking defect. One
reviewer authored the retirement module, and the other independently reviewed that module and
the root integration. Review specifically traced Fault versus Stop, lock order, queued/running
work, failed replacement, stopped admission and exact owner publication.

## Limits And Follow-up

This is targeted development qualification, not cross-platform release certification. Counts
overlap and are not a unique total. Existing v1/v2/v3 timing data stays frozen; this correction
does not claim a latency improvement. OS calls remain non-preemptible and can keep close pending.
The independent cleanup fault is private test instrumentation, not a claim about all OS failures.

The audit reconciliation found no other unclosed confirmed correctness defect among the selected
N/I corrections. Startup-closure preparation remains the next measured responsiveness trigger.
Browser/source receipts, task capture disposal and high-batch GPU holds require their own
consumer measurements. Multiple same-kind documents, code freshness, Tile tools and other
untriggered capabilities remain separate product work.

## Citations

- [Plan](../../../../plans/2026-10-20-editor-host-lifetime-custody-plan.md)
- [Compound tests](../../../../../src/project_host/runtime/tests/host_custody.rs)
- [Retirement owner](../../../../../src/project_host/runtime/editor/persistence/retirement.rs)
- [Previous Reopen evidence](2026-10-20T022302Z-owned-editor-reopen-preparation-and-publication-qualification-557ea95db99b4b8a87829870db1d6b13.md)
