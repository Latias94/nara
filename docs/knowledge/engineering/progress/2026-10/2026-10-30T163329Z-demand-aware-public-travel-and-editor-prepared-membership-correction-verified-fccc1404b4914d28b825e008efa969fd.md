---
type: "Work Progress"
title: "Demand-aware public travel and editor prepared-membership correction verified"
description: "Work Progress for Demand-aware public travel and editor prepared-membership correction verified."
timestamp: 2026-10-30T16:33:29Z
record_id: "fccc1404b4914d28b825e008efa969fd"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-30-reference-validity-and-load-demand-plan.md"
git_branch: "refactor/foundation"
git_commit: "1ddf670"
supersedes: "e2a98fb4e3ef490d83d93fd3b900f655"
---

# Summary

The complete foundation Goal remains active. Main remains `8e175db`, refactor/foundation remains
`1ddf670`; the entire demand-aware production unit is uncommitted. No push or PR. All implementation
and review agents have been closed after source-ready integration. Preserve vendor and references.

The demand-aware Runtime/source/Authoring implementation now has passing focused and independent
workflow evidence, but final affected regression/native qualification and fixture generation are
not complete. Do not treat this record as the complete unit's qualification or merge authority.

# Verified Results

- Asset/Input/Image all-features 464 passed, one existing skipped:
  `1dad5df6-0700-4793-aec5-59631c7b3a5a`.
- Root first focused run 61/64, three failures:
  `75e3daa7-3d7d-404d-bd3c-089c6d4865d9`. Six prepared discovery/order/once-decode/budget/deep-drop
  tests passed. Two Source fixtures lacked a real declared bundle, and Authoring Deferred to
  Immediate attempted a duplicate record insertion. Source fixtures now generate the current
  actual plan bundle; the owning Authoring helper borrows and reuses only completely equal
  records, with original conflict rejection and pre-allocation accounting preserved.
- Source/Authoring/catalogue/deep-growth corrected focused selection 30 passed:
  `2d0bda27-7708-4ff8-a04e-f6dacf1975ca`. The two deep growth tests cover old4/new8 plus live decoder
  work, one-short refusal without mutating the vector, retry and complete refund.
- Independent processed public Scene workflow 10 passed:
  `e0929993-e620-418a-b83c-2a2435b5cbe3`. Deferred A-B-C travel, shared atlas, heavy B-only Image/CPU,
  Immediate data-only D, actual Ready/cancel/commit, malformed B preserving A and exact dependency
  failure paths all execute under fixture-isolated cross-thread observations. A startup sees
  atlas/heavy/CPU decode counts [1,0,0]. A later deep-failure counter assertion was added before
  reset and was part of the actual test source at compilation.
- Scene/Tilemap all-features 891 passed, three existing skipped:
  `ae7b2084-8df2-44f5-ba4e-cfdbcde7dfe8`. Includes the two new public production Prefab composition
  tests for defaults, property pins, discarded references, Absent and identity rewrite demand.

An independent source review found a real P2 in Editor startup: a new Immediate reference could
resolve a previously Deferred-only native identity without its prepared data. A real red oracle
failed against the old owner helper (`4be4beab-71d1-4a67-a1e3-15cf86852d47`). The snapshot owner now
checks actual Scene root/dependency, Image and TileSet membership by canonical source path and
refuses with `project.content.asset-not-prepared` when a new capture is needed. Concrete domain
Product membership and native auxiliary rejection remain intact. The Scene/Image/TileSet regression
also checks original source reusability and all budgets returned after refusal. Final Source six
tests pass (`89190481-d001-484c-bfad-2c6c7f0b56ea`). Limited independent rereview accepted this guard;
an independent Runtime adversarial review reported no confirmed P1/P2 within its stated scope.

# Integration Details

Frozen Source receipts retain captured facts rather than live filesystem authority. A later load
or request resolves current targets. Authoring separately owns narrow live metadata/file/parent
observations. No new watcher or FileIdentity field was added to the frozen receipt.

Native TileSet test integration was corrected without weakening exact-set admission: a silent
inventory codec exercises the independent descriptor Image-edge defense; a separate correct-
inventory/lax-identity codec reaches singleton cardinality. New deep checks are added to those
consumer cases, but that binary has not yet been rerun. Source malformed target native membership
correction means relevant Editor/Trusted Play regressions need coverage.

Actual schema producers regenerated Reference Game (`authoring_bundle --write`), the image-import
fixture (Reference headless `schema-write`) and collect-game (its headless `schema-write`). The
generic fixture also acquired a formerly stale stock Tilemap declaration from the actual recipe.
The proposed stock-only temporary example safely refused the external-owner fixtures and has
been deleted. Do not retain it as a migration tool. Behavior-language authored catalog explicitly
declares Immediate. Remaining canonical fixtures: two-level-game and renamed-root project.
The renamed-root lib currently contains a TEMPORARY `regenerate_reference_demand_schema_bundle`
test that invokes its actual private recipe; execute once, then remove that entire test with
apply_patch before delivery. Never commit it as a source-mutating ordinary test.

Root and independent consumers were formatted after all workers froze; parent later formatted
the prepared-membership correction. No fmt rollback. No compile-fail goldens were rewritten
apart from the six new asset-demand cases.

Cargo remains serial, jobs one and root target. No Nara Cargo is running at this record boundary.
F disk fell to about6.3GiB after root relinking. A first transparent NTFS LZX helper89556 was
explicitly identified and stopped after reaching34.2GiB. New artifacts then consumed space.
Second helper69100 is currently compressing only resolved `target/debug/deps`, with hidden window,
stdout/stderr under TEMP; do not run Cargo until stopping this exact helper after checking its
command line. Stop at adequate free space (about35-40GiB), do not kill another process. No
artifacts were deleted. Other repositories may run Cargo independently.

# Next Action

Finish compression, generate remaining two-level and renamed-root canonical bundles through
their actual recipes, remove temporary renamed-root test. Run final root all-feature lib
regression, independent runtime_base/processed_host/Scene/Image/TileSet/source consumers,
appropriate correctness Clippy/minimal checks and selected actual Windows/DX12 retained Scene
and Canvas workflows. Stock native publication defenses and actual Check/Export remain needed
before qualified commit. Respect disk capacity without another target directory or broad clean.

Update owning guides/ledger with actual evidence, review complete diff and exact filenames,
Conventional Commit and local ff main only after the coherent unit is qualified. Full foundation
Goal stays active; CP4 backing/residency, Schema evolution, motion/execution and custom UI/render
remain distinct ordered work.

# Citations

- [Selected design](../../../../architecture/reference-validity-and-load-demand-design.md)
- [Unit plan](../../../../plans/2026-10-30-reference-validity-and-load-demand-plan.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
