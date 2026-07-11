---
type: "Decision"
title: "Execution cursor disclosure boundary"
description: "Applies host observation allowlisting, redaction, budgets, and safe source locators to interpreter cursor payloads."
timestamp: 2026-07-11T03:54:00Z
record_id: "417ce2d6031b4bfd9b09c1d175539233"
producer_id: "codex-root"
run_id: "recovery-019f4918-177d-70b1-9694-619ba1df9f2b"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Decision

Optional `ExecutionCursor` held/local-data projections and source-map references are observation
payloads. Before remote transport, logging, or persistence, the host applies the same independent
allowlist, redaction, byte, and depth policy used for component observations. Schema `Inspect`
eligibility alone is not disclosure authorization.

Source-map references use stable program/source identity plus an optional validated
project-relative locator. They do not embed absolute host paths, credentials, unclassified URLs,
or arbitrary source contents.

# Context

Interpreter domains legitimately own program counters and source maps, but those values can expose
script locals, project layout, developer usernames, checkout paths, or secrets even when ordinary
component capture is correctly gated. ADR 0076 originally made cursor payloads bounded without
stating the independent disclosure policy.

# Alternatives

- Treating cursor payloads as implicitly trusted because they originate from an engine domain was
  rejected; the disclosure boundary depends on the destination, not only the producer.
- Removing held-data and source-map support was rejected because bounded, explicitly authorized
  projections are valuable for AI/script debugging.

# Consequences

- Interpreter-domain adapters classify cursor fields and provide stable source identities.
- Tooling filters/redacts cursor data before it enters timelines, remote models, logs, or replay
  records.
- Absolute filesystem locations remain local implementation data and never become stable debugger
  vocabulary.

# Citations

- `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`
- `docs/architecture/adr/0048-runtime-diagnostics-and-observability-bus.md`
- `docs/architecture/adr/0068-global-resource-budgets-metrics-and-diagnostic-privacy.md`
- `AGENTS.md`
