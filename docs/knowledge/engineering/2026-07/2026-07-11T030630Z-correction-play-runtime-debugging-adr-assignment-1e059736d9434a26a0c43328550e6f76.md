---
type: "Research Findings"
title: "Correction: Play runtime debugging ADR assignment"
description: "Corrects the reserved ADR number and Bevy reference version in the prior research shard."
timestamp: 2026-07-11T03:06:30Z
record_id: "1e059736d9434a26a0c43328550e6f76"
producer_id: "codex-root"
run_id: "2026-07-11-play-runtime-debug-adr-0076"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
supersedes: "f6f7c22c453344a8ab7c9ca7d0eb96ff"
---

# Summary

The prior research conclusions remain valid, but two identifiers were wrong. The Play runtime debug
decision is ADR 0076 because the active foundation plan already reserves ADR 0071 for artifact
publication integrity. The inspected `repo-ref/bevy` snapshot declares `0.20.0-dev`; nara itself
depends on `bevy_ecs = "0.19"`. Findings should refer to the inspected Bevy stepping implementation,
not label the reference tree as Bevy 0.19.

# Details

- Canonical decision path: `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`.
- Reserved future path retained unchanged:
  `docs/architecture/adr/0071-artifact-publication-integrity-and-recovery.md` in U27.
- The stepping analysis was source-based: the inspected `repo-ref/bevy` implementation uses the
  schedule cursor/skip-set behavior described by ADR 0076. Version claims remain separate:
  `repo-ref/bevy` is `0.20.0-dev`, while nara's dependency declaration is `0.19`.
- Persistent replay format work remains gated on U8/U9/U16 plus representative workflow
  size/latency measurements; the research did not authorize an immediate replay file format.

# Next Action

Continue with the U8/M2 identity spike under ADR 0076. Keep the prior shard as immutable history and
use this successor as the current research head.

# Citations

- `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`
- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md` U27
- `Cargo.toml` workspace `bevy_ecs = "0.19"`
- `repo-ref/bevy/Cargo.toml` version `0.20.0-dev`
- `repo-ref/bevy/crates/bevy_ecs/src/schedule/stepping.rs`
