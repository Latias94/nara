---
type: "Work Progress"
title: "Tracked local artifact execution and complete-group store implementation in progress"
description: "Work Progress for Tracked local artifact execution and complete-group store implementation in progress."
timestamp: 2026-10-30T21:30:10Z
record_id: "610ba9175c1b41bebdaa3f8f0c86bd79"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-local-artifact-build-plan.md"
git_branch: "refactor/foundation"
git_commit: "ec7fe1b"
---

# Summary

The finite local-build replacement is integrated as uncommitted work on `refactor/foundation`
at `ec7fe1b`. The full foundation Goal remains active. Source recipe and pure CPU Image cooking
are qualified predecessors, not proof of the new durable execution/store workflow.

# Details

The optional lower `SourceAssetBuild` facet preserves source-only settings ergonomics. Root
`asset_build` selects concrete typed execution and Image adapters, tracks current raw/presence
and processed observations, and consumes exact normalized metadata. Seed, closed recipe, output
content and authored Product identities remain distinct. The local store keeps separate seed
hints and successful recipe associations, verifies complete captured bytes, and publishes blobs,
manifest and then one index. Post-index callback errors/panics retain an uncertain receipt.
Each store owns its budget; clones/escaped groups retain that original owner. A proposed global
root/budget registry was rejected and removed before integration.

Independent static store review found no confirmed P1/P2; execution and Image review remain
separate. Two real `AssetPath::meta_path()` String/AssetPath compilation mistakes in parent-owned
execution were corrected without changing the global path API. Root `project-workflow` library
checks pass, including the newly declared concrete Image adapters. The first root all-feature
store-test build is in progress; no R6b test or native success is claimed here.

The newly supplied research ZIP is the same already-imported package: all 91 manifest entries
match their recorded lengths/SHA-256 values, and all ten session research documents match the
current imported files byte-for-byte. Supporting snapshots were not copied over current owners.
An independent current-plan omission review is running; no duplicate omnibus plan is introduced.

# Next Action

Finish independent reviews, run serial store/compound/Image public consumers including actual
interruptions and fresh-process reuse, resolve owning failures, format only frozen write sets,
and record matching evidence before a precise commit/local main fast-forward. Keep general
dependency scheduling, complete target delivery and the remaining foundation waves open.

# Citations

- [Local build plan](../../../../plans/2026-10-31-local-artifact-build-plan.md)
- [Selected build design](../../../../architecture/local-artifact-build-design.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [Qualified prerequisites](../../verification/2026-10/2026-10-30T201802Z-source-import-recipes-and-target-sensitive-cpu-image-cook-prerequisite-qualification-462c0179d20b44fd8545d4c135e814f9.md)
