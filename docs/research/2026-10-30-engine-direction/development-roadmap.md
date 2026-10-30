# Development Direction And Design Readiness

Date: 2026-10-30. Status: recommended direction, not a newly Accepted ADR or an implemented roadmap.
Baseline: `75e1284ed28bc5a96686609f7dae8805d1924788`.

This document captures the follow-up route advice from the research conversation. Read it with
the [assessment](README.md), [implementation studies](README.md#research-lanes), and
[supplemental review](supplemental-review.md). The receiving development agent should use
[the handoff](development-agent-handoff.md) to reconcile this snapshot with its current checkout.

## Product Direction

Develop Nara as a **Rust-first integrated production tool with a Nara-drawn Editor and a composable
runtime**. Maintain one coherent, qualified default product combination. Module boundaries should
support real independent use and extension without forcing every game author to assemble the engine.
This is consistent with [Strategy](../../../STRATEGY.md), not a replacement product mandate.

Keep two concrete consumers active:

1. A small PC 2D game that is continuously authored, played, debugged, saved and exported through
   ordinary public paths. Add basic physics, animation and audio as actual gameplay needs arise.
2. A useful Nara-drawn Editor slice, beginning with editable properties and a live Scene view.
   It should exercise real interaction and document operations early, before a full widget library.

These consumers supply complementary pressure. Neither must wait for the other to become a finished
product. The Editor may share rendering and text/layout algorithms with the game while owning its
own interaction state, scheduling and platform integration.

The aim is to contain future changes inside their owning modules. Decide stable identity, data
meaning, write responsibility and lifetime before broad adoption. Evolve private algorithms,
caches and backend optimizations against measurements. No architecture can remove all future
refactoring, and speculative universality can increase its cost.

## Milestones With Observable Exit Conditions

These are dependency-oriented milestones, not a calendar or a new parallel plan hierarchy. Existing
R4c/R5/R6/R7/R8/R9/R10/R11 units remain the implementation organization.

| Milestone | Work | Exit evidence |
| --- | --- | --- |
| A. Resolve expensive semantic boundaries | Content reference/demand, Schema/Prefab evolution, render contributions and joint motion phases | Each boundary has a complete consumer, explicit refusal/recovery behavior, selected ownership and the affected contract reconciled. |
| B. Make ordinary production usable | Continue the small game while delivering self-drawn Inspector, hierarchy and viewport slices | Import/edit/play/debug/save/export works through public paths; text input, selection and Undo survive real editing; corresponding Dear workflows can be retired after parity. |
| C. Improve Rust production feedback | Rebuild/resume, data/shader reload, profiling, errors and automated delivery | Common changes have measured edit-to-correct-result endpoints; failure can be repaired; exports run outside the source checkout. |
| D. Qualify independent consumers | An independent game/tool author, two installed contributions, another platform and a bounded 3D experiment | External authors complete the task with documented APIs; integration friction is repaired by the owning module; only demonstrated support is advertised. |

Start the first editable UI and motion experiments during milestone A. Do not postpone IME,
interaction completion or solver writeback until after their surrounding interfaces are frozen.
Milestone D does not require immediate high-end 3D, networking, mobile or console support.

## Seven Decisions To Design Before They Become Expensive

### 1. Content Identity, Evolution And Source Refactoring

Separate source identity, imported Product identity, Prefab occurrence identity, runtime instance
identity and transient ECS indices. They answer different questions and have different continuity.

The first upgrade consumer should change one component's representation while retaining some stable
fields, update nested Prefab claims and dependent dirty documents, and encounter an unavailable
owner. A source extraction should explicitly map old occurrences to new ones. Decide whether an
upgrade establishes a new history checkpoint or proves an old history basis; neither follows from
component-value migration alone. See [content CP1/CP2](content-and-persistence.md).

This does not require preserving every unreleased prototype format. The existing
[rebaseline rules](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#decisions-made-in-this-round)
allow obsolete internal formats/readers to be removed. Test meaningful authored evolution without
creating permanent compatibility scaffolding for abandoned prototypes.

### 2. Reference Validity, Delivery, Preparation And Residency

Distinguish a valid destination from a resource necessary to materialize the current Scene. A build
closure can include all valid travel destinations without preparing every destination's heavy data
for the current level. Independently separate immutable catalog/code-plan authority from encoded
backing, decoded values and native residency. See [content CP3/CP4](content-and-persistence.md).

Use three linked Scenes, one shared atlas and heavy later-level resources. Validate the destination
set, selected acquisition, cancellation, overlapping generations and an escaped Image. Preserve
exact original backing authority; never use a lazy implementation that silently reads a new mutable
base. The existing eager backend remains valid for its bounded captured-data use case.

### 3. Authoring-To-Runtime Projection

An authored object may generate several runtime products: a TileMap can produce render chunks,
colliders and navigation data; a model can produce mesh, material, skeleton and animation products.
The desired authoring and runtime layouts need not match one-to-one.

Define source-to-output provenance, stable generated identity, input dependencies, invalidation and
publication for the first real consumer. OQ-008 in
[open questions](../../architecture/open-questions.md#oq-008-authoring-to-runtime-projection-and-baking)
already owns this decision. A universal Baker interface is not a prerequisite. Do not freeze public
persistence or extension contracts around a one-record/one-runtime-component assumption.

### 4. Time, Pose Writes And Execution Completion

Exercise animation intent, physics advancement, game correction, pose commit, spatial publication
and interpolation together. Define pause, exact-step, zero/multiple fixed ticks, teleport and parent
frames. Make the supported ECS execution policy explicit rather than relying on dependency feature
unification. Keep borrowed same-tick jobs distinct from owned cross-frame tasks.

Compare existing public sets, a finite sealed phase plan and the Bevy App control using the same
consumer. Preserve Nara's lifecycle guarantees whichever composition mechanism wins. Future replay
or networking can remain deferred, but input, RNG and external results need clear simulation entry
points before promising reproducibility. Fixed ticks alone do not prove deterministic replay. See
[runtime RE1-RE5](runtime-and-execution.md).

### 5. Render Resources And Independently Updated Views

Preserve the selected finite resource-dependency graph. Qualify external Features, an alternate
renderer policy, texture/buffer access and durable material evolution under one Host. Distinguish
an exact output's resource lifetime from display attempts and input authority.

The first shared-view consumer should include different producer/redraw rates, a passive mirror,
a failed or hidden destination, and retirement of one producer while another continues. A retained
texture view alone does not preserve an old pixel version or settle one-shot UI completion. See
[render F1-F3](rendering-and-custom-ui.md).

### 6. Custom UI Interaction And Editable Text

Design model/property identity, live widget incarnation and saved panel identity separately. Retain
edit drafts, caret/selection, focus, capture, composition and semantic accessibility state through
filtering, virtualization and appropriate window transitions. Separate layout, paint and semantic
invalidation so unrelated viewport redraws do not require rebuilding every control.

Use actual multilingual editing, keyboard navigation, numeric editing, Undo, Save/Reopen and a Scene
image beneath clipped UI/popups. Native detach additionally needs DPI, focus, IME and retirement
evidence. Reuse Parley, Swash, Taffy and platform bridges where suitable; Nara still owns the toolkit
and its scheduling. See [render F4/F5](rendering-and-custom-ui.md) and
[Editor E1](editor-and-extensibility.md).

### 7. Contribution Placement And Executable Lifetime

Treat source-built Rust plugins, finite child tools, resident language adapters and native binary
extensions as distinct execution products. Activation, disable, replacement and physical unload
have different completion conditions.

First qualify a command available with its panel closed, two independent installed tools, and
game-owned capture/rebuild/restore. Retained views, history, callbacks and destructors must be
considered before advertising native code unload. A universal ABI, resolver, scripting runtime or
extension kernel is not required to deliver these workflows. See
[Editor E2-E5](editor-and-extensibility.md).

## Industry Directions And Their Weight For Nara

The facts below were checked against primary sources on 2026-10-30. A dated strategy is evidence of
investment direction, not a declaration that every planned feature has shipped. These references
extend the [industry study](industry-directions.md); they do not prove Nara defects.

| Observed direction | Primary evidence and status | Recommended response |
| --- | --- | --- |
| Product reliability and fast iteration | [Godot's vision, revised 2026-06-23](https://godot.foundation/policies-and-procedures/project-vision-statement) prioritizes reliable complete features, iteration and usability across a game team. | Measure complete author workflows and recovery, not only runtime capability counts. |
| Less renderer fragmentation | [Unity's February 2026 strategy](https://discussions.unity.com/t/render-pipelines-strategy-for-2026/1710004) concentrates development on URP and describes a staged Built-in deprecation/HDRP maintenance direction. | Maintain a clear default product and shared infrastructure while allowing proven extension seams. Do not reproduce multiple largely duplicated renderer families. |
| Processed assets and tool-driven composition | [Bevy 0.20, released 2026-10-08](https://bevy.org/news/bevy-0-20/) improves texture processing; its subsequent scene-format and remote-inspector items remain future work in that announcement. | Strengthen build products, dependency invalidation and semantic authoring APIs. Choose Nara's own representation based on its consumers. |
| Procedural, programmable content production | [Unreal 5.7, released 2025-11-12](https://www.unrealengine.com/news/unreal-engine-5-7-is-now-available) makes PCG production-ready and describes parameterized, independently executable content tools. It is an illustrative released baseline, not a claim that 5.7 is the newest release. | Let tools generate editable, attributable content through normal transactions and import publication. Start with one useful operation rather than a general graph language. |
| Higher-density GPU workloads and dynamic rendering | The same Unreal release describes advanced materials, lighting and geometry, with individual Experimental/Beta labels. [Unity's strategy](https://discussions.unity.com/t/render-pipelines-strategy-for-2026/1710004) also emphasizes scalable dynamic scenes. | Preserve resource access, batching, target capability and fallback boundaries. High-end algorithms follow a real target workload. |
| Ordinary hardware and accessible tools | [Godot's vision](https://godot.foundation/policies-and-procedures/project-vision-statement) includes common hardware, low setup burden, languages and assistive technologies. | Include idle work, keyboard/IME/accessibility and platform behavior in the custom Editor's design from the first real slice. |

AI-assisted authoring is an additional optional client of these foundations. It can inspect project
state, propose a change, preview a diff, submit, undo and save through the same operations as the UI
and ordinary scripts. Keep model/provider policy outside ECS and rendering. Runtime inference for a
game feature remains a separate consumer. The dated beta evidence and limits are in
[industry topic 8](industry-directions.md#8-ai-assisted-creation-belongs-above-semantic-tools-and-asset-publication).

## Measurement And Stop Rules

Record current source, toolchain, hardware, workload and endpoint before choosing numeric budgets.
Track edit-to-correct-result, restoration to a useful gameplay situation, owner-thread occupancy,
input-to-present, frame tails, encoded/decoded/native retention, activation latency and clean-machine
export. Keep CPU, GPU and monitor presentation facts separate. Historical five-sample measurements
are not current P95 guarantees.

A foundation unit should either improve a complete workflow or eliminate a named structural risk.
Its completion is a working consumer, migrated call sites, removed obsolete mechanism and scoped
verification. Additional cases should address new uncertainty rather than expand the test/script
surface indefinitely. Use [feedback/platform findings](feedback-and-platform.md) for the existing
measurement and qualification limits.

Defer a replacement ECS, universal task kernel, universal language runtime/ABI, package marketplace,
distributed build service, high-end 3D suite and general world partitioner. Keep their relevant
identity/dependency/ownership seams open to demonstrated requirements. A bounded 3D import/spatial
experiment can challenge 2D assumptions before animation/property formats harden without promising
a complete 3D product.

## Recommended Immediate Selection

Begin with content reference/evolution semantics and the render/interaction boundary required by
an editable custom Inspector. Bring the joint physics/animation consumer into phase-plan selection
at the same time. Continue the ordinary game and reliable rebuild/resume path as product evidence.
The [handoff](development-agent-handoff.md) specifies how to turn these recommendations into a
bounded change proposal against the receiving agent's actual HEAD.
