# Development Agent Handoff

Date: 2026-10-30. Purpose: consume this session's research and adjust development direction.
This is a reading/decision handoff, not an Accepted ADR, a source-code patch or release evidence.

## Start Here

1. Read [the roadmap](development-roadmap.md) for product priorities and design readiness.
2. Read [the synthesis](README.md) for source-confirmed constraints, recommendations and counterevidence.
3. Read [the supplemental review](supplemental-review.md) for missing cross-boundary acceptance cases.
4. Select the relevant implementation lane: [content](content-and-persistence.md),
   [runtime](runtime-and-execution.md), [render/UI](rendering-and-custom-ui.md), or
   [Editor/extensions](editor-and-extensibility.md). Use the
   [industry study](industry-directions.md) and [feedback/platform study](feedback-and-platform.md)
   as supporting context.
5. Reconcile the selected work with current [Strategy](../../../STRATEGY.md),
   [architecture navigation](../../architecture/README.md), Accepted ADRs, implementation ledger,
   actual source and the relevant current plan in the receiving checkout.

The reviewed Nara baseline is `75e1284ed28bc5a96686609f7dae8805d1924788` on `main`.
Local references were Bevy `ae6152152d6fb8308ad6038314b48ef295e80fbc`, Godot
`7be59b463a4798834d72d6d0a2942ddd01d920ee`, and Fyrox
`5495d012b32145674501af2e67d2fd06a06f04dd`. Do not silently treat a newer checkout or upstream
release as the same evidence. A source path/line identifies the reviewed snapshot, not a permanent
symbol locator; use `rg` to find its current implementation.

## Task Intent And Constraints

The user values avoiding future engine-wide replacements and permits pre-1.0 breaking changes.
They have selected Nara's own custom-drawn Editor UI. Use complete owning-contract replacements
when evidence supports them, while keeping future algorithms/support breadth conditional on real
consumers. Research and small experiments are valid before a finished reference game exists.

The repository's current instructions and the receiving user's task define execution scope.
This package does not grant credentials, approve deployment or authorize discarding concurrent
work. Preserve unrelated changes and reference checkouts. Follow the existing Cargo serialization,
formatting, verification and commit-approval instructions. Communicate in Chinese; development
documents and code remain English.

Do not read these reports as a demand to implement every observation immediately. The primary
studies established architectural constraints and risks, not a newly reproduced failure of a
currently supported runtime contract. An intentional limit can be an appropriate prototype and
still need replacement before its public model becomes an ecosystem dependency.

## Reconcile Before Selecting Work

For each selected finding, record a short disposition against current HEAD:

| Disposition | Required explanation |
| --- | --- |
| Already implemented | Link the current owner and actual qualification; do not redo it. |
| Still present, already planned | Identify the owning plan unit and refine its consumer/decision. |
| Still present, new contract refinement | Identify the affected Accepted/Proposed decisions and the concrete missing behavior. |
| Rejected or superseded | Give source counterevidence or the changed product requirement. |
| Deferred | State the trigger and which current contract remains deliberately bounded. |

At the reviewed baseline, source-independent processed startup/travel, compound Product identity,
canonical source locations, finite cyclic/diamond Scene closures, additive Schema adoption,
source Input definitions, task activity scopes and owner-local close custody already exist.
Earlier reports describing their absence are historical. Processed Input and complete target/package
delivery retain separate open work. Check the latest
[asset plan](../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md) and
[Input plan](../../plans/2026-10-30-input-definition-and-source-admission-plan.md).

## Candidate Implementation Units

These are complete consumer boundaries to refine, not a mandatory sequential backlog or a new
authority hierarchy. Reuse the existing
[foundation rebaseline](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md).

| Candidate | Owners and existing work | First useful acceptance |
| --- | --- | --- |
| Validated lazy Scene destinations | Schema/reference semantics, `nara_scene`, `runtime_content`; R6/R7 | Three Scenes and a shared atlas; strict Check/export covers destinations, current preparation excludes unrequested heavy destinations; cancellation remains owned. |
| Authored evolution and source extraction | Schema lineage, Prefab claims/addresses, document history/publication; R5 | One component upgrade and one nested Prefab extraction with explicit dependent rewrites, unknown-record preservation and failed-publication recovery. |
| Typed render composition and UI completion | `nara_render`, WGPU Host, material/image/UI owners; R8/R9/R11, Proposed 0121 | External texture/buffer Feature, alternate policy, different producer rates, one interactive placement/passive mirror, window-free boot and producer-local retirement. |
| Editable custom Editor slice | Editor application/document owners, separate UI interaction/text/platform owners; R1/R2/R11 | Multilingual Inspector and live image with exact results, Undo/save/reopen, filtering, focus, keyboard/IME/accessibility and a closed-panel semantic command. |
| Joint motion execution | App/schedules, Transform/Hierarchy, concrete solver/animation owner; R4c/R10 | Intent/solve/pose/publication protocol under zero/multiple ticks, pause/step, teleport and held-worker close; deliberate executor configuration. |
| Rust rebuild/resume and tool coexistence | Supervised Play, game-owned checkpoint, installed contributions | Body and structural changes restore a useful scene; two tools remain available across one tool's successful replacement/disable. |

## Important Interpretations

- Preserve the selected **finite dependency graph** direction. Deterministic serial topological
  execution is a valid initial implementation; declaration-order-only or fixed Sprite/UI families
  do not establish the selected extension contract.
- Exact output leases, a destination's display attempt and one-shot Runtime UI completion differ.
  Join Proposed 0121 to the relevant ADR 0108/0112 interaction/completion rules.
- Lazy backing and lazy reference demand solve different problems. Neither automatically changes
  the other, and today's `Soft` edge is not a drop-in validated-lazy reference.
- Keep immutable spatial read cuts while evaluating motion writes and private locality. Fixed ticks
  alone do not prove deterministic replay or whole-tick transactional success.
- Default executor behavior can change when the same Bevy dependency's features are unified on
  supported targets. This is a conditional source fact, not an observed current concurrency failure.
- A successful finite-tool activation replaces the active predecessor by current deliberate policy;
  a failed replacement retains it. A multi-installation graph is useful even with one active route.
- Ordinary Editor shell command contribution is the gap. Public query/prepare/commit APIs already
  let a custom Host implement the operation.
- Source-built Rust APIs are not a native binary ABI. Disabling a tool does not prove old code can
  unload while views, callbacks, history or destructors still reference it.
- Preserve the option to remove obsolete pre-release formats and APIs. Meaningful authored-content
  migration is different from carrying every abandoned prototype reader indefinitely.

## Expected Next Output

Return a short current-HEAD disposition, selected first unit, affected owners/contracts, rejected
alternatives and observable acceptance. Prefer adjusting the owning plan over creating another
omnibus architecture plan. Where the receiving user requests implementation, carry the selected unit
through consumer migration, obsolete-code removal and relevant checks, recording actual results.
Resolve substantive product choices left open by that task with the user; do not use stale research
registration or a historical plan ID as a new approval requirement.

Use current measurements to select numeric budgets. Match checks to changed behavior, reuse focused
nextest/native procedures, and keep a fast owner-test signal. Avoid compiler-like verification scripts
and repeated full-product qualification for documentation closure alone.

## Archive Contents And Limits

The ZIP includes all Markdown documents in this session's research directory, repository workflow
and strategy context, selected current architecture/plan/verification entries, and the research
documents' direct local Markdown references. Paths are preserved relative to the repository so
research-layer document links remain usable after extraction.

The package is documentation only. It does not include Rust source, reference checkouts, raw chat,
build artifacts, every historical document or the unrelated pre-existing untracked registration.
Source links and deeper references from supporting documents require the corresponding repository;
external URLs require their original sites. It does not claim a recursive offline mirror.

The archive root contains `START_HERE.md` and `MANIFEST.json`. The manifest records the actual
packaging time, Nara/reference revisions, inclusion categories, file sizes and SHA-256 digests.
Its purpose is checking this archive's exact bytes, not proving source qualification. No Cargo,
native run or fresh benchmark was performed for this document handoff.
