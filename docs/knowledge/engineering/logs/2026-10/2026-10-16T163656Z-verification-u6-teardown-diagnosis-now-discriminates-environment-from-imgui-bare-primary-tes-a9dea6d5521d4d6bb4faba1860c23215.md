---
type: "Memory Event"
title: "Verification: U6 teardown diagnosis now discriminates environment from ImGui. Bare-primary tes"
description: "U6 teardown diagnosis now discriminates environment from ImGui. Bare-primary test602d2967-7412-4e5b-a18c-8e507830e143 configures Nara Vulkan"
timestamp: 2026-10-16T16:36:56Z
record_id: "a9dea6d5521d4d6bb4faba1860c23215"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bb6314e"
event_kind: "Verification"
---

# Event

U6 teardown diagnosis now discriminates environment from ImGui. Bare-primary test602d2967-7412-4e5b-a18c-8e507830e143 configures Nara Vulkan RTX4090 driver617.14, flushes/polls and exits2173 on execution Drop without initializing/calling Dear. Same early-renderer binary on DX12 exits requested1; Vulkan child with all implicit layers disabled3eb5f9 also exits1, and individually excluding VK_LAYER_GAMEPP or VK_LAYER_NV_present e11cf7 each exits1. No system settings or production filters changed. Khronos official LoaderDebugging documents these process filters. Both real bridge negative/positive and bare tests pass on unfilteredDX12 f262213c-5b0d-4dc1-93e2-e5f328f222f1 and Vulkan child excluding onlyGamePP 39df5edc-37ba-4c16-bf7f-94ef1ee3460e. This does not assign sole blame to either overlay implementation. The separate upstreamWinit0.20 RefCell combined-shutdown defect is avoided by explicit renderer-disableviewports-platform sequence and the bridge test passes. Latest integratedEditor WGPU UIrender f0ac7f6a-032c-4144-91fe-d3c071dbfbd7 passes233 normal tests, twoignored native tests separatelyexecuted. Latest reference binary build36194 completed; DX12 earlyplatform/earlyrenderer exit1 correctly, observed fail-after-present actualmessage initially mismatched controllertext (notengine failure), then238b14 passesexpectederror and all8x4native lifecycle phases with realdestroy recovery, stale rejection and shutdown. Expected lost-oldGPU retirement diagnostic is preserved. Latest51913 DX12 realDPI120 detached ChineseIME clipboard cancel Tab Restore Reset exit0 leavesoneprimary. Fullworkspace alltargets allfeatures locked check62349 remains live; no otherCargo/native. New delta review found no blockers, duplicateinitialization drain simplified thenbuilt. U9 U1-U5 committed; U6 notyetcommitted, finalchecks andgatewrite pending; U7 U8 fullscope stillpending.

# Impact

# Citations
