---
type: "Subagent Finding"
title: "Complete foundation review: six independent lanes and integrated design choices"
description: "Six source reviews challenge all conversation foundation topics and 103 baseline ADRs without claiming implementation."
timestamp: 2026-10-23T11:38:40Z
record_id: "0520f18c9f084c319ab268ab36204938"
producer_id: "codex-foundation-integration"
run_id: "foundation-rebaseline-20261023"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "9fafdac"
---

# Finding

The preceding nine-owner-finding review was not a complete architecture reassessment. Six new
read-only lanes reconsidered the supplied 3,609-line conversation and all 103 baseline ADRs,
including future extensions and named author workflows rather than assuming Accepted means optimal.
The resulting 36-topic coverage and D01-D18 design replace the earlier overly broad suitability
inference, not its narrowly scoped probes.

The recurrent structural problems are representation/cardinality, extension execution rights,
logical-versus-physical lifetime and observation authority. They justify design changes without
implying that Bevy ECS, document truth or domain ownership should be discarded.

# Evidence

All six lanes reported source/contract review only: no tests, performance, production changes or
new platform qualification. Bevy reference `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` are pinned comparison mechanisms, not latest/compatibility
claims. The actual released ECS dependency remains separately pinned.

| Lane | Agent | Scope and distinctive result |
| --- | --- | --- |
| A | `01a16873-06e6-78e2-a85d-4c037fdaa7e8` | 20 ADRs; automatic phase rights, facilities/scopes, completion dependencies, repeatability and motion authority |
| B | `01a16873-6d03-72e3-be17-873ebf0a607e` | 19 ADRs; exact native binding cannot represent multiple dynamic Schemas through one carrier without a new complete proof |
| C | `01a16873-bc68-70c2-b226-b47294ec8f4d` | 15 ADRs; authored product continuity must survive cache deletion; build Host/target and cyclic admission require separation |
| D | `01a16874-0772-7863-950b-dfc40a89ac85` | 17 ADRs; immutable packets do not require closed render topology; GPU Host and typed Features are distinct |
| E | `01a16874-4b2b-7ce0-ab08-10658676ec32` | 9 ADRs; source-built, semantic isolated and trusted binary placement are real alternatives; optional language UX is a current design input |
| F | `01a16874-89b0-78d1-ab61-c3537ca14ab4` | 23 ADRs; prepared document edits, selection-independent tools and independent UI products; minimal semantic tracer needs no new toolkit |

The six review agents were closed after their reports. Three later disjoint documentation
workers reconciled extension/OQ/package scope, Editor/UI scope and identity/artifact/delivery
scope. Their documentation changes are integrated, not independent production implementations.

# Recommendation

Retain native Bevy APIs and complete native binding safety. Replace reflection-as-authority now
because actual providers already use Nara Schema without Reflect. Compare owner-managed rows
against explicit attachment envelopes, not a duplicate-TypeId exception. Dynamic Bevy descriptors
are a bounded presence/query counterfactual, not a mandatory new ECS.

Prefer immutable owner publications, typed resource facets, a bounded render dependency plan and
Host-owned scarce resources as directions; validate their public interfaces with the same real
consumers and complete retirement. Keep whole-Runtime/process restart as an honest fallback.
Do not unify all scheduling, services, scripting, UI or operation results behind one generic kernel.

Research gates must not require historical plan IDs, a complete game or full Nara UI for every
experiment. Portable compatibility freeze still requires real semantic variation, not two fakes.

# Disposition

The integrated inventory records 63 Amend, 17 Narrow, 12 Keep, 6 Replace and 5 Historical
recommendations. Only specifically edited contracts change authority; recommendations are not a
blanket implementation or adoption. Accepted ADR 0119 supersedes 0004. New 0120/0121 stay Proposed.
Current native registry and stock renderer implementations were not relaxed by this documentation.

The private export is not copied into the repository; its digest is recorded in the coverage
inventory. Missing exported code blocks were not reconstructed. No Goal or token budget was
created during this phase. R1-R12 are executable follow-up units, not completed work.

# Citations

- [Complete coverage and ADR dispositions](../../../../audits/2026-10-23-foundation-architecture-coverage.md)
- [Integrated alternatives and recommendation](../../../../architecture/foundation-architecture-reassessment.md)
- [Fearless refactor execution plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Metadata authority](../../../../architecture/adr/0119-schema-first-semantic-metadata-authority.md)
