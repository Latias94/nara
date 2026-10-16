---
type: "Memory Event"
title: "Verification: U6 native 51827 passes actual mouse Game docking/detachment/redocking, native DP"
description: "U6 native 51827 passes actual mouse Game docking/detachment/redocking, native DPI 144/120, secondary resize/close/reopen while Play runs, Ga"
timestamp: 2026-10-16T15:31:59Z
record_id: "45ff1cea389b49a8b3f4000c4f6f7573"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bb6314e"
event_kind: "Verification"
---

# Event

U6 native 51827 passes actual mouse Game docking/detachment/redocking, native DPI 144/120, secondary resize/close/reopen while Play runs, Game input release to Editor, live CJK glyph update and secondary Stop reaching Empty. Native bd656a repeats all eight four-frame lifecycle phases and real device destroy/recovery. Root nextest 14eb747b-892d-4b0c-9d9a-30a3262a4200 passes 233 Editor/WGPU/UI-render lib tests; reference nextest 97ac8210-87b3-49b6-b64d-65b07a6568df passes both strengthened shared-input regressions. New fail-after-present canary fbef3d returns expected original error with exit 1 and no graphics cleanup exception; an event-loop error now also reaches explicit graphics cleanup. Final full review is running in Codex subagent u6_full_review_orchestrator; root all-feature workspace/all-target check 81094 is live, no other Cargo. U6 remains uncommitted and adoption pending; U7-U8 not started. A startup-close controller used a wrong native class name, then wrongly required zero presentations despite queued WM_CLOSE; it did observe clean exit 0 with two presents, so no before-first-present claim is supported. Never use those controller assertions as engine-failure evidence.

# Impact

# Citations
