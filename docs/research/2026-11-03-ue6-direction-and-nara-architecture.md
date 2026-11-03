# UE6 Direction And Nara Architecture

Research and design review: 2026-11-03. This reconciles the
[primary-source investigation](2026-11-03-ue6-primary-source-findings.md) with the current
[foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md).
It is not an implementation or performance qualification.

## Evidence And Product Boundary

The supplied official article and the newer official-account clip concern gameplay-framework
replacement. The investigation distinguishes their chronology and unresolved transition scope;
neither establishes removal of the native C++ engine. No complete Shanghai transcript or
converter execution was obtained. See the source investigation's claim table and access limits
before repeating a release or compatibility claim.

For Nara, the useful comparison is the separation of authored meaning, runtime state, execution
rights and external effects. Epic's large persistent-world goals do not by themselves create
the same product requirements here. [Strategy](../../STRATEGY.md) continues to require a complete
Rust production path with optional language adapters and an integrated Nara-drawn Editor.

The concrete native reference is more useful than a language slogan: Epic's historical AutoRTFM
implementation instruments native execution and explicitly handles operations outside its
transaction model. This is evidence that reversible execution needs an admitted implementation
boundary, not a property conferred by a scene or component API name.
[AutoRTFM technical article](https://www.unrealengine.com/tech-blog/bringing-verse-transactional-memory-semantics-to-c?lang=en).

## Existing Decisions Reinforced

| Nara boundary | Current authority and scope | Disposition |
| --- | --- | --- |
| Semantic definitions versus native Rust representation | Accepted [ADR 0119](../architecture/adr/0119-schema-first-semantic-metadata-authority.md) and [ADR 0081](../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md). Proposed [ADR 0120](../architecture/adr/0120-schema-publications-and-execution-representations.md) remains broader than its adopted data-family subset. | Preserve Schema-owned meaning and exact native binding. A native type, process handle or editor control is not persistent semantic identity. Complete executable replacement still needs its own consumer. |
| Authored composition versus execution data | Accepted [ADR 0006](../architecture/adr/0006-scene-and-prefab-data-model.md) and [ADR 0101](../architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md); current [CP2 extraction](../plans/2026-11-02-prefab-extraction-plan.md) is unfinished. | Keep source-owned composition and independent flat compiled output. Do not introduce a parallel runtime hierarchy simply to copy the name Scene Graph. |
| Editor frontend versus semantic operation | Accepted [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md); R1/R2 have bounded operation/consumer evidence. | A visual frontend or optional automated adapter must use the same query, preparation, exact outcome and persistence authority. The self-drawn frontend remains the target. |
| Runtime, activity and native lifetime | Accepted [ADR 0084](../architecture/adr/0084-executable-runtime-ownership-and-isolation.md), existing Tasks custody and the bounded R4 activity work. | Keep physical return and resource retirement distinct from logical cancellation. A new component callback or language adapter cannot silently weaken this. |

Epic's entity API independently recommends component-owned logic so prefab structure can vary.
Its component API separates scene membership, simulation and shutdown. These are useful concrete
comparison points; neither API proves a particular ECS memory layout or a cancellation guarantee
for Nara's arbitrary Rust/native jobs.
[Entity API](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/entity),
[component API](https://dev.epicgames.com/documentation/en-us/fortnite/verse-api/versedotorg/scenegraph/component).

## Concrete Gap: Speculative Execution And Effects

An independent read-only review found a bounded omission in the future R4c/R10 design. The plan
already requires same-tick solver joins, explicit executor selection, closed inputs, RNG/outcome
policy and a prediction/effect-suppression tracer. It does not yet fully specify the state admitted
to a speculative attempt or distinguish an attempt's completion from authoritative confirmation.
This is an unfinished design boundary, not evidence that ordinary runtime behavior violates its
current contract.

The code supports that distinction:

- [`App::add_systems`](../../crates/nara_app/src/lib.rs) accepts ordinary Bevy configurations.
  Registration and ECS access information do not establish absence of external effects.
- [`nara_gameplay::queue`](../../crates/nara_gameplay/src/queue.rs) validates command consumption
  against runtime generation and tick. This supports its current authoritative execution model;
  it is not an identity for repeated speculative attempts of the same tick.
- [`nara_gameplay` fault regressions](../../crates/nara_gameplay/src/lib.rs) retain effects from
  later consumers/deferred work after a consumption failure. Accepted ADR 0084 explicitly allows
  partial mutation and requires close/reconstruction. Clearing the fault and rerunning would
  contradict that contract.
- [`TaskScope`](../../crates/nara_tasks/src/runtime/scope.rs) and the
  [Tasks runtime](../../crates/nara_tasks/src/runtime.rs) retain admission/disposal custody.
  Rejecting a stale result cannot retract I/O already executed by its job.

The review read these sources; it did not execute motion, rollback or native solver tests.

## Selected Plan Adjustment

Refine Accepted [ADR 0024](../architecture/adr/0024-determinism-fixed-update-and-replay-policy.md#future-prediction-qualification)
with the qualification boundary, and extend the existing R4c/R10 consumer rather than creating a
separate execution platform:

1. **Define the bounded restorable state.** Include participating components/resources, entity
   lifecycle, RNG, system-local state, pending commands, admitted asynchronous outcomes and solver
   state that affects the selected result. Restoration may use tested reconstruction. Uncovered
   state/effects refuse admission to that prediction scope. Ordinary systems need no speculative
   registration or new purity annotation.
2. **Separate three observations.** Command consumption, successful simulation and authoritative
   confirmation are independent. Select the ordinary Ack boundary through the already planned
   R4c comparison; a speculative success must not implicitly publish irreversible effects.
3. **Separate attempt and event identity.** An attempt identity rejects old work from a rerun of
   the same tick. A semantic effect identity supports the concrete effect owner's duplicate
   suppression across reruns. Keep both inside the first concrete consumer until reuse is proven.
4. **Prove effects through the already planned motion/audio tracer.** Correct and rerun a tick, compare
   complete authoritative state, delay an old-attempt result, suppress discarded audio, and
   refuse an unmodeled native operation. Preserve the ordinary sticky-fault/partial-effect test.
   Select delayed publication or a tested recovery policy for each participating external effect.

R4c must settle these semantics before freezing the affected phase Interface. Complete rollback
implementation remains R10 work, not a prerequisite for every scheduler comparison. Start with
the serial profile and then each actually admitted parallel profile and real solver join.
This selects no global system taxonomy, static purity analyzer, universal event bus, STM compiler,
Wasm boundary or exactly-once external delivery protocol.

## Follow-ups Within Existing Units

- **R3 executable behavior:** continue comparing one complete Rust/optional-language workflow,
  including author iteration, immutable program/data publication, old-instance retirement and
  fresh Runtime recovery. Do not mistake an extensible representation for a complete language
  product or make another language mandatory.
- **R1/R2 and R11 tools:** retain frontend-independent semantic operations for the Nara-drawn
  Editor. A future model-assisted adapter proposes an ordinary bounded source edit with exact
  review/outcome/history/persistence; it receives no new raw-World or file-write authority.
  This study does not select an MCP server implementation.
- **CP1/CP2 and conversion:** verify preserved meaning, references, defaults, history and actual
  disk results. Generated conversion is a proposal, not evidence of equivalence. Nara has no
  released compatibility burden requiring an Actor-style legacy subsystem or another migration
  guide. Current CP2 extraction continues on its existing complete workflow.
- **R8/R9 rendering:** preserve their selected resource composition/Host execution plan. The
  inspected UE6 gameplay announcement provides no measured render-graph comparison that would
  replace the pinned Bevy/Godot studies or reopen qualified render work.

The next evidence needed for a broader UE-specific conclusion is a complete versioned migration
contract or full Shanghai transcript. The next Nara implementation gate is the concrete
motion/executor/effect consumer, after the current extraction unit. Neither gap is solved by
renaming modules or making every engine operation transactional.
