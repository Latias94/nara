# Input Definition And Runtime Ownership Review

Date: 2026-10-30. Status: bounded background research; implementation not started.

This review examines one manifest-selected physical-input-to-action workflow and its processed
startup successor. It does not change current Source Scene qualification or add input acceptance
to that unit. Successor execution follows the owning plan's source-before-processed dependency order.
Evidence below comes from the observed working tree, including concurrent Source Scene changes;
this document records no new build, test, native-platform or packaged-process qualification.

## Scope And Authority

The [strategy](../../STRATEGY.md) favors complete consumers over speculative engine infrastructure.
The [Input And Target-Build successors](../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md#input-and-target-build-successors)
require actual source support before processed input and preserve the current truthful refusal.
Accepted owners are [ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md),
[0049](../architecture/adr/0049-untrusted-project-input-and-parse-budget-policy.md),
[0051](../architecture/adr/0051-persistent-file-envelope-migration-and-golden-fixtures.md),
[0035](../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md),
[0095](../architecture/adr/0095-plugin-owned-specialized-domains-and-project-configuration.md),
[0007](../architecture/adr/0007-asset-identity-and-import-pipeline.md) and
[0122](../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md).
Candidate decisions here require narrow contract reconciliation before implementation.
No generic input framework, provider registry, engine-wide budget kernel or compatibility reader
is proposed. Target-build selection and Scene-owned context publication remain separate work.

## Confirmed Current Facts

| Boundary | Observed implementation |
| --- | --- |
| Source selection | [Input settings](../../crates/nara_project/src/sections.rs) lower `action_map` to `Option<ProjectPath>`; [profile overlays](../../crates/nara_project/src/profile.rs) can explicitly clear it. |
| Early refusal | [Runtime projection](../../crates/nara_project/src/runtime_configuration.rs) returns `runtime.configuration.source-input-required`; [manifest ingest](../../src/project_host/ingest.rs) calls it before constructing the candidate. |
| Persistence | [ActionMap serde](../../crates/nara_input/src/lib.rs) accepts bindings and disabled contexts, then rebuilds its index through `bind`. It has no persistent envelope or file-admission limits. No source loader installs the requested file. |
| Compilation and state | The same resource owns bindings, a `BTreeMap<InputBinding, Vec<usize>>`, disabled contexts and revision identity. `bind` changes mapping lifetime; `disable_context` can retain arbitrary context names. Derived `Clone` copies mutable configuration. |
| Semantic ownership | [SemanticActions and ActionStaging](../../crates/nara_input/src/semantic.rs) aggregate physical contributions and preserve observed mapping/context exits across configuration restoration. [Timeline](../../crates/nara_input/src/timeline.rs) retains staged lifetime observations. |
| Existing configuration | [Reference Game](../../reference-game/src/input.rs) and [two-level game](../../samples/two-level-game/src/gameplay/input.rs) install physical bindings and action-to-command mappings during plugin construction. A file map would compete with their physical definitions. |
| Source custody | [Content snapshots](../../src/project_content.rs) retain actual decoded content and leases. [Closure members](../../src/project_content/closure.rs) have no input role or captured input definition. |
| Startup window | [Host preparation](../../src/project_host/runtime.rs) instantiates plugins, executes candidate admission/materialization, then calls `complete_startup`. Installation can happen before Startup in the unpublished World. |
| Scene replacement | [Residency replacement](../../src/project_host/runtime/content_residency/replacement.rs) replaces `ProjectDomainAssets`. That collection alone cannot own a global input lifetime. |
| Processed custody | [Typed acquisition](../../src/runtime_content.rs) retains its actual product, original base and decoded-output reservation. [Codec selection](../../src/runtime_content/codecs.rs) freezes family, implementation, role and limits independently of source import. |

The actual path is normalized ingress, First projection/UI routing/staging, PreUpdate action
resolution, then [semantic command mapping](../../crates/nara_gameplay/src/lib.rs) and gameplay.
`ActionOutcomes` reports physical binding edges; `SemanticActions` owns aggregate held state.
Context cancellation is not a fabricated physical release and requires a fresh subsequent press.

## Pinned Reference Comparison

- Bevy `85503941e488152711a4695bbe248e9ed9f453fc`:
  [InputPlugin](../../repo-ref/bevy/crates/bevy_input/src/lib.rs) initializes normalized resources
  and schedules their update. This supports the ingress/state boundary, not a Nara action-file
  format or a context ownership decision. No external action framework is inferred from it.
- Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`:
  [InputMap](../../repo-ref/godot/core/input/input_map.h) owns action definitions, while
  [Input::ActionState](../../repo-ref/godot/core/input/input.h) owns delivered state.
  [Project-settings loading](../../repo-ref/godot/core/input/input_map.cpp) clears and rebuilds a
  global mutable map. Definition/state separation is useful evidence; global reload is not a
  suitable Nara Scene-travel installation contract. Neither reference proves Nara's custody costs.

## Proposed Decisions, Not Adopted Contracts

1. Introduce immutable `ActionMapDefinition`, a finite code-first `ActionMapBuilder`, and a strict
   source-file candidate in `nara_input`. Keep `ActionMap` as World-local runtime state. Replace
   live-resource persistence rather than preserving its draft shape behind another reader.
2. Prefer a canonical v1 JSON envelope, kind `action_map`, containing a complete primary Product,
   declared contexts with initial enable state, and ordered key/mouse bindings. Intrinsic source
   Product identity is still a candidate: compare it with an authored sidecar before adoption.
   Intrinsic identity avoids duplicate authority for this native format; a sidecar integrates with
   existing metadata discovery but adds another receipt and consistency obligation. Do not mint
   identity during read, derive it from content/path, or add an Input `SourceDocumentKind`.
3. Candidate initial caps: encoded 256 KiB, 1,024 bindings, 64 contexts, 128 UTF-8 bytes per ID,
   fanout 32 per physical input, depth 16, 16,384 nodes and 128 KiB total string bytes. These are
   proposed values, not measured defaults or an accepted runtime memory promise. Derive checked
   compile-work, retained-output and runtime-state allowances and qualify their exact boundaries.
4. `SourceRuntimeProjection { runtime, input_requirement }` is a candidate API, not existing code.
   It would retain the original source selection as an obligation while admitting shared settings.
   It must not be implemented by clearing `input.action_map` and invoking the old projection.
   Source-independent projection continues refusing an unresolved source path.

Reuse [core shape preflight](../../crates/nara_core/src/serde_shape.rs) and the
[persistent envelope](../../crates/nara_core/src/format.rs). Require object records, reject unknown
and duplicate fields, positional-record alternatives, bad versions, undeclared contexts and
duplicate complete bindings. Different actions may share an input. Apply domain counts before
their allocation; a post-decode vector-length check is insufficient. Preserve binding order in
the compiled input index. Chords, analog values and a second source syntax are outside this unit.

## Ownership Challenges The Design Must Resolve

| Challenge | Required property |
| --- | --- |
| Immutable definition | Compile once per admitted capture into finite data and indices. No mutable contexts, held state, Host paths or execution authority belong in the definition. |
| World-local state | Enable/disable only declared contexts. Each independently mutable state needs its own admission; immutable bindings may share one owner. Remove implicit live-resource cloning or replace it with an explicitly admitted operation. |
| Nonrollback staging authority | Preserve `ActionStaging`'s non-reverting observations. Restoring old definitions or context snapshots cannot revive a retired hold or paused staged press. Persistent IDs and serialized revisions cannot replace this authority. |
| Once before Startup | Install through the existing unpublished candidate admission before Scene materialization and Startup. Close the installation phase; reject duplicate/late installation and a missing actual Input owner. |
| Competing plugin configuration | Distinguish pristine plugin defaults from explicit code configuration, including explicit empty configuration and resource replacement. Comparing empty bindings or default equality is insufficient. Keep action-to-command mapping game-owned. |
| Non-Scene residency | Global bindings and context state survive successful travel unchanged except for explicit context operations. Failed travel preserves them. Scene CPU collection replacement must not release or reinstall them. |
| Escaped bindings | Every owning bindings handle retains actual compiled data and its original source/acquisition charge. A copied definition plus a lease elsewhere does not establish custody. |

Do not share `Arc<Mutex<ActionMap>>` between Worlds. An immutable handle should borrow its
definition through a concrete owner; it must not expose a cloneable inner allocation without
custody. Processed handles retain the actual `RuntimeContentAsset<ActionMapAsset>`, not a clone
of `product()` detached from that acquisition. Source handles retain their compiled output and
generation owner without retaining the whole Scene graph back through a cycle.
Input-owned cost queries must include context tables and retained revision/staging snapshots,
using existing finite timeline/route limits. Source Hosts reserve through `ProjectContentBudgetHost`;
processed Hosts use the original base's retained admission. This needs no new shared budget kernel.

## Ordered Executable Successor Units

1. **Finite input owner.** Add `crates/nara_input/src/definition.rs` and `format.rs`; adjust
   `lib.rs`, `semantic.rs`, `timeline.rs` and `Cargo.toml`. Establish candidate decode, builder
   compile, immutable owner and cost APIs before Host integration. Keep current cancellation and
   multi-binding behavior; do not serialize runtime state as authoring data.
2. **Explicit source obligation.** Reconcile ADRs 0035/0041/0051, then change
   `crates/nara_project/src/runtime_configuration.rs`, `src/project_host/ingest.rs` and
   `src/project_host/composition.rs`. Retain selection through candidate/plan admission. Reject a
   required input owner absent from composition; actual receiving authority is checked again later.
3. **Actual source capture.** Add `src/project_content/input.rs`; extend `project_content.rs`,
   `budget.rs`, `closure.rs` and `delivery.rs`. Read the exact selected project-relative file via
   the real project capability, not an inferred Assets scope. Reserve encoded/compile/output costs
   before work; retain definition and identity; record an `InputActionMap` member before digest
   publication. Join admitted source identities and reject conflicting UUIDs or canonical aliases.
4. **Startup and source consumer.** Install from `src/project_host/runtime.rs` before Startup,
   using Input-owned installation authority and existing failed-candidate retirement. Retain the
   global owner in source request context; travel may verify the original receipt in its worker
   and reuse that owner. A changed input source rejects the candidate pending explicit restart.
   Extend `src/project_workflow/export.rs` for the receipt role; Check and source-carrying Export
   consume the same captured facts, without another dependency discoverer or action-map editor.
5. **Processed startup contract, after source acceptance.** Add an optional complete action-map
   Product to admitted Runtime configuration. Use the existing Product type, not shadow UUID
   strings. Catalogue validation requires that member and expected format independently of Scene
   roots or addressable travel permission. `nara_input` owns `ActionMapDomain` and a
   `runtime_action_map` v1 codec with explicit empty Required-reference inventory. Select that
   codec through the original configuration plan and frozen limits; family identity alone grants
   no decode authority. No fake source importer is needed for a Runtime-only facet.
6. **Processed preparation and process proof.** Extend `src/runtime_content.rs`,
   `runtime_content/catalog.rs`, `codecs.rs` and Host startup with a separate retained startup-input
   owner. Acquire once, reserve independent World state, and install through the same receiving
   operation. Do not place sole custody in `ProcessedSceneContent` or Scene collections. Preserve
   code-first input when no Product is selected; never reopen source on a missing/invalid member.

Steps 1-4 must produce a complete source workflow before steps 5-6 claim processed support.
A parser, successful codec, catalogue entry or fabricated semantic command cannot substitute for
physical-input acceptance. This review records design work; implementation has not started.

## Acceptance And Concrete Test Locations

| Scope | Existing evidence locations and proposed additions |
| --- | --- |
| Format/compile | Add `crates/nara_input/tests/action_map_format.rs`: golden bytes, strict shape/version/identity, all finite boundaries, deterministic compile/encode, exact/one-short costs and failed publication. |
| Runtime semantics | Extend [semantic_actions.rs](../../crates/nara_input/tests/semantic_actions.rs) and [timeline_runtime.rs](../../crates/nara_input/tests/timeline_runtime.rs): held cancellation once, fresh press, paused old input, configuration restore and independent endpoints. |
| Source admission | Extend [runtime_configuration.rs](../../tests/runtime_configuration.rs) and [project_content_boot.rs](../../tests/project_content_boot.rs); add `tests/project_input_source.rs` for real bytes, identity, receipts, source mutation and budget release. |
| Host installation | Extend [project_runtime_boot.rs](../../tests/project_runtime_boot.rs); add `src/project_host/runtime/tests/source_input.rs` for first Startup observation, missing owner, conflicting/empty configuration, late/duplicate install and preserved travel state. |
| Independent source consumer | Add `tests/fixtures/module-consumer/tests/source_input_workflow.rs`, following [source Scene requests](../../tests/fixtures/module-consumer/tests/source_scene_references.rs). Extend [source Export tests](../../src/project_workflow/export/source_tests.rs) for exact receipt copying and relocation. |
| Processed successor | Extend [processed_start.rs](../../src/project_host/runtime/tests/processed_start.rs), [codec tests](../../src/runtime_content/codecs/tests.rs) and [catalogue tests](../../src/runtime_content/catalog/tests.rs); add a prebuilt processed-input process fixture. |

The complete consumer starts in A with file-defined key/mouse bindings, observes semantic action
provenance and actual gameplay effect, disables a held context with one cancellation, re-enables
without reviving it, then releases and presses again. Exercise rejected travel and A-to-B-to-A
successful travel with the same global input owner and explicit enable/disable operations.
Prove that escaped bindings keep actual data and charges until the last owner drops, that separate
mutable state is separately charged, and that failure/cancellation returns reservations without
publishing partial state. Include a no-file code-first control and a competing plugin control.
For processed acceptance, launch a prebuilt executable with only processed members and its normal
platform runtime dependencies. Make source, metadata, cache, manifest and Cargo unavailable during
execution. Exercise the same physical route in the child; malformed/missing members fail closed.
Normalized driver injection proves the platform-neutral physical-to-action boundary; a native
keyboard/window claim additionally needs the corresponding native route. Keep platform claims
specific, and keep build/export target selection outside this input qualification.
