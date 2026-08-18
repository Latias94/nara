---
type: "Verification Evidence"
title: "SGE-U1 data-only authoring schema admission"
description: "Verifies the bounded data-only schema bundle, project manifest declaration, and exact root-private Authoring Project admission receipt."
timestamp: 2026-08-18T16:45:00Z
record_id: "c644e7a78e6d489c9237382f282dfd67"
tags: ["engineering-memory", "verification", "editor", "authoring", "schema"]
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "088259975e7c0037c6656bca6cc38800a950512c"
verified_by: "codex-root"
---

# Verification

SGE-U1 was verified at commit `0882599` from the repository root. Cargo commands ran serially
with `CARGO_BUILD_JOBS=1` and reused the shared target directory.

# Result

`nara_reflect` now owns a bounded JSON-only authoring schema bundle with persistent owner identity,
canonical current and immediate-predecessor catalogs, derived composition identity, aggregate
limits, and no owned-to-static interning path. `nara_project` declares one optional project-wide
authoring bundle path that profiles cannot override.

The root-private Authoring Project Host opens the manifest and bundle through one exact directory
capability, retains immutable source identities, bytes, digests, settings, composition, generation,
and budget usage, and publishes them only by consuming a move-only candidate. The path does not
construct a Runtime plan, executable registry, World, project-content snapshot, or Adapter receipt.

# Evidence

- `cargo nextest run -p nara_reflect --features serde --test authoring_bundle --test-threads=1` -
  5 passed.
- `cargo nextest run -p nara_reflect --features serde --test catalog_format --test-threads=1` -
  6 passed.
- Focused `registry_contract` owner and composition-fingerprint selection - 10 passed.
- `cargo nextest run -p nara_project --test-threads=1` - 38 passed.
- Root-private Host tests for exact receipt publication, aggregate budget rejection, and spoofed
  owner separation - 3 passed.
- Standalone Editor dependency-boundary test - 1 passed and proves its only normal local path
  dependency remains the public root `nara` crate.
- `cargo fmt --all -- --check` and `git diff --check` passed.

# Follow-up

SGE-U2 consumes this exact receipt to decode and preserve bounded Scene and Prefab source without
requiring executable providers. The temporary root-private dead-code allowance is removed when
that consumer lands.

# Citations

- [Standalone Generic Editor Project Open and Recovery plan](../../../../plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md)
- [ADR 0090](../../../../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0114](../../../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
