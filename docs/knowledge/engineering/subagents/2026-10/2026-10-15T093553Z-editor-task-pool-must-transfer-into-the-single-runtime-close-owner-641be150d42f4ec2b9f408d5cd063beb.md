---
type: "Subagent Finding"
title: "Editor task pool must transfer into the single runtime close owner"
description: "U5 must migrate the existing RuntimeOwner state and editor preinserted TaskPools, rather than wrap the executable App or retain a second task-close authority."
timestamp: 2026-10-15T09:35:53Z
record_id: "641be150d42f4ec2b9f408d5cd063beb"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
---

# Finding

U5 must remove the executable App nested inside RuntimeOwner and the Editor's independently closed
task pool. Merely changing the normal launch call would preserve two execution/retirement authorities.

# Evidence

- `nara_app/src/runtime.rs:2719-2731` already owns a private RuntimeOwner/RuntimeOwnerState in the
  low-level crate; its state currently nests RuntimeOwnedValue<App>.
- `nara_app/src/lib.rs:1219` still holds World, schedules, installed plugins, obligations, reporter,
  startup state, and raw execution methods. App Drop closes separately at line 1252.
- `src/project_host/authoring_runtime.rs:539-544` manually creates/inserts TaskPools.
  `nara_tasks/src/runtime.rs:3425-3427` skips close-participant registration for an already present
  pool. EditorAuthoringRuntime still calls shutdown_tasks and owns its own Drop.
- Editor preview drives App directly, while image operations also mutate its World. Migrating
  only preview run_once would omit these real consumers.

# Recommendation

Have configuration App own the existing private RuntimeOwner from construction. Move World,
schedules, installed runtime plugins, startup/fault execution, and cleanup directly into its state;
seal/candidate/instance transfer that same owner. Remove RuntimeOwnerState.app, App's separate Drop,
runner, and raw execute methods after migrating consumers. Do not add AppParts/another kernel or
make the low-level crate depend on the product root.

Keep the ordinary start/drive/close entry in nara_app for independent modules. Root DesktopRun adds
content/platform composition over it. Move root's admission orchestration inward before claiming
cross-crate machinery is private; a doc-hidden public bridge is still a public bridge.

Let TaskPlugin create/register the Editor task pool, or explicitly transfer that existing close
participant. Editor close observes/drives the same runtime close state. Preserve failed-start
retirement ownership and reporter/registry replacement checks during this migration.

# Disposition

Admitted as concrete U5 implementation guidance. No U5 code or tests have been changed yet. The
existing owner and task APIs are sufficient; no additional lifecycle framework is required.

# Citations

- `docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md` U5 / KTD1
- `docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md`
- Read-only source trace by Codex subagent `/root/render_world`, 2026-10-15
