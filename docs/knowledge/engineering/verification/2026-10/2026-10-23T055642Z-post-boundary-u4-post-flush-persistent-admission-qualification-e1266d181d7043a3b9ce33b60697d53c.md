---
type: "Verification Evidence"
title: "Post-boundary U4 post-flush persistent admission qualification"
description: "Post-flush target facts preserve original asset interference guards across direct batch and fresh preparation."
timestamp: 2026-10-23T05:56:42Z
record_id: "e1266d181d7043a3b9ce33b60697d53c"
producer_id: "codex-post-boundary"
run_id: "post-boundary-20261023"
related_plan: "docs/plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "53a62bf"
---

# Verification

U4/R4 changes only persistent preparation and its tests, with ADR 0081 clarification. The same
commit owns the source delta over `53a62bf`; the baseline is not claimed to contain that delta.

## Results

- The worker added eight regressions before implementing the fix but ran no Cargo. No fresh red
  execution is claimed for U4.
- The serial Tasks/Reflect/Scene/Winit library cohort passed 312 tests, four ignored.
- The subsequent full-feature root/Tasks/Reflect/Scene/Winit library cohort passed 670 tests, five
  ignored, including actual Scene/Prefab persistent consumers and Host source/persistence paths.
- Direct and batch queued-first-apply tests pass, as do both binding-conflict directions, original
  asset mutation during flush, support topology revalidation, fresh support existence and queued
  support-hook rejection. No asset-baseline recapture admits the interfering write.
- Independent read-only U3/U4 review found no blocking implementation defect. It inspected actual
  Scene/Prefab calls without claiming to execute them. Workspace formatting and scoped diff checks
  passed before the unit commit.

## Limits

The initial combined attempt stopped on a stale R1 expectation in a Reflect Runtime-scope test;
that test now observes the post-operation fault envelope, and the complete cohort was rerun.
The result does not establish arbitrary callback rollback, other-platform native behavior or
performance. Native and public-driver integration evidence belongs to U1's separate record.

## Sources

- [Plan](../../../../plans/2026-10-23-1305-refactor-post-boundary-contracts-plan.md)
- [ADR 0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Owner and regressions](../../../../../crates/nara_reflect/src/persistent_apply.rs)
