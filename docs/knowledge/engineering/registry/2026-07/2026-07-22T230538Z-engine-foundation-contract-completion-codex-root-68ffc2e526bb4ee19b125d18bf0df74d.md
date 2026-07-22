---
type: "Work Registration"
title: "Reference-game runtime authority and delivery: RGD-U6 complete"
description: "Completes the renamed-root external managed-runtime Runner proof without a Nara Runner SPI."
timestamp: 2026-07-22T23:05:38Z
record_id: "68ffc2e526bb4ee19b125d18bf0df74d"
tags: ["rgd-u6", "runtime", "external-runner", "completed"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "12696d45b167cb4b8f0cb9ed61060f8b9cda7dae"
supersedes: "5fffaafa60444261b1b1db9e01b669f8"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/knowledge/engineering/verification/2026-07/2026-07-22T230538Z-rgd-u6-external-managed-runtime-runner-verification-5c2044aca8cd4eaf81087d08c7c75e40.md"
---

# Scope

RGD-U6 proves that an external Rust package with an independent lockfile can rename the public
root dependency and own one concrete `RuntimeInstance` loop through public Nara APIs.

# Current Claim

Completed at `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae`: reservation, code-first App setup,
seal/admission, pause, exact step, fault observation, and finite retirement are reachable without
a Nara-owned Runner trait, factory, registration key, provider catalogue, hidden driver port, raw
App driving, runtime World mutation, or private crate dependency.

# Latest Links

- docs/knowledge/engineering/verification/2026-07/2026-07-22T230538Z-rgd-u6-external-managed-runtime-runner-verification-5c2044aca8cd4eaf81087d08c7c75e40.md
- docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md

# Handoff

The next admitted unit is RGD-U7. Reconcile the current architecture-document authority and
implementation ledger before deciding ADR 0084; the U6 fixture is execution evidence, not a
pre-accepted Runtime or Host topology verdict.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u6-prove-a-renamed-dependency-external-runner`
- Commit `12696d45b167cb4b8f0cb9ed61060f8b9cda7dae`
