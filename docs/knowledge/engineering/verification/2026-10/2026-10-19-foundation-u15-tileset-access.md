---
type: Verification Evidence
title: Foundation U15 concrete TileSet resource access
description: Runtime, managed Editor and TaskUpdate share one concrete publication resource join.
timestamp: 2026-10-19T09:29:00Z
record_id: 18a3d2ac726e42aabfa00e48fd0f90c9
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Implementation

`TileSetContent` is a concrete SystemParam with a private resource join. Runtime snapshot
publication, managed Editor refresh and TaskUpdate use it for request/reject/revoke/reconcile and
the read-only observations needed by their workflows. The Editor's nested with-assets callback and
the Runtime's seven-resource tuple are removed.

The Host still admits filesystem reads and decoded source data, chooses source updates and drives
Image dependency demand. TileSetAssets retains its independently testable publication/retention
state machine. No generic World service, importer executor, async lookup or Tile tools were added.

# Verification

Serial Windows all-features cohort `7f6e5032-bf1f-4cdf-8f1b-d5eef0ab784e` passed all 242 selected
App/Input/Gameplay/Tilemap tests. Four new concrete-access cases prove retarget recovery with
last-good content, exact pixel coexistence and final-holder retirement, late dependency rejection
after a rejected source, initial source-kind rejection and a missing-resource join failing before
work. Existing TileSet dependency removal/catalog revocation, incompatible atlas successors,
capacity retry and retained-image accounting tests passed unchanged.

Independent review found no actionable change in the Runtime/Editor mutation ordering or the
desired/published state machine. The private join does not transfer Host source authority.

# Host Integration And Limits

Root run `3fa70695-ee5e-4ecb-9953-a013eb0c04bd` passed all 224 tests in its library, composition,
content boot and runtime boot cohort, with tooling/runtime-2d/serde/presentation-ui/project-workflow.
This includes real runtime TileSet publication after Image dependencies, shared atlas validation,
catalog discovery, hidden-Scene Editor dependency retarget/revocation and malformed-source rejection
before a late Image can publish. U15's focused acceptance is complete; the final workspace gate
remains a separate plan-wide requirement.

This feature-minimal cohort also exposed two existing Image preview tests gated only by UI
presentation while calling 2D presentation methods. Their gates now include presentation-2d; the
full Editor/native cohort continues to exercise the relevant supported presentation profile.
