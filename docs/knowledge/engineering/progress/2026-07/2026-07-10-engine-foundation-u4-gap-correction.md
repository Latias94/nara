---
type: "Work Progress"
title: "Engine foundation U4 blocks M1 gate"
description: "Correction: M1 includes U4, whose frame-cleared gameplay queue still violates authoritative tick admission."
timestamp: 2026-07-10T13:47:02Z
record_id: "f20c564eb0b140f68d60d29adee70f0f"
resource: "nara engine foundation"
tags: ["m1", "u4", "gameplay", "commands", "correction"]
status: "active"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "423e0f5"
supersedes: "32157c227cce4c0d9395d5ef83755895"
---

# Summary

This record corrects the superseded M1 gate-ready claim. The milestone table names `U1-U5`, which
includes U4. U4 is not implemented, so M1 cannot yet make a `continue`, `revise`, or `abort`
decision.

# Details

- `crates/nara_gameplay/src/lib.rs` still stores commands in one frame vector.
- Local action commands carry `fixed_tick: None` rather than being assigned to the next
  authoritative tick.
- `clear_gameplay_commands` runs in `CoreStage::Last`, so a zero-fixed-step frame discards a command
  before any fixed system can consume it.
- There is no bounded future horizon/count/byte policy, duplicate or late rejection, deterministic
  `(tick, source, sequence)` admission order, or per-tick exactly-once drain/ack view.
- `docs/architecture/adr/implementation-status.md` correctly leaves ADR 0024 partial and names U4 as
  the trigger. That ledger evidence overrides the mistaken progress shorthand.
- U1, U2, U3, U5, U18, and U25 remain completed; none is reverted by this correction.

# Next Action

Implement U4 from failing zero-step/multi-step characterization tests, update ADR 0057 and the
migration guide, verify `nara_gameplay` plus root/server composition serially, then return to the M1
gate.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/0024-determinism-fixed-update-and-replay-policy.md`
- `docs/architecture/adr/implementation-status.md`
- `crates/nara_gameplay/src/lib.rs`
