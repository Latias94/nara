---
type: "Verification Evidence"
title: "Bounded direct-member discovery and non-active panel query workflow"
description: "Qualified one-level durable Record-member discovery, selection-independent Session/panel query forwarding, and an independently compiled non-active panel discover/read/prepare/retain/commit workflow."
timestamp: 2026-10-26T004155Z
record_id: "7003450c49204d8eba92ba90db765dc0"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "main"
git_commit: "90a92e5"
---

# Verification

This record qualifies the bounded direct-member and panel-query successor in the working tree
after `90a92e5`. It does not claim that the whole foundation plan, full R2, recursive or
collection-member discovery, Prefab overlay, installable tools, shared-owner preparation locality,
or native interactive preparation performance is complete.

# Result

- `nara_scene` exposes a borrowed direct-member view over the admitted Schema declaration and
  tombstone slices. It does not build or sort a complete member inventory for one page.
- `nara_tooling` exposes `EditorAuthoringProjection::Members` with a private owner-bound cursor.
  Active members without durable IDs consume scan work but do not fall back to wire names; direct
  tombstones produce retired rows; nested tombstones and recursive members do not leak into the
  one-level projection.
- Member pages enforce independent scan and row limits, report actual inspected candidates and
  charge the root ID again for every copied `root + Member(id)` property path in aggregate
  metadata. Continuations retain exact workspace, document kind, source incarnation, revision,
  Schema, entity, projection and active/tombstone positions.
- `EditorWorkspace::document_query_basis`, `EditorProjectSession` and
  `ImguiEditorPanelContext` now share selection-independent query forwarding. Panel query access
  preserves the `Closing` lifecycle rejection instead of collapsing it into workspace absence.
- An independently compiled registered Workspace-target panel discovers an explicitly named
  non-active Scene entity, component, root and direct member; performs a bounded read; prepares a
  source-owned property candidate; retains it for a later paint; and commits it through the
  existing ordered submission/history owner. The active document, selection-independent routing
  and commit result are asserted.
- Recursive fixture Record members now carry durable IDs, so the public panel workflow proves the
  durable selector path rather than relying on wire-name fallback.

# Review Corrections

The first independent review found two correctness gaps and several proof gaps. The implementation
was corrected before the final focused execution:

- member metadata now includes the copied root ID for every returned property path;
- member discovery no longer allocates/sorts a full vector before applying scan limits;
- successful member continuation, asymmetric scan/row limits, nested and wire-only members,
  exact row metadata, and non-active panel query forwarding are covered;
- the panel test asserts exact target/component/readiness and covers the full nested workflow;
- the public query rejection model retains the owning Editor's `Closing` state.

The final review was requested after these corrections; its result must be appended if it identifies
another concrete defect. Public enum additions are intentional pre-1.0 source breaks and are
documented in the owning ADR/design rather than hidden behind compatibility aliases.

# Serial Execution

All Cargo commands used the repository checkout and shared `target` with `CARGO_BUILD_JOBS=1`; no
Cargo commands were run concurrently.

| Scope | Run/result |
| --- | --- |
| Root/Scene/Tooling focused cohort | `18868517-7b1f-435a-8b4a-21dcdea5333c`: 420 passed, 4 skipped |
| New panel facade and non-active nested workflow | `13cdf150-805d-4250-9015-09655b0a5c4e`: 3 passed |
| Existing recursive authoring regression cohort | `cde7e693-f4a9-4d80-9fcf-286a845fe23a`: 4 passed |
| Initial direct-member focused regression after correction | `3c08624f-6233-4595-8521-1b65e65f479c`: 1 passed |
| Correctness Clippy for affected packages/all targets | Passed with existing non-correctness warnings; no correctness error |
| Formatting and whitespace | `cargo +1.95.0 fmt --all` and `git diff --check` passed |

A full `nara-editor` package nextest run remains non-green on seven existing application/lifecycle
contract tests (`0c00a96f-04b7-4e24-8a25-ed9d64bc453e`; the same seven reproduce in the focused
rerun `9d9e3597-f0d0-4a24-81e0-a267959caaf2`). The failures concern unrelated process-exit,
workspace-retention and recovery assertions; the new facade and nested workflow pass in isolation.
This unit does not attribute or repair those pre-existing lifecycle failures.

# Limits

- The member projection is deliberately one direct Record level. It does not paginate arbitrary
  nested paths, list/map/keyed-list items, or expanded Prefab projections.
- The panel proof uses a headless Dear ImGui test host. It is not native Windows/DX12, Linux/macOS,
  IME/accessibility, latency, installable-tool or release qualification.
- The query basis Start path retains the existing workspace owner model; only owner-created
  continuations carry the process-local workspace token.
