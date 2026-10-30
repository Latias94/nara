---
type: "Work Progress"
title: "Retained local pack ownership replacement and public consumer qualification in progress"
description: "Work Progress for Retained local pack ownership replacement and public consumer qualification in progress."
timestamp: 2026-10-30T18:25:51Z
record_id: "5d5a3464bbcd436581a379fcee22e3e1"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-retained-runtime-pack-plan.md"
git_branch: "refactor/foundation"
git_commit: "c0a8325"
---

# Summary

CP4 selected concrete retained local pack against the qualified eager captured-directory control.
The full foundation Goal remains active. Parent/private backing replacement and independent
consumer tests are in progress, uncommitted on refactor/foundation from c0a8325.

# Details

- ADR0007 and the new design/plan separate immutable expected content/compiled authority from
  encoded backing and decoded/native residency. A file handle does not prove portable byte
  freezing; mutation may reject future reads but cannot retarget an old base.
- `open_pack(FileCapability, expectation, recipe, limits)` consumes the exact file. `write_pack`
  uses a standard Write sink and preserves original catalogue bytes. Concrete version1 is
  NARAPACK/u32LE1/u64LEcatalogue length followed by catalogue and members in admitted order.
- Private admission shares the original target/recipe/Schema/codec checks. Captured mode keeps
  its complete source-removal guarantee. Pack mode retains metadata/file/derived offsets, reads
  one bounded private member buffer, authenticates it and passes that same buffer to the decoder.
  Its WorkBytes charge overlaps decoder/dependency work and drops after bytes. No encoded cache.
- Root reduced-feature production lib check passed. Two too-small lib-test unions hit existing
  cfg omissions in unrelated test modules; the established project-workflow-cli/tooling/
  presentation-2d union compiled and ran the new focused three tests, all passed:
  fb137668-f35f-4700-b128-baf64f77a312.
- Runtime content owner cohort 58/58 passed:
  de5e7a84-0250-46b7-9a79-05ff64cbc1f6. Includes captured controls and exact/one-short pack raw/
  decode/dependency overlap through complete preparation and deep validation. Subsequent test-only
  panic oracle and bounded-read allocation error mapping still need final verification.
- Kuhn supplied pinned Bevy/Godot research and now independently reviews production ownership/
  budgets. Maxwell owns runtime_base/processed_scene_references consumers, still writing. Turing
  finished and closed after Image/TileSet consumer source review; no worker ran Cargo/fmt/commit.
- Turing tests exercise actual pack files, Scene travel, original object versus replaced path,
  member mutation and escaped CPU/prepared pixel lifetime. They are source-ready, not yet run.

# Next Action

Collect remaining source-ready/review, inspect final diffs and freeze writers before fmt. Serial
nextest root target for final owner cohort and four independent consumer binaries, then affected
minimal/all-feature checks. Fix actual failures, qualify complete scope, update guides/ledger/
memory, commit precisely and ff main. Do not push/PR; do not touch vendor or read/write repo-ref.

# Citations

- [Selected design](../../../../architecture/runtime-content-backing-and-residency-design.md)
- [Unit plan](../../../../plans/2026-10-31-retained-runtime-pack-plan.md)
- [Pinned source study](../../../../research/2026-10-31-retained-pack-and-resource-lifetimes.md)
