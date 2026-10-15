# ADR 0117: 2D Foundation Ownership and Authoring Contracts

**Status**: Accepted
**Date**: 2026-10-15
**Owner**: game composition, managed execution, content, domain plugins, rendering, and tooling
**Implementation Plan**: [2D foundation contracts](../../plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md)

## Context

The source studies and correctness audit show author-facing execution choreography, duplicate
schema registration, load-attempt state hiding published content, primitive-kind depth ordering,
and persistence restrictions that cannot accommodate arbitrary native side effects honestly.
The user authorized a pre-1.0 refactor before adding more engine domains. The aim is a coherent
general-purpose 2D authoring path, not implementing every Proposed ADR or promising no future refactor.

## Decision

This ADR refines only the clauses listed below. Unrelated guarantees in the referenced ADRs remain
binding. Acceptance authorizes the changed contracts; it is not implementation or verification evidence.

### One game definition and one execution owner

App configures systems, resources, plugin contributions, and persistent schema. The resulting
definition is frozen once per executable composition and consumed by managed execution. Normal
headless, desktop, and embedded use share startup, fault, stop, and physical-close ownership.
Candidate staging and publication remain private; raw App execution is removed as a competing
production lifecycle after consumers migrate. This refines ADRs 0003/0084 without removing retryable
task retirement, sticky faults, or generation isolation.

One typed plugin declaration contributes systems, durable component IDs, migrations, and cleanup
responsibility. Provider/binding receipts remain internal composition facts. Durable owner/type/field
IDs, owner omission semantics, tombstones, and migration requirements remain unchanged (0046/0081/0098).

Strict Check/Export uses an explicit mode on the compiled project's existing headless executable,
before gameplay starts. The generic CLI can discover/build that executable before schema-dependent
strict admission. The project process checks using its own frozen definition and owns the admitted
directory capabilities, closure, build agreement, preparation, and publication. Cross-process summaries
do not transfer authority. Generic Editor recovery stays data-only and does not load project Rust.
This refines the generic CLI flow under ADRs 0110/0114, retaining source-race and product-agreement checks.

### Persistent data and native effects have different transaction semantics

Persistent document validation, migration, reference resolution, and revision publication are atomic.
Domain systems reconcile accepted configuration into transient native state and own its update and
cleanup. A plain persistent component may also be runtime data; separate representations are needed
only where native ownership/effects demand them.

A rejected document keeps its prior revision. Failed runtime reconciliation reports the accepted
revision as degraded/unavailable and supports explicit retry or reconstruction. It does not promise
rollback of arbitrary native effects. Startup candidates remain unpublished on failed startup.
Strict old-or-new native activation requires a concrete domain's prepare/activate protocol.

This refines the universal reading of persistent-binding/retirement guarantees in ADRs
0006/0027/0081/0100/0101. Existing narrow lifecycle-free transactions retain their stronger guarantees;
their guards remain until affected consumers move to the new owning seam. RequiredComponents and
hooks are not simply enabled inside an existing rollback-protected mutation. The first proof uses
the real Image pipeline and a hook-owning transient fixture, not a universal hydrator or physics SDK.

### Identity and resource state remain independently meaningful

Logical asset UUID, content locator, derivation key, and process-local handle identify different facts.
The initial supported profile has one principal durable product and one entity-bearing document per
source; importer-private auxiliaries do not imply a public product graph. Authored entity IDs remain
document-local and opaque. Prefab provenance is structured; live lookup requires instance selection.
Saved dynamic identity and an explicit authored-instance binding do not establish a universal save format.

Source rename preserves identity. Copy-as-new changes IDs and internal copied references; conflicting
mounted UUIDs fail instead of silently remapping immutable packs. Runtime export is a bake/convert
operation, not a lossless authoring round trip. This narrows the admitted work under
0037/0038/0058/0101/0103 without accepting Proposed 0083/0087/0088 wholesale.

Published usable value/revision is independent from the latest load attempt and diagnostic.
Preparation consumes the published revision even if a later attempt failed. Removal, identity
invalidation, or successful replacement invalidates the corresponding derivation. Async completions
still validate their owner/revision at publication. This clarifies ADRs 0033/0037/0115.

### Structural mutation uses one baseline

Admitted deferred work is materialized before resolving structural topology. Preparation and commit
share a semantic baseline with no intervening external flush. Every admitted hierarchy participant,
including a childless root, participates in lifetime invalidation. Raw ECS escapes invalidate
completion rather than act as an equivalent second writer. Ordered structural transactions,
hierarchy/lifetime separation, and completed transform projection in ADRs 0097/0100/0101 remain binding.

### 2D order, views, and presentation

Sprites and tilemaps share per-view layer/sort-group order, optional group-scoped Y sorting, and a
stable tie order. Primitive kind does not dominate user depth. Only adjacent compatible items batch.
UI has explicit overlay placement and internal painter order. This supersedes ADR 0105's fixed
Tilemap-before-Sprite ordering while preserving private execution and resource-complete transfer.

A view owns an invertible camera/projection mapping, viewport, visibility, order, and clear/load
policy. A target owns physical configuration; presentation maps it to a window/panel. A bounded
ordered view list executes serially, including shared targets and explicit offscreen sequencing.
This supersedes the single-view cardinality clauses in 0018/0094/0111/0112, retaining exclusive target
ownership. Optional pixel-canvas policies do not quantize simulation. No public render graph or RHI is added.

Material contributions are trusted Rust startup declarations with parameters, textures, alpha semantics,
and supported renderer capabilities, proven by one palette example. Shader code changes require restart
in this scope. Scripts/data select admitted definitions and values without Device/Queue authority.
Unused duplicate material descriptors are removed; public intermediate renderer orchestration is narrowed.

### Document transactions and toolkit-neutral presentation

Non-geometric authoring edits use explicit base-revision transactions, one atomic commit and undo entry.
They are not limited by one published-frame batch. Another actor's stale transaction is rejected without
implicit rebase. Picking/dragging remains bound to the actual displayed geometry. Selection and diagnostic
acknowledgment do not consume a document mutation slot. This refines affected exact-frame action clauses
in 0078/0111/0112 without discarding displayed-frame authority.

Content, target, device, and presented-frame identities remain separate. One private presentation owner
joins them and returns an opaque token. Toolkit texture registration belongs in the native adapter,
refining 0040/0107/0111/0112. Existing egui remains an adapter; no replacement toolkit is selected.

### Validation and effect ownership

Retain bounded ingress checks, definition collision/dependency checks, revision checks at async commit,
save-race checks, and physical cleanup checks. Remove repeated same-owner proofs only after mutable
replacement escapes have been closed. The temporary reporter swap-and-restore counterexample remains
covered; equal declaration hashes never prove arbitrary factory behavior.

Strict publication preflights the actual capability instance before avoidable file effects. Failure
after creating temporary state retains verified cleanup ownership; unverified path deletion is forbidden.
SPL requests and observations belong to one private effect owner, publish only changed observations,
allow reachable close retry, and move slow publication to cancellable owned work. ADR 0116's exact
generation/handle/digest/nonce/first-present guarantees remain unchanged. Cargo/native code is trusted,
not sandboxed. This refines interface ownership, not platform guarantees.

## Consequences and Limits

Migrate starter, examples, Reference Game, and independent consumers through each corrected seam, then
remove obsolete paths. Preserve explicit durable fixture conversion and original input on failure.
Existing SPL-U0–U2 evidence is retained; unfinished SPL work is carried into foundation U12.

Use behavioral regressions and real compile/privacy consumers. Do not extend source-parsing tests into
a Rust compiler approximation or reintroduce ADR 0099's rejected proof/approval framework. No universal
package kernel, save system, script VM, native plugin hot-unload, physics/audio backend, or second UI
is required for foundation completion. Windows/Linux/macOS share the design; native release claims
require evidence for that platform. Web remains deferred.

## Evidence

- [Correctness audit](../../verification/2026-10-15-foundation-correctness-audit.md)
- [Runtime study](../../research/2026-10-15-foundation-design/runtime-concepts.md)
- [Content study](../../research/2026-10-15-foundation-design/content-concepts.md)
- [Rendering study](../../research/2026-10-15-foundation-design/render-concepts.md)
- [Resolved plan review](../../research/2026-10-15-foundation-design/plan-review.md)
