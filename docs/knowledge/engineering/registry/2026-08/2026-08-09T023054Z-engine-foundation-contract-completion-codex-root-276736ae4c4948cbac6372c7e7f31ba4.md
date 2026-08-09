---
type: "Work Registration"
title: "UI Foundation correction: UIF-U2C profile and text-work correction active"
description: "Reopens the snapshot/compiler boundary after the contract audit found missing profile identity and failed-layout work-budget bypass."
timestamp: 2026-08-09T02:30:54Z
record_id: "276736ae4c4948cbac6372c7e7f31ba4"
status: "active"
producer_id: "codex-root"
run_id: "uif-u2c"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e0948b5"
supersedes: "e25d8b72ab3d4254a6e72723727816eb"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-08T160401Z-uif-u2-root-envelope-and-order-safe-fallback-5dfa71eb2270485699155b13fb595f07.md"
---

# Scope

UIF-U2C only: exact authoring-profile identity, candidate-wide attempted text-work accounting,
deterministic failed-layout reuse, modal safety escalation, and rejection atomicity.

# Current Claim

The immutable UIF-U2 and UIF-U3 evidence remains historical input, but the 2026-08-09 contract audit
supersedes their completion claims for the affected paths. UIF-U2C is the sole active unit. UIF-U3C,
UIF-U4C, UIF-U5 and both product successor plans remain inactive. Commit `e0948b5` preserves the
valid endpoint-retirement and accessibility work already completed during the started UIF-U4 unit.

# Latest Links

- docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md
- docs/architecture/adr/0109-root-local-ui-compilation-and-presentation-closure.md
- docs/knowledge/engineering/verification/2026-08/2026-08-08T160401Z-uif-u2-root-envelope-and-order-safe-fallback-5dfa71eb2270485699155b13fb595f07.md

# Handoff

Make `UiAuthoringSnapshot` retain the exact root/source-node/depth/key profile used by its builder and
reject compiler mismatch before any revision, cache or LRU mutation. Reserve candidate text input
work before each unique shaping attempt, cache deterministic failures within the candidate, and keep
successful shaped-glyph output accounting separate. Prove mismatch in both directions, repeated and
distinct failed layouts, modal escalation, and last-good/revision/cache preservation. Do not start
UIF-U3C presentation-authority work until this unit publishes immutable completion evidence.

# Citations
