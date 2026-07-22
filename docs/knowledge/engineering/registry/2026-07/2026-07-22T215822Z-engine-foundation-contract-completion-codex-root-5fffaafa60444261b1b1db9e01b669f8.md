---
type: "Work Registration"
title: "Reference-game runtime authority and delivery: RGD-U6"
description: "Prove a renamed-dependency external managed-runtime Runner without a Nara Runner SPI."
timestamp: 2026-07-22T21:58:22Z
record_id: "5fffaafa60444261b1b1db9e01b669f8"
tags: ["rgd-u6", "external-runner", "runtime", "fixture"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0c2dadd849e08d5b3f22dcedac963bbfccf08595"
supersedes: "e1709d47de9c4e36be654c755f99dfaa"
registration_id: "engine-foundation-contract-completion-codex-root"
source_workspace: "F:\\SourceCodes\\Rust\\nara"
latest_link: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
---

# Scope

RGD-U6 proves an independently locked external Rust package can rename the root dependency and own
a concrete `RuntimeInstance` loop exclusively through public Nara APIs.

# Current Claim

Active: the fixture must cover reservation, code-first App setup, seal/admission, pause, exact step,
fault observation, and finite retirement without a Nara-owned Runner trait, factory, registry, hidden
driver port, raw App driving, runtime World mutation, or private dependency.

# Latest Links

- docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md
- docs/knowledge/engineering/verification/2026-07/2026-07-22T215822Z-rgd-u5-real-three-host-parity-verification-481fd3d8ac1e43b09fc91d38f47062da.md

# Handoff

Start from the schedule-extension renamed-root fixture's metadata/source audit. Keep the external
fixture outside the root workspace, let it retain its own lockfile, and create a test-owned bounded
result only after the public loop has reached a truthful terminal state.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u6-prove-a-renamed-dependency-external-runner`
- Commit `0c2dadd849e08d5b3f22dcedac963bbfccf08595`
