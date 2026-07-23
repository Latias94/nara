---
type: "Verification Evidence"
title: "RGD-U7 Runtime and Host authority verification"
description: "Records the independent refreshed Runtime review, conditional Host/compatibility review, and bounded acceptance of ADR 0084 and ADR 0082."
timestamp: 2026-07-23T08:12:41Z
record_id: "38a8bf4d48614a829bdd6388f02c9446"
tags: ["rgd-u7", "runtime", "host", "adr", "verification"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "eb9698628fd9c4688b95deaed01259bd6b0249aa"
verified_by: "Independent Runtime review, conditional independent Host/compatibility review, exact staged-scope audit, rustfmt, and document consistency inspection"
---

# Verification

RGD-U7 was completed at `eb9698628fd9c4688b95deaed01259bd6b0249aa`. It re-reviewed
ADR 0084 first against the refreshed U2-U6 implementation evidence. Only after that review
accepted Runtime authority did the independent Host/compatibility review evaluate ADR 0082 and
the pair.

# Result

- The independent Runtime review found no P0/P1 and accepted ADR 0084 for one thin lifecycle
  owner around one `App` in the already-compiled, Host-trusted scope.
- The independent Host/compatibility review found no P0/P1 and accepted ADR 0082 plus the
  compatible pair. Concrete product Hosts publish only through their owned publication slot.
- The decision matrix preserves the direct code-first boundary: `promote()` may retain a faulted
  owner for observation and close, while a product Host uses `publish_into` and never exposes a
  runnable session after the final fault race.
- Neither acceptance introduces an `EngineHost`, service hub, `RuntimeFactory`, universal Runner
  trait, replacement Render Host, package/native-code activation path, or shared process-service
  model.
- The architecture catalogue, implementation ledger, foundation ownership map, and future
  governance assertion now point to the new immutable U7 matrix rather than editing the historical
  RGF-U23 decision.

# Evidence

- The matrix pins the reviewed source baseline
  `5ebc45e287c94dac99f194aa921adaf5086cc8a2` and exact U2-U6/RGF-U24/RGF-U26 revisions.
- The Runtime reviewer evaluated startup publication, ownership handoff, admission, scheduled
  execution, driver parity and authority, exact stepping, fault closure, isolation, finite close,
  stop-first replacement, API authority, and early ownership value. Every metric passed.
- The Host reviewer evaluated pre-mutation rejection, recipe coherence, fresh preparation, early
  topology value, Runtime delegation, parent lifetime, cross-Host parity, least privilege,
  embedding, and the only currently admitted external authority role. Every metric passed.
- The combined matrix records passing sequential/overlapping Runtime, process-contention,
  reconstruction, and plan/World behavior-registry scenarios without claiming identical internal
  frame calls across Hosts.
- `rustfmt --edition 2024 --check tests/architecture_docs.rs`: passed.
- `git diff --cached --check`: passed before the decision commit. The commit scope contains only
  the five accepted-authority documents, their catalogue/ledger/foundation reconciliation, the
  new immutable matrix, and the governance assertion update.
- `architecture_docs` was intentionally not executed under the user's instruction. This unit is a
  documentation-only decision backed by the cited executable U2-U6 verification; the test source
  was updated for future governance runs, not used as new executable evidence.
- No Cargo build or test ran for this decision-only commit, avoiding unnecessary shared-workspace
  CPU and memory pressure.

# Remaining Boundary

U7 removes only the Runtime/Host authority blocker. It does not close hosted CI, first-playable
measurement, standalone candidates, pre-publication evidence, or release publication. Any later
Rust, Cargo, policy-test, or workflow repair still invalidates U8; a repair that changes U2-U6
evidence also requires the affected review and U7 decision to be refreshed.

# Follow-up

Activate RGD-U9 for local measurement helper, policy, and protocol preparation only. U9 cannot
execute or close its baseline until U8 has final hosted Windows/Linux CI evidence on the integrated
revision.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u7-re-review-runtime-then-host-authority`
- `docs/knowledge/engineering/decisions/2026-07/2026-07-23T074018Z-rgd-u7-runtime-and-host-independent-decision-matrix-e2e5ea1ed4cf4e28860cedb32f0e7e48.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- Commit `eb9698628fd9c4688b95deaed01259bd6b0249aa`
