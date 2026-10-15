---
type: "Subagent Finding"
title: "Host close obligations must enter the runtime owner before callbacks"
description: "U5 review found two unwind windows in ordinary startup and staged admission; corrections and red regressions are pending."
timestamp: 2026-10-15T10:43:37Z
record_id: "51473f488c09410c8db5bb9f5579070a"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/mods_plugins"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Finding

U5's same-owner field move alone did not close two callback-before-transfer windows. Host close
obligations must be transferred before any operation that can unwind, not merely before a returned
error is classified.

# Evidence

In RuntimeInstance::start_inner, the external ledger remained a local variable while
App::seal_internal invoked a configured authority validator. A panic retains the App's owner but
drops the external ledger without beginning its participants. RuntimeCandidate::admit_reserved
similarly bound the managed handler and materialized/flushed observers before merging its external
ledger; an observer hook panic could lose the same physical responsibility.

PluginPlan::instantiate_runtime_candidate already installs the Host ledger before plugin callbacks
and is the correct ownership precedent. Read-only review by /root/mods_plugins identified these
reachable paths; no new validation or general panic handler is necessary.

# Recommendation

Merge all obligations into the existing owner before seal/bind callbacks. Preserve Host-first
order, retain both physical participants on duplicate-ID rejection using the existing retirement
append path, and keep the typed admission failure. Add real validator/observer panic regressions
that observe begin/drop counts and drain the retained quarantine owner after catching the unwind.

# Disposition

Assigned to /root/u5_execution_owner. Regressions are being prepared before production changes;
this finding is not yet closed by verification. Normal Drop's finite same-owner close attempt does
not replace the nonblocking unwind retention policy.

# Citations

- crates/nara_app/src/runtime.rs: RuntimeInstance::start_inner
- crates/nara_app/src/runtime.rs: RuntimeCandidate::admit_reserved
- crates/nara_app/src/lib.rs: App::seal_internal
- crates/nara_app/src/plugin/resolve.rs: PluginPlan::instantiate_runtime_candidate
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5
