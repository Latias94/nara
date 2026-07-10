---
type: "Work Progress"
title: "Engine foundation M1 implementation units ready for gate"
description: "U1, U2, U3, U5, U18, and U25 are committed; the sequential low-memory M1 decision gate is next."
timestamp: 2026-07-10T13:29:35Z
record_id: "32157c227cce4c0d9395d5ef83755895"
resource: "nara engine foundation"
tags: ["m1", "lifecycle", "time", "tasks", "diagnostics", "filesystem"]
status: "active"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6a70847"
---

# Summary

The implementation units required by the M1 runtime-safety gate are committed. U1 established ADR
governance; U2 contains plugin lifecycle failure; U3 makes fixed-frame advancement atomic; U5 owns
bounded task admission and finite shutdown; U25 provides capability-oriented filesystem authority;
U18 now owns bounded privacy-safe diagnostics and pressure snapshots. The next action is the gate,
not another implementation unit.

# Details

- U1: `164feb4` defined the completion plan and `0c6fac5` separated accepted decisions from
  implementation evidence.
- U2: `2867235` made plugin setup/finish failure terminal, cleanup fallible/reverse/once-only, app
  mutation fallible, and built-in component registration contextual.
- U3: `a4167e4` validated time configuration, advanced fixed time per authoritative tick, and made
  failed frame planning atomic.
- U5/U25: `e77da64` added bounded threaded task pools, typed terminal outcomes, cooperative
  cancellation, finite shutdown, ordered integration, and host-issued filesystem capabilities.
- U18: `6a70847` added static validated diagnostic identities, safe summaries, classified fields,
  sticky bounded reports, indexed runtime retention/dedupe, tracing cursors, and separate bounded
  pressure snapshots; project/asset/scene/tooling/facade callers were migrated without shims.
- U18 verification is recorded in
  `docs/knowledge/engineering/verification/2026-07/2026-07-10-u18-diagnostic-privacy-core.md`.
- The host previously exhausted memory during a broad combined nextest run. Subsequent evidence is
  intentionally sequential with `CARGO_BUILD_JOBS=1`, one package or test target, and one test
  thread. Unknown Cargo/Codex processes must not be terminated.
- M1 is still open until the cross-unit gate records a continue/revise/abort decision. U18's runtime
  producer bridges remain U31 and do not block the U18 core result.

# Next Action

Run the M1 verification matrix one crate/target at a time for ADR docs, `nara_app`, `nara_tasks`,
`nara_fs`, `nara_diagnostic`, and root composition. Recheck formatting, the locked workspace, stale
symbols, and capability invariants. If all load-bearing contracts remain green, record a `continue`
decision and start Wave C/M2 work; otherwise revise the owning ADR before downstream APIs expand.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `docs/knowledge/engineering/verification/2026-07/2026-07-10-u18-diagnostic-privacy-core.md`
