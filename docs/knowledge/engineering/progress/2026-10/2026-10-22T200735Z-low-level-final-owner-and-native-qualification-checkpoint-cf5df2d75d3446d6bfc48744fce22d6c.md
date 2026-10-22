---
type: "Work Progress"
title: "Low-level final owner and native qualification checkpoint"
description: "U5 and U10 committed; all final owner and five native trajectories pass, U4 costs and integration closure remain."
timestamp: 2026-10-22T20:07:35Z
record_id: "cf5df2d75d3446d6bfc48744fce22d6c"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "fb6af18"
---

# Summary

Goal remains active. Do not restart session recovery or planning. U1/U2/U3/U6/U7/U8/U9 and U10
are committed; U5 is now `fb6af18`, U10 `cfde813`. U4 source is final and independently reviewed
without blockers; not yet committed. All workers/reviewers from this phase are closed.

# Verified State

- Final Asset/Image/Watch/Tilemap/Hierarchy/Transform all-feature libraries: 329/329,
  `3bb6abef-0e57-4bf0-b7dd-160cf4d191b6`. The previous 328/329 run exposed a new test's
  fixed-two-drive pollable shutdown assumption; bounded five-second owner polling repairs the test,
  no production shutdown change. New atomic publication spelling uses try_update, not deprecated
  fetch_update. Core SF5 and asset AF4/AF5 review findings are fixed and independent rereviews closed.
- Final Windows/DX12 Rust 1.95.0 five native trajectories: 5/5,
  `7552d87a-bd13-4c48-bed0-83e103048dcb`, 76.896 s, supervised Play 59.820 s. Includes ordinary
  Save/Reopen, paused cadence, input/resize/Step, views detach/reopen/recovery and real supervised
  Cargo last-good/successor/physical exit. Earlier run also passed 5/5 but predates final repairs.
  Editor generation-store15/15 `cbb91eca-37be-48da-a6b6-8b4392c82b6b`.
- Independent module workspace77/77, existing skips2, `0cd32434-1ad3-4c7c-bae7-4a997d585eb9`.
  Two-level Save7/7 `f222687b-fd93-4b4f-a151-5c1e424a1072`; Collect gameplay4/4
  `e0ef2025-d93e-4d83-a9c5-e48f0ae06ea8`; renamed schedule9/9
  `3506b29b-51d6-4a9c-a47d-573372dc5542`.
- U4 optimized owner/render cohort191/191 `8edcd9d4-e1b0-4455-93ae-85a90cd229c6` before SF5;
  SF5actual slot/history restore regressions pass in final329. APIs remove Global Component;
  snapshots private, Historical/Fixed/Presented readers, nonspatial direct parent ends inheritance.
- Before/initial/final-cost files under ignored root target/low-level-refactor. Report and selected
  summaries are new docs/benchmarks/2026-10-23-low-level-spatial-cuts.md and data directory. Final
  measurement predates only SF5 atomic allocator; repeat timing/allocation on final source next.
  4096 P50 before→optimized: static18.5→39.7us, leaf159.3→177.4,root178.7→178.0,
  interp419.6→344.0,four181.8→230.0. Zero steady allocations; sparse retained requested+475600B.

# Open Threads

Current sole Cargo session90228: full headless Reference Game, root target, Rust1.99, onejob:
`cargo nextest run --locked --manifest-path reference-game/Cargo.toml --test-threads=1 --no-fail-fast`.
Poll it before any Cargo. Native tests finish, no remaining workers. Other repos may run cargo;
do not stop them. All remaining dirty tracked files are coordinated implementation/docs or known
own cargo-fmt formatting. Preexisting untracked vendor/ remains untouched.

U5 staged exact source and was committed by index-plumbing only for src/lib.rs Asset export and
ledger/migration U5 rows. Remaining src/lib.rs diff is U4 exports. cargo fmt --all plus Reference,
Collect and schedule fixture fmt applied; it formatted previously committed own ECS/Identity/Gameplay
lines (known only whitespace, reviewed diff) and these should be included in final tidy commit.

# Next Action

1. Finish Reference full headless; run desktop_render feature/target (actual child native GPU probe).
2. Final U4 release timing/allocation with absolute new output path, retain separate prior process;
   adjust report/evidence rather than pool.
3. Fresh root library/content boundary/refresh/Check/Export/Workspace Play relevant tests after final
   Asset correction. Existing earlier root evidence passes but predates final repair.
4. Scoped lint/check + Transform negative doc and fmt checks serial. Existing dependency/root
   dead-code and known App/Asset lint baseline stays explicit; fix new lints, don't expand cleanup.
5. Update final ledger/docs/report/evidence, commit U4 exact source/consumers then integration docs
   and own formatting. Goal requires committed source coverage and no unresolved blocker.
6. Validate/render memory; final status registration successor and actual Goal completion only after
   all done criteria. Do not repeat native acceptance for documentation-only closure.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Review follow-ups](../../../../audits/2026-10-23-low-level-refactor-review-followups.md)
- [U4 evidence](../../verification/2026-10/2026-10-22T194451Z-u4-immutable-spatial-cuts-and-exact-hierarchy-publication-4023754775ba445a887c9056715584e0.md)
- [U5 evidence](../../verification/2026-10/2026-10-22T193928Z-u5-asset-association-desired-recipe-and-runtime-owned-watch-rescan-e081ec487db94262b392fc8b7d7ff3ef.md)
- [U11 evidence](../../verification/2026-10/2026-10-22T193724Z-u11-low-level-refactor-public-and-native-consumer-integration-ddb983fa80454657955de17ff65d1772.md)
