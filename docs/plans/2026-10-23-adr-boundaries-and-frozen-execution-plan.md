---
title: "refactor: Clarify ADR boundaries and frozen component execution"
date: 2026-10-23
type: refactor
execution: code
---

# ADR Boundaries And Frozen Component Execution

## Outcome

Implement the four changes accepted in the architecture review after `12768dc`.
Accepted decisions distinguish durable ownership and observable behavior from unvalidated future
representations and version-specific implementation details. Ordinary persistent operations use
an immutable, frozen execution authority with explicit failures.

The previous low-level refactor remains complete. This successor preserves its typed candidates,
target-World admission, asset baselines, spatial snapshots, task custody and exact Gameplay success.
No new wire format, 3D implementation, asset acquisition framework, scheduler or compatibility
layer is introduced.

## Work Units

1. Revise ADR 0097 to retain spatial domain and derived-affine invariants while moving future
   residual/3D representation and KeepWorld normalization into a non-authoritative candidate
   design. Reconcile affected current references. Revise ADR 0037 to separate identity, residency
   ownership and policy; remove the mixed cache-mode reservation.
2. Refine ADR 0081 before changing its public execution boundary. Reuse the existing frozen
   snapshot rather than introducing a second registry. Move executable codec operations to that
   immutable authority; distinguish unknown schema, unavailable native binding, codec failure and
   actual component absence. Migrate Scene, Export, Editor, tests and independent consumers.
3. Refine ADRs 0002/0044 and the ECS facade. Maintain an explicit ordinary authoring surface,
   expose the version-bound complete Bevy substrate through an explicit upstream path, and place
   internal transaction support with its internal consumers. Preserve facade-aware derives,
   renamed-dependency support and original Bevy runtime types.
4. Integrate focused tests, public consumers, relevant native Editor acceptance, documentation,
   implementation-ledger updates and independent review. Delete obsolete operations and imports.
5. Apply the user's root-document cleanup: retain README, AGENTS and Strategy; remove the
   redundant CONTEXT redirect and small GLOSSARY after preserving useful terminology at its owning
   guide/ADR and repairing active links. Immutable historical memory keeps its original references.

## Coordination

One canonical checkout on `test/compiled-asset-domain-export`; main thread owns integration,
Cargo, the shared ledger and architecture catalogue, final review and precise local commits.
Preserve the preexisting untracked `vendor/` and treat `repo-ref/` as read-only.

| Lane | Owner | Writable scope | Output |
| --- | --- | --- | --- |
| Frozen execution and integration | Main | Reflect, all migrated consumers, ADR 0081, guides, shared documents and this plan | Frozen execution API, complete migration and verification |
| Spatial and asset decisions | Worker A | ADRs 0037/0097 and directly affected spatial ADR clauses; one candidate design | Narrow accepted decisions with preserved candidate reasoning |
| ECS public boundary | Worker B | `nara_ecs`, `nara_ecs_derive`, ADRs 0002/0044 and ECS-specific documentation | Explicit authoring/upstream/internal paths and migration inventory |
| Independent challenge | Reviewers | Read-only | Concrete contract, correctness and ergonomics findings |

Workers do not run Cargo, format the workspace, commit, edit shared integration documents, or
change another lane's files. Main migrates imports outside the facade crates after the boundary
settles. Coordinated worker edits are expected; unexpected external changes require inspection.

## Verification

- Serial Cargo only, existing `target`, `CARGO_BUILD_JOBS=1`; prefer focused nextest cohorts.
- Reflect tests cover unknown schema versus missing native binding versus codec failure versus
  real absence, frozen-authority admission, typed candidates, contextual codecs and retained
  snapshot identity. Existing transactional/lifecycle/asset tests retain their guarantees.
- Compile-fail documentation proves execution cannot be called on the building registry.
- Scene, tooling and root tests cover persistent spawn/export, component editing and runtime
  authority. Native Editor qualification covers affected document-edit/save/reopen behavior.
- Independent module and renamed root/ECS fixtures compile and execute their supported derives
  and manual codecs; public games retain a working headless path.
- Run `cargo fmt --all`, affected correctness/suspicious Clippy checks, documentation/reference
  checks and final diff review. Record actual commands, outcomes and platform limits in engineering
  memory rather than treating prior test runs as fresh evidence.

## Completion

Every work unit has current source/document evidence, affected verification passes, independent
review has no unresolved blocking finding, and precise Conventional Commits contain only intended
files. Execution progress belongs in immutable engineering-memory records linked to this plan.
