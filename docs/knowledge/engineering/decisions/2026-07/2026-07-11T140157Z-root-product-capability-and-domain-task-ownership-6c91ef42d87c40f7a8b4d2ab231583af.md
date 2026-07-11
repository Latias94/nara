---
type: "Decision"
title: "Root product capability and domain task ownership"
description: "ADR 0079 and ADR 0080 align compiled product ceilings, preflight composition, placeholder retirement, and domain-owned TaskUpdate integration."
timestamp: 2026-07-11T14:01:57Z
record_id: "6c91ef42d87c40f7a8b4d2ab231583af"
resource: "nara engine foundation"
tags: ["architecture", "cargo", "plugins", "tasks"]
status: "accepted"
producer_id: "codex-root"
run_id: "goal-019f5096"
source_session: "019f4f36-42c9-7043-92b5-661311b14e21"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "94530a1"
---

# Decision

Adopt ADR 0079 and ADR 0080 as the owning contracts for U32 and U33.

- Root Cargo features are coarse compiled product-capability ceilings. The resolved plugin plan's
  required product capabilities must fit the normalized project request, which must fit that
  ceiling. Plugin service requirements/conflicts close separately before any `App` mutation.
- `default` becomes `runtime-core`; serde weak-forwards only into enabled domains; the placeholder
  audio crate is retired until a real vertical slice satisfies crate-admission evidence.
- `nara_app` retains only `CoreStage::TaskUpdate`, `nara_tasks` retains execution mechanics, and
  `nara_asset` owns Poll/ResolveSourceChanges/SpawnJobs/ApplyResults.
- Each poller captures one immutable ready membership or queue prefix at system entry. Eligible
  predecessor-unblocked outcomes in that snapshot and eligible synchronous SpawnJobs outcomes must
  apply in the same frame; stale/superseded outcomes retire, while later-ready or eligible
  missing-predecessor work waits under the domain contract.

# Context

At commit `94530a1`, a no-default root tree still compiled 24 direct engine crates, project
composition could mutate settings before discovering a missing backend, `nara_audio` had no
production consumer, and asset-specific task phases were owned by `nara_app` and configured by
`nara_tasks`. The committed `nara_identity` core crate existed in the workspace, but the root
package dependency, facade, and consumer migration were still open.

# Alternatives

- Keep mandatory root dependencies and the global task set. Rejected because compile cost remains
  false, capability failure remains partially mutating, and foundational crates retain asset policy.
- Expose one feature per crate and one generic Poll/Spawn/Apply chain. Rejected because crate layout
  is not product vocabulary and unrelated domains do not share one proven frame-boundary contract.
- Add an internal aggregator and central task coordinator. Rejected because it adds shallow
  ownership without fixing product preflight or domain scheduling authority.

# Consequences

The plan gains U32 and U33 after U8/U5 prerequisites. ADR 0008 and ADR 0052 return to `partial`
until U33 lands. ADR 0035, ADR 0055, ADR 0056, and ADR 0070 gain explicit U32/U17 gaps. Public
feature/profile/task-set removals require one migration-guide update and scoped stale-symbol checks.

# Citations

- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/architecture/adr/0080-domain-owned-task-update-integration-sets.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/knowledge/engineering/2026-07/2026-07-11T114131Z-root-capability-task-ownership-and-manifest-io-audit-d3b79814f13b4bc3980973c209bf1e72.md`
