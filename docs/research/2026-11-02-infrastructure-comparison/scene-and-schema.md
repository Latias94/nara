# Schema, Scene Intent, Prefab And Identity Evolution

Date: 2026-11-02. Status: source-grounded architectural assessment for report integration;
not a new ADR, implementation change or fresh qualification.

Retain Nara's separation of persistent meaning, authored intent and executable instances.
Complete the already selected source/compiled representation split before broadening publication
machinery. Stable identities reduce incidental migrations; immutable admissions prevent stale
interpretation. Neither eliminates semantic migration, topology correspondence or code retirement.
Claims that finer publication granularity reduces total cost still need the same consumer measured
under competing implementations.

## Baseline And Authority

Only local sources were inspected, at these verified HEADs:

| Repository | Commit |
| --- | --- |
| Nara | `a2640a10973b6d1724176a506237bc458aa5f3bc` |
| Bevy | `ae6152152d6fb8308ad6038314b48ef295e80fbc` |
| Fyrox | `b5b151b603bba0549ac03db676568726f17e6807` |
| Godot | `95c0fa5fe8a019adc29d074c16b64469b04571f7` |

These are source snapshots, not claims about stable releases. In particular, Bevy identifies its
scene dependency as `0.20.0-dev` ([Cargo.toml:801](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/Cargo.toml#L801)).
Nara's product criterion is complete production workflows and independently usable modules
([STRATEGY.md:16](../../../STRATEGY.md#L16)).
[ADR0119:23](../../architecture/adr/0119-schema-first-semantic-metadata-authority.md#L23)
accepts Schema authority; [ADR0120:3](../../architecture/adr/0120-schema-publications-and-execution-representations.md#L3)
remains Proposed. The [ledger:31](../../architecture/adr/implementation-status.md#L31) records bounded
CP1/S2 qualification. [CP2:3](../../plans/2026-11-02-prefab-extraction-plan.md#L3) explicitly leaves flat
compiled storage, root/placement replacement and complete extraction unfinished. Those are planned
work, not newly discovered bugs. No Cargo, network access or subagents were used.

## Different Facts Have Different Lifetimes

An authored override equal to today's default still means "keep this value after the default
changes." A runtime snapshot cannot reconstruct that choice. Similarly, a stable entity address
does not identify one allocation, and a schema catalog does not authorize executable code.

| Fact | Changes when | Consequence |
| --- | --- | --- |
| Type, field, owner and source identity | Meaning or ownership deliberately changes | Preserve through spelling changes; migrate intentional identity changes. |
| Authored declarations and overrides | The author edits intent | Save these, including hidden defaults and dormant declarations. |
| Compiled effective values and references | Source, dependencies or interpretation changes | Rebuild an explicitly derived product; do not infer original intent from it. |
| World instances, tokens and code publications | Activation, replacement or reload occurs | Reject stale capabilities even when logical identities remain equal. |

Nara already distinguishes component versions, catalog generations and predecessor fingerprints
([schema.rs:237](../../../crates/nara_reflect/src/schema.rs#L237),
[schema.rs:372](../../../crates/nara_reflect/src/schema.rs#L372)). Its nonserializable
`WorldEntityToken` separately includes domain and incarnation
([domain.rs:163](../../../crates/nara_identity/src/domain.rs#L163)). Collapsing these clocks would make
a body-only edit resemble a data migration, or allow a saved identity to authorize stale mutation.

## Comparison Matrix

References identify mechanisms, not equivalence of guarantees.

| Dimension | Nara | Bevy at the pin | Fyrox at the pin | Godot at the pin |
| --- | --- | --- | --- | --- |
| Metadata authority | Explicit semantic Schema; native provider needs `Component`, not `Reflect` ([authoring.rs:15](../../../crates/nara_reflect/src/authoring.rs#L15)). | Reflection registry and type-path lookup for serialization ([registrations.rs:40](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_reflect/src/serde/de/registrations.rs#L40)). | `Reflect` exposes runtime metadata; `Visitor` stores a named intermediate tree ([reflect/mod.rs:72](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-core/src/reflect/mod.rs#L72), [visitor/mod.rs:21](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-core/src/visitor/mod.rs#L21)). | SceneState tables contain named properties and Variant values, ownership and instances ([packed_scene.h:59](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.h#L59)). |
| Source versus execution | Authored records retain contextual defaults; expanded and processed candidates still share `SceneDocument` storage ([document.rs:189](../../../crates/nara_scene/src/document.rs#L189), [runtime_scene.rs:178](../../../crates/nara_scene/src/runtime_scene.rs#L178)). | BSN composition resolves templates before applying them; world serialization separately uses `DynamicWorld` ([resolved_scene.rs:153](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_scene/src/resolved_scene.rs#L153), [dynamic_world.rs:17](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_world_serialization/src/dynamic_world.rs#L17)). | Model instantiation copies a graph and records inheritance origins ([model/mod.rs:477](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/resource/model/mod.rs#L477)). | PackedScene packs node state and later instantiates it with base-scene/instance handling ([packed_scene.cpp:222](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L222), [packed_scene.cpp:873](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L873)). |
| Inheritance intent | Versioned property Set/Absent and explicit removal-to-inherit, separate from Undo ([overlay.rs:123](../../../crates/nara_scene/src/prefab/overlay.rs#L123)). | Templates compose into a resolved scene, including cached-scene precedence ([resolved_scene.rs:158](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_scene/src/resolved_scene.rs#L158)). | Modified flags prevent inheritance; unmodified fields normally serialize nothing ([variable.rs:332](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-core/src/variable.rs#L332)). | Packing compares inherited defaults but retains pinned properties ([packed_scene.cpp:1061](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L1061)). |
| Missing module | Bounded unavailable records retain value, owner/version and contextual defaults ([authoring_decode.rs:2381](../../../crates/nara_scene/src/authoring_decode.rs#L2381)). | This reflection deserializer requires registered type paths; spawning requires reflected component bindings ([dynamic_world.rs:117](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_world_serialization/src/dynamic_world.rs#L117)). | Missing script constructors error; Base ignores script visitation failure to keep the node editable ([base.rs:1157](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/scene/base.rs#L1157), [base.rs:1213](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/scene/base.rs#L1213)). | Configured missing-class handling can record properties in MissingNode/MissingResource ([packed_scene.cpp:389](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L389), [resource_format_text.cpp:565](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/resource_format_text.cpp#L565)). |

The old `DynamicScene` type is absent from the inspected scene/serialization implementation.
The relevant comparison is `DynamicWorld` plus BSN/Template composition. The BSN file crate exposes lexer/parser/AST modules
([bevy_bsn/src/lib.rs:1](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_bsn/src/lib.rs#L1));
scene documentation still describes `.bsn` assets as future work
([bevy_scene/src/lib.rs:78](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_scene/src/lib.rs#L78)).
Neither parser presence nor inline composition proves a complete persistent editor workflow.

## One Source-To-Runtime-To-Editing Loop

Consider two instances of a Prefab containing a reference-bearing default, with one explicit
property pin and an unrelated unavailable component.

1. **Admit source meaning.** `SceneComponentRecord` carries owner, version, value and contextual
   defaults, including defaults hidden by explicit values. Source classification distinguishes
   unknown, future, unmigratable and known-unbound records
   ([document.rs:197](../../../crates/nara_scene/src/document.rs#L197),
   [authoring_decode.rs:1781](../../../crates/nara_scene/src/authoring_decode.rs#L1781)). Preserving
   recognized envelopes and canonical values is not permission to accept arbitrary future syntax.
2. **Interpret composition.** The staged compiler closes structure, composes property programs,
   materializes values and closes surviving reference demands before emitting output
   ([staged.rs:177](../../../crates/nara_scene/src/prefab/overlay/compile/staged.rs#L177)). Final lowering
   resolves authored references and clears contextual declarations
   ([output.rs:243](../../../crates/nara_scene/src/prefab/overlay/compile/staged/output.rs#L243)).
   Provenance retains canonical source and instance ancestry independently of generated locators
   ([provenance.rs:64](../../../crates/nara_scene/src/provenance.rs#L64)).
3. **Bind and execute.** Expanded candidates require a containing source. Scene preparation performs
   preflight and prepares component application; publication installs components, source associations,
   hierarchy and occurrence identity ([candidate.rs:151](../../../crates/nara_scene/src/candidate.rs#L151),
   [spawn.rs:456](../../../crates/nara_scene/src/spawn.rs#L456),
   [publication.rs:303](../../../crates/nara_scene/src/spawn/publication.rs#L303)). Processed decoding
   separately admits exact definitions and flat data without source resolution, migrations or codecs
   ([runtime_scene.rs:145](../../../crates/nara_scene/src/runtime_scene.rs#L145)). Authoring preview can
   omit unsupported native coverage explicitly; that preview is not complete execution readiness
   ([native_preview.rs:31](../../../crates/nara_scene/src/prefab/native_preview.rs#L31)). With the
   unavailable record still selected, strict activation refuses before World mutation
   ([authoring_decode.rs:772](../../../crates/nara_scene/tests/authoring_decode.rs#L772)); restoring
   compatible definitions and bindings enables the complete runtime path.
4. **Edit original declarations.** Revert prepares an exact-revision source transaction through the
   existing history owner ([prefab_declarations.rs:165](../../../crates/nara_scene/src/authoring/prefab_declarations.rs#L165)).
   The conversion consumer commits, replays history, writes JSON/RON and freshly reopens source
   ([prefab_conversion.rs:632](../../../crates/nara_scene/tests/prefab_conversion.rs#L632)). Changes to
   effective runtime values alone cannot recover the pin, default environment or unavailable record.

The earlier follow-up's reproduced [F1:37](../../audits/2026-11-02-refactor-follow-up-review.md#L37) supplies the
counterexample: conversion succeeds but loses a required contextual-default declaration, so fresh
source admission fails. It was not reproduced again here. This exposes an existing conversion
boundary, not the absence of CP2 extraction. The equally established
[F2:72](../../audits/2026-11-02-refactor-follow-up-review.md#L72), identical upgrade retry colliding with retained
recovery storage, remains a bounded publication-attempt issue.

## Four CTO Decisions

### 1. Keep Schema Authority; Do Not Build A Universal Object Runtime

Nara's stable field/type identities, tombstones and owner lineage let persistence outlive native
type names and unavailable executors. That directly serves independently installed modules.
The [provider interface:15](../../../crates/nara_reflect/src/authoring.rs#L15) and unavailable-record
storage establish a concrete advantage over making every authored definition a reflected Rust
type. Stable IDs preserve addresses, not arbitrary wire encodings: changed stored keys or field
meaning still require compatible decoding or migration.

Bevy's type paths can themselves be customized, so "every Rust rename breaks Bevy data" would be
false ([type_path.rs:49](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_reflect/src/type_path.rs#L49)).
Nevertheless, stable paths alone do not supply Nara's field lineage, owner evolution or missing-owner
authoring policy. Replacing Nara Schema with Bevy reflection would save declaration infrastructure
but require rebuilding those semantics elsewhere. Serde-only records would be cheaper still, while
leaving reference traversal and migration to individual consumers.

The converse limit matters: retained unknown data does not reveal its intended semantics. A
topology-changing operation must refuse affected unknown meaning, while unrelated edits preserve
it. Godot's missing-class placeholders demonstrate another viable preservation mechanism; Nara
need not copy its live-object representation to deliver the same user outcome.

### 2. Make Compiled Storage Closed, While Preserving Authoring Intent

The current immutable wrapper owns `Arc<SceneDocument>` and exposes a `plain` constructor whose
admission occurs later ([candidate.rs:31](../../../crates/nara_scene/src/candidate.rs#L31)). This
prevents independent document/provenance substitution, but cannot make source-only states
unrepresentable. CP2's selected separate compiled storage is the appropriate next boundary
([plan:283](../../plans/2026-11-02-prefab-extraction-plan.md#L283)), supported by F1 rather than invented
as another architecture project.

Bevy's [ResolvedScene:153](https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_scene/src/resolved_scene.rs#L153)
already distinguishes a derived template program from its composition.
Fyrox's modified flags and Godot's pinned-property handling show why equality to the current base
cannot erase author intent. Nara's durable overlays correctly separate that intent from exact
Undo operations ([overlay.rs:179](../../../crates/nara_scene/src/prefab/overlay.rs#L179)).

A shared record type minimizes conversion code but requires interpretation checks at more boundaries.
Closed compiled records cost a deliberate lowering boundary and additional owned storage where both
forms are retained. A completely separate compiler would duplicate defaults/reference semantics;
reuse the existing interpreter and change its terminal output. This reduces semantic ambiguity;
runtime speed and memory savings remain unmeasured.

### 3. Freeze Identity Meaning Before Freezing Extraction Mechanics

Nara separates logical occurrence, canonical source routes and runtime incarnation
([occurrence.rs:9](../../../crates/nara_scene/src/occurrence.rs#L9),
[provenance.rs:118](../../../crates/nara_scene/src/provenance.rs#L118)). That avoids treating allocation
bits or lookup aliases as persistent identity. It does not guarantee continuity when extraction
changes declaration ownership and instance routes.

The references also distinguish several identity axes. Fyrox persists node instance IDs,
randomizes them on Prefab instantiation, and separately tracks resource origins; inheritance resolves
origins by configured names or handles ([base.rs:456](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/scene/base.rs#L456),
[fyrox-graph/src/lib.rs:1384](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-graph/src/lib.rs#L1384)).
Godot separates resource UID-to-path mapping, scene-local node IDs and ID-path recovery
([resource_uid.cpp:169](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/io/resource_uid.cpp#L169),
[packed_scene.cpp:1180](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L1180),
[packed_scene.cpp:2024](https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/resources/packed_scene.cpp#L2024)).

CP2's root binding and mixed local/inherited sibling ordering require a source hierarchy replacement
([plan:55](../../plans/2026-11-02-prefab-extraction-plan.md#L55)). An extra wrapper is cheaper but changes
transforms and identity; forcing single-root Prefabs discards a demonstrated forest consumer.
Preserving leaf IDs alone misses changed ancestry. Require explicit old/new declaration and occurrence
correspondence, including dormant references, before promising extraction continuity. Avoid a universal
alias registry until a consumer needs continuity beyond that bounded operation.

### 4. Retain Immutable Admissions; Measure Their Granularity

Immutable owner snapshots and nominal authoring bases keep old work attached to its actual
interpretation ([schema_owner.rs:23](../../../crates/nara_reflect/src/schema_owner.rs#L23)). This prevents
accidental reinterpretation; it does not perform migration. CP1 deliberately saves reviewed working
copies and establishes new history checkpoints instead of translating every old inverse
([upgrade plan:32](../../plans/2026-11-01-authored-schema-upgrade-plan.md#L32)). That is a defensible
reduction of migration scope, paid for by explicit loss of pre-upgrade Undo continuity.

Fyrox instead supports local reader compatibility, such as old plain values becoming inheritable
variables ([variable.rs:357](https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-core/src/variable.rs#L357)).
That is simpler for bounded changes, but does not itself prove a coordinated multi-document upgrade.
Nara should retain both local decoding migration and explicit project evolution where each applies.

The overengineering risk is implementing every proposed publication/cohort mechanism before it
beats full reconstruction on the same consumer. ADR0120 itself retains restart and distinguishes
data-only snapshots from executable leases ([ADR0120:99](../../architecture/adr/0120-schema-publications-and-execution-representations.md#L99)).
Whole-runtime reconstruction costs interruption but simplifies physical lifetime; mutable registries
reduce apparent setup work while spreading invalidation; immutable owner publications add dependency
compatibility and retirement obligations. No inspected source establishes a universal cost winner.

## Freeze Semantics; Delay Unproven Mechanisms

Freeze identity versus incarnation; Set/Absent/Revert meaning; lexical reference/default scope;
lossless preservation of unaffected unavailable records; explicit refusal when affected meaning is
unknown; source-to-compiled admission; and truthful upgrade/recovery checkpoints. These determine
whether future files and edits retain meaning. Reconcile their owning ADRs when CP2 changes them.

Delay generic migration languages, arbitrary Undo translation, universal cross-owner hot replacement,
global continuity aliases and additional storage families. Also avoid freezing current materialized
locator spelling or requiring every execution change to advance Schema generation. The existing
CP1 checkpoint and Proposed0120 fallback already provide narrower mechanisms; replace them only
when a measured consumer demonstrates the missing behavior. Freeze the rule that admitted snapshots
retain their interpretation, while leaving snapshot size, cache layout and invalidation granularity
subject to that comparison.

## Runnable Consumers And Cost Comparison

The following are suggested sequential PowerShell checks, not executions performed for this report:

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo nextest run -p nara_scene --features serde --test inline_authored_references --test prefab_overlay_comparison --test authoring_decode --test occurrence_lifetime --test prefab_conversion
cargo nextest run --manifest-path tests/fixtures/behavior-languages/Cargo.toml --test authored_upgrade_command
```

The first cohort covers lexical defaults, equal-value pins, missing-owner preservation, incarnation
and conversion/history/persistence. The independent second cohort covers authored evolution through
public consumers. Fresh-process recovery tests are explicitly ignored unless their separately built
companion is supplied; default success must not count them
([process_recovery.rs:688](../../../tests/fixtures/behavior-languages/tests/support/authored_upgrade_session/process_recovery.rs#L688)).
F1/F2 need permanent regression coverage at their existing owners; the earlier temporary probes are
evidence, not tests asserted to exist here.

Use one EnemyStats project with two Scenes, repeated/nested Prefabs, inactive dirty documents,
dormant declarations and an unavailable owner. Compare complete source capture/recompile with
immutable sharing and affected-owner invalidation; compare restart with a bounded unchanged-Schema
body replacement. Measure edit-to-usable-result latency, peak retained bytes, interpreted records,
codec calls and actual retired instances, with identical correctness assertions and cold/warm runs.
Do not substitute logical budget counters for measured memory or latency.
The [compiler loop:184](../../../crates/nara_scene/src/prefab/overlay/compile/staged.rs#L184) repeats
composition when new reference demands expand the scope; measure that work alongside retained
captures. Use temporary comparison implementations, not two permanent interpreters. A result must
account for author-visible interruption and implementation/maintenance burden as well as throughput.

The CP2 successor should extract the selected root, preserve mixed sibling order and cross-cut
references, and establish the selected saved/history checkpoint. Then edit the new Prefab,
Revert, Undo/Redo those ordinary subsequent edits, save, freshly reopen and Check/Play. This does
not add Undo of the cross-file extraction itself. Assert
actual effective values, pin state and target identities, not merely successful compilation. This
extends the selected [CP2 acceptance:375](../../plans/2026-11-02-prefab-extraction-plan.md#L375);
it does not report that open workflow as a regression or as already implemented.
