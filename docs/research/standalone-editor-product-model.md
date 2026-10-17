# Standalone Editor Product Model: Historical Research

This is a synthesis of research collected on August 18, 2026. It preserves the
reasons for separating source authoring from executable project behavior, rather
than the original agent task definitions, repeated engine surveys, or proposed API
catalogues. It is research evidence, not a plan or implementation status report.

Current product scope belongs to [STRATEGY](../../STRATEGY.md). Current contracts are
[ADR 0114](../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
for generic authoring,
[ADR 0116](../architecture/adr/0116-standalone-supervised-editor-play.md) for supervised
Play, and [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
for workspaces, frontends, panels, and creation templates. Their later decisions
take precedence over the historical recommendations below.

## Original scope and limits

The seven source reports compared Godot, Unity, Unreal, Fyrox and Rust tools, the
then-current Nara Editor, candidate architectures, and editor extension models.
They used official documentation, upstream source, and Nara repository evidence.
No cross-engine latency, crash-rate, or usability benchmark was performed.

| Subject | Recorded research scope |
|---|---|
| Nara | `a6afc4bfb64ac87dbd086baaa071ff2a9608a457`, August 18, 2026 |
| Godot | 4.7.1-stable; documentation branch 4.7 |
| Unity | Mainly Unity 6.5 / 6000.5; individual Safe Mode and Shader Graph sources used 6000.2 and 6000.3 |
| Unreal | Version-neutral Unreal Engine 5.x documentation; some pages resolved to 5.8 |
| Fyrox | `d1e3469ca5cd17d9a88da26246593d5ef63564f0`, August 17, 2026 |
| Bevy | `e8b3598ff5e5ec40e8ba84edd5750a1c0e4d4e59`, August 7, 2026; separate inspector and overlay projects |

These are the reports' original version scopes, not a fresh verification of those
engines or their current APIs. In particular, the Nara report's egui, complete-schema
Open, in-process Play, and missing extension descriptions are historical snapshots.
Later generic recovery, external Play, and Dear ImGui work must be assessed through
their owning contracts and current implementation.

## Product choices that motivated the separation

The useful commonality was one recognizable editor product with source documents,
Undo, diagnostics, assets, and domain tools. The engines achieved this through
different execution models; a generic editor executable did not imply that every
custom type or tool was available without building or loading project code.

Godot illustrated generic project opening, resource-oriented tools, separate game
execution, and missing-type placeholders. Its script and native editor extensions
also showed why extension convenience and the editor's fault domain are separate
questions. [Project Manager](https://docs.godotengine.org/en/4.7/tutorials/editor/project_manager.html)
and [game embedding](https://docs.godotengine.org/en/4.7/tutorials/editor/game_embedding.html)
were the relevant product references.

Unity illustrated recovery from compilation errors and custom UI integrated with
serialization and Undo. Its managed assembly and reload machinery was a dependency
of that experience, not a mechanism directly transferable to Rust.
[Safe Mode](https://docs.unity3d.com/6000.2/Documentation/Manual/SafeMode.html) was a
recovery precedent, not proof of unrestricted scene editing while code was broken;
[SerializedObject](https://docs.unity3d.com/6000.5/Documentation/ScriptReference/SerializedObject.html)
was the reference for keeping custom inspectors inside existing editing semantics.

Unreal illustrated separate runtime/editor module roles and the power of deeply
integrated native tools. Its Live Coding and object reinstancing requirements also
showed the cost of treating structural native changes as in-place updates.
[Modules](https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-modules)
and [Live Coding](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime)
supported these tradeoffs; they did not establish a Rust reload contract.

Fyrox demonstrated substantial Rust editor functionality and command-based tools,
while its project-generated editor made compile-before-open coupling concrete.
Its [editor template](https://github.com/FyroxEngine/Fyrox/blob/d1e3469ca5cd17d9a88da26246593d5ef63564f0/template-core/src/lib.rs#L416-L483)
and [external Play implementation](https://github.com/FyroxEngine/Fyrox/blob/d1e3469ca5cd17d9a88da26246593d5ef63564f0/editor/src/lib.rs#L1619-L1762)
were separate lessons. The research identified a potential lossy boundary around
unavailable scripts, but did not run a destructive save/reopen experiment.

## Architecture alternatives considered

| Alternative | Historical assessment |
|---|---|
| Generic schema-driven editor with external game process | Preferred foundation: source recovery survives game build failure; arbitrary executable tools need an additional boundary |
| Project-compiled editor companion | Useful trusted integration path; unsuitable as the required recovery editor when that same project cannot compile |
| Native modules loaded into the generic editor | Rejected as the default: ABI, allocator, panic, callbacks, live-state migration, and unloading become editor correctness obligations |
| Declarative tools plus optional process, managed, or Wasm execution | Possible extension direction; larger protocol and lifecycle costs require concrete consumers |

The Rust constraint was specific: independently compiled Rust trait objects and
Rust-layout values are not a stable plugin ABI. A source-built companion, a semantic
process protocol, and a separately designed C-compatible boundary are different
contracts. The [Rust ABI reference](https://doc.rust-lang.org/reference/items/external-blocks.html#abi)
does not supply a general native hot-unload guarantee. Wasm was an optional research
candidate, not a selected second authoring language or universal editor interface.

## Boundaries worth retaining

Authoring schema describes durable identities and values; executable bindings supply
constructors, codecs, systems, and native behavior. A running game cannot be the
only source of schema if source recovery must work while the build is broken.
`ProductRecipe` remains compiled project composition, not a package manager or an
editor extension kernel.

Document identity, accepted revision, selection, Undo, dirty state, and Save belong
to the editor's semantic owners. Custom panels and inspectors should consume read
models and propose admitted actions. Toolkit objects, GPU handles, and live `World`
references cannot become a portable process protocol.

Authored source, imported artifacts, and runtime state have different lifetimes.
Keeping a last-good schema or artifact can preserve useful work after a failed build;
it does not prove compatibility with changed source or authorize restoring old data.
Play changes remain transient unless an explicit authoring transaction applies them.

| Failure or mismatch | Reason for an explicit boundary |
|---|---|
| Project compilation fails | The generic shell and admitted authoring documents must remain usable |
| Schema is known but native binding is absent | Data editing and strict runtime admission require different completeness checks |
| Schema or migration is unavailable | Preserve understood records; disable operations whose semantics cannot be established |
| File envelope or value grammar is unsupported | Unknown bytes cannot be safely rewritten as a partially understood document |
| Game or tool process crashes | Retire that execution attempt while retaining document ownership; a subprocess alone is not an OS sandbox |
| A late result targets an old generation or revision | Reject it instead of applying work to replacement state |
| A native tool shares the editor process | Treat it as fully trusted code with the same crash domain |

The original last-good recommendation must not be read as permission to restart an
old binary against arbitrary current files. ADR 0116 now requires matching saved
source receipts: divergence rejects Restart while retaining the running child;
Stop followed by Play targets current saved source. Its version-one protocol does
not promise the research's proposed remote inspection, Pause/Step, or frame transport.

## Questions the research did not settle

Independent package installation, executable extension trust, protocol/version
compatibility, isolation, and safe removal still require their own concrete product
workflows. The reports did not choose an RPC encoding, Wasm runtime, managed host,
stable native ABI, or universal widget protocol. See
[OQ-031](../architecture/open-questions.md#oq-031-product-package-contribution-and-trust-topology)
for the package question; ADR 0118's finite trusted source-built panels do not by
themselves answer it.

Embedded external Game View, remote inspection, animation and shader document models,
cross-machine support, and performance thresholds were also unresolved. Engine feature
breadth was not evidence that Nara needed all of those systems before a useful editor.

## Original records

The removed raw bundle contained `outline.yaml`, `fields.yaml`, and seven reports:
`nara_current_editor.json`, `nara_candidate_architectures.json`, `godot_editor.json`,
`unity_editor.json`, `unreal_editor.json`, `fyrox_and_rust_native.json`, and
`editor_extension_models.json`. Their full arguments, source lists, and speculative
API sketches remain in Git at revision `73c6857`:

```console
git show 73c6857:.codex-research/standalone-editor-product-model/outline.yaml
git show 73c6857:.codex-research/standalone-editor-product-model/results/editor_extension_models.json
```

Use the same prefix to recover any other original report. Old report paths and
section names identify that historical bundle, not additional current contracts.
