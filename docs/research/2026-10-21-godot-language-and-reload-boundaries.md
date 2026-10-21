# Godot Language And Reload Boundaries For Nara

Date: 2026-10-21. This is source/document research, not an adopted architecture decision.
Nara basis: `6bb1a355c2e09d60979b063569b1de2237e25163`. Godot source basis:
`81ab961f91066a63bcbf3ec340962bb1a1ada558`; its `version.py` identifies **4.8 beta**.
Official documentation basis: **4.5** branch of `godotengine/godot-docs`, pinned at
`33e08e14c517541fac3d9069e82fb3ee2fc99a10`, retrieved on the date above.
Source findings describe the pinned source; the 4.5 manuals supply separately versioned guidance.

## Finding

Godot provides several separately engineered reload paths. It does not establish that selecting
a script language makes arbitrary running state, native memory, callbacks, or references safely
replaceable. The useful precedent for Nara is an explicit supported state contract and a visible
restart boundary. It supports the separation already accepted by
[ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md),
not a mandatory second language or a universal native ABI.

| Path | Replacement unit | Retention mechanism | Important limit |
| --- | --- | --- | --- |
| GDScript | VM-owned script implementation | Compatible named Variant members; selected static data and function references | Suspended execution is canceled; changed/new members and references have separate rules |
| C# | Collectible project assemblies in the Editor | Serialize supported instance state, recreate managed instances, deserialize | Hard reload; unsupported state and unloading failures remain possible |
| GDExtension | Native extension library in an editor build | Track Godot object identities, recreate extension instances, restore stored properties | Requires binding support; private native memory and arbitrary ABI changes are not migrated |

## GDScript: What `keep_state` Keeps

`GDScript::reload(false)` refuses to reload while instances exist. With `true`, compilation can
update the implementation used by existing instances. In this source revision the language-level
`reload_scripts` calls `reload(true)`; there is no public soft/hard selector in that method's
signature. Here, *soft* means keeping instances and transferring supported VM state; *hard* means
recreating instances and restoring selected data. These are descriptive distinctions, not a claim
that every language shares one interchangeable reload API. [^G1][^G2]

Instance members are rebuilt into a new Variant array. The implementation matches the old member
by name and retains its value only when the new declared type accepts that value. Otherwise it
constructs an empty value for the new type. A renamed field therefore has no automatic rename
migration, and a newly introduced member is not guaranteed to receive its source initializer as
if the whole object had just been constructed. This is VM-controlled storage migration, not
preservation of a C++ or Rust object layout. [^G3]

Transferred Variant values retain their ordinary value/reference semantics; this is not a deep
checkpoint of every object reachable through them. Godot object identity, a script instance, a
RefCounted reference, and a callable are different preservation questions. Function/lambda
references have an explicit old/new mapping pass; unmatched replacements can become null.
Compilation preparation also calls `cancel_pending_functions(true)`, clearing suspended execution
and warning about the reload. `keep_state` therefore does not mean keeping an arbitrary stack,
await continuation, or every retained callback executable. [^G4][^G5]

In tools-enabled builds, a runnable previously valid script saves static variable data before
compilation. After static initialization, it restores surviving names and recurses into inner
classes. This contradicts a blanket claim that script reload always resets statics. Conversely,
the name-based restoration code is not evidence of semantic migration after a static type change,
nor a promise that static initializer side effects did not run. [^G6][^G1]

`@tool` scripts run in the Editor. Adding/removing `@tool` reconstructs the script instance or
placeholder and restores saved property state; failed instantiation can retain pending state.
The 4.5 manual warns that tool code can crash the Editor by freeing nodes involved in Editor work,
and states that tool-script breakpoints are unsupported in that version. Running author code
inside an Editor is therefore a separate lifecycle/debugging product, not free isolation. [^G2][^D1]

Parsing, analysis and compilation can fail. The implementation sets validity and mutates compiler
state along this path; the presence of an error return does not establish transactionally retained
last-good execution. No failure-rollback or migration experiment was performed here. [^G1][^G5]

## C#: Assembly Reload Is Reinstantiation

`CSharpLanguage::reload_assemblies` returns outside the Editor because the game player disables
collectible assemblies. Its comment explicitly describes this path as always hard reload.
Reloadable scripts are collected in inheritance order; eligible managed delegates are serialized;
native RefCounted owners are kept alive; instance state is saved; script instances are removed;
project assemblies are unloaded/reloaded; managed script instances are recreated; and state is
restored. Existing Godot object identity is not the same thing as preserving managed object
identity or an arbitrary managed object graph. [^C1]

The source generator chooses non-static, Godot-compatible instance fields and readable/writable
properties. It excludes readonly fields, indexers, init-only properties, and ignored members.
This is broader than Inspector-exported fields but narrower than all C# memory. Restoring by
property name and converting from Variant is not a schema rename or semantic migration protocol.
`ISerializationListener` supplies `OnBeforeSerialize` and `OnAfterDeserialize` for explicit repair.
Managed delegate serialization can fail, in which case handles still have to be released to allow
assembly unloading. Statics are excluded by this generated instance-state mechanism. [^C2][^C3][^C1]

Assembly unloading or loading can fail. The language path preserves affected state in placeholders
instead of assuming old code resumed; repeated assembly-load failures eventually stop retries and
request an Editor restart. This is evidence of recovery work and limits, not universal atomic
rollback. [^C1][^C4]

The 4.5 manual requires both the .NET-enabled Godot build and a separately installed .NET SDK.
It describes built-in C# editing as minimal, points to external IDEs for completion/debugging,
and requires rebuilding assemblies to expose new exported variables or signals in the Editor.
A mature managed integration still carries compiler, binding-generation, IDE and packaging work.
These are Godot-specific examples of cost categories, not an estimate of Nara's engineering time. [^D2]

## GDExtension: Library Reload Requires An Object Contract

The 4.5 manual describes `reloadable` as mainly a development/debugging feature, supported by
godot-cpp from Godot 4.2, with other bindings responsible for their own support. In the pinned
source, `reload_extension` is restricted to editor builds and checks global enablement, recovery
mode, loading status and `reloadable`. A constructible class lacking `recreate_instance_func`
disables reloading for that extension. Merely producing a new dynamic library is insufficient. [^D3][^E1][^E2]

The engine tracks extension instances by Godot ObjectID. `prepare_reload` saves storage properties,
skipping some default values and null objects according to usage flags. Clearing the extension
removes its private native portion while the Godot object temporarily becomes its native parent.
After loading/re-registration, `finish_reload` resets extension portions, restores properties,
invalidates removed methods/classes, and emits `NOTIFICATION_EXTENSION_RELOADED`. Native private
fields, statics, threads and arbitrary allocations are not covered by that property backup. [^E3]

The registration path rejects changing an existing class's native parent and restricts changing
its runtime-class identity. Method binds are reused only if the compatibility check succeeds;
otherwise old binds become invalid and new ones are created. This is a concrete binding/object
protocol, not proof of a stable Rust dynamic ABI or arbitrary component-layout replacement. [^E2][^E4]

Reload orders preparation, unload, close, open, load, and finishing. Intermediate failures return
without a general restoration of the former library. No claim of fault-free extension callbacks,
quiescent user threads, or automatic last-good native rollback follows from this control flow. [^E1]

## SceneTree, Servers, And ECS Are Different Choices

Godot's public scene model is a tree of `Node : Object`; `SceneTree : MainLoop` processes scene
work and frame/physics events. A physics node creates and addresses a PhysicsServer body through
an RID-backed service boundary. These source facts describe an object/scene authoring model over
specialized services, not a universal ECS world/query/schedule model. They also do not prove that
every internal Godot subsystem has one uniform storage or execution architecture. [^S1][^S2]

The 4.5 server manual explains that rendering, physics and audio servers sit below the scene
system and can be used directly. It describes RID ownership obligations and warns against
independently controlling server objects already owned by a node. That article is explicitly
marked outdated; this study uses it for the broad layering model, corroborated by source, not
for universal threading or performance claims. The node-alternatives guide also offers Object,
RefCounted and Resource for lighter responsibilities. [^D4][^D5][^S2]

Thus "Godot is not ECS everywhere" does not justify Nara building its own ECS. Godot instead
illustrates that authoring hierarchy, domain ownership and efficient low-level storage can have
different boundaries. [ADR 0002](../architecture/adr/0002-use-bevy-ecs-as-ecs-substrate.md) already
assigns ECS query/borrow/schedule mechanics to `bevy_ecs` and product semantics to Nara. A change
requires a concrete Nara consumer and unacceptable measured/API cost, not architectural imitation.

## Language Choice For Nara

The following comparison is an engineering judgment against [Strategy](../../STRATEGY.md) and
ADR 0093, not a measured total-cost ranking or a proposed language-runtime implementation.

| Responsibility | Rust primary, optional scripting adapter | Script primary over a Rust engine |
| --- | --- | --- |
| Complete workflow | One required Rust build/debug/export path; optional adapter has its own declared scope | Script path must cover ordinary gameplay, tooling and delivery before it is the primary product |
| Iteration | Compile/restart cost remains; data reload and explicit restoration can reduce lost context | Many script edits avoid native compilation; state/module/reference compatibility still needs defined rules |
| API and maintenance | Native public APIs work directly; adapter binds only its actual supported workflows | Every required domain needs script-facing APIs, type/value conversion, ownership rules and versioned diagnostics |
| Editor and debugging | Rust IDE/compiler integration plus Nara data/schema tools; optional VM adds its own debugger/editor cost | Language services, stack/source mapping, breakpoints, errors and tool-code lifecycle become required product work |
| Runtime and deployment | No mandatory VM; adapter targets/dependencies are opt-in | VM/runtime, native bindings, resource budgets, distribution and platform qualification enter the required matrix |

An established VM can remove language-parser/compiler work; it does not remove Nara API binding,
Editor integration, debugger integration, deployment or state migration. Script-first could be
the right choice for a product primarily serving script authors, but it changes Nara's current
Rust-first target customer and complete-production promise. Designers can already be served by
visual/data authoring without requiring every contributor to write Rust. [Strategy](../../STRATEGY.md)

For the current Trial, retain complete Rust authoring and the existing `bevy_ecs` substrate.
Prioritize one useful rebuild-and-resume workflow with game-owned validated state, then measure
edit-to-useful-result separately for data, function-body and structural edits. Evaluate a narrow
optional script adapter only for a real gameplay/designer workflow, including its bindings,
debugger, state rules, fault behavior and export targets. This implements the evaluation order in
ADR 0093; it does not select a VM or introduce a language-neutral behavior host.

Revisit language primacy if complete-game evidence shows Rust's measured workflow cannot meet an
accepted production requirement after attributed improvements, or the primary users demonstrably
need a script-authoring workflow and its complete support cost is funded. Revisit shared adapter
contracts only when independent consumers prove them; revisit ECS only when a concrete workload
cannot meet its correctness/API/performance requirement through the accepted substrate. Faster
syntax feedback alone does not settle those broader product decisions.

## Source Register And Limits

Source links are pinned; local paths identify files reviewed under read-only `repo-ref/`.
No Godot binaries, reload migration tests, Nara code tests, or cross-engine benchmarks were run.
Unknowns include platform reliability, repeated-reload memory, complete callback/reference edge
cases and Nara adapter cost. HTML retrieval failed with gateway errors/403; official versioned
documentation source was retrieved successfully.

[^G1]: [Godot reload source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript.cpp#L745); [local reload](../../repo-ref/godot/modules/gdscript/gdscript.cpp), lines 745-920.
[^G2]: [Language reload source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript.cpp#L2521); same local file, `reload_scripts` and property reconstruction, lines 2521-2658.
[^G3]: [Member migration source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript.cpp#L2057); same local file, `reload_members`, lines 2057-2084.
[^G4]: [Function replacement source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript.cpp#L1432); same local file, replacement and pending-function cancellation, lines 1432-1535.
[^G5]: [Compiler source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript_compiler.cpp#L2707); [local compiler](../../repo-ref/godot/modules/gdscript/gdscript_compiler.cpp), lines 2707-2725, 3068-3073 and 3218-3317.
[^G6]: [Static state source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/gdscript/gdscript.cpp#L722); local `gdscript.cpp`, static save/restore, lines 722-741.
[^C1]: [C# reload source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/csharp_script.cpp#L625); [local lifecycle](../../repo-ref/godot/modules/mono/csharp_script.cpp), lines 625-1027.
[^C2]: [Generator source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptSerializationGenerator.cs#L111); [local generator](../../repo-ref/godot/modules/mono/editor/Godot.NET.Sdk/Godot.SourceGenerators/ScriptSerializationGenerator.cs), lines 111-260.
[^C3]: [Serialization bridge source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/glue/GodotSharp/GodotSharp/Core/Bridge/CSharpInstanceBridge.cs#L289); [local bridge](../../repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/Bridge/CSharpInstanceBridge.cs), lines 289-346; [listener](../../repo-ref/godot/modules/mono/glue/GodotSharp/GodotSharp/Core/Interfaces/ISerializationListener.cs).
[^C4]: [Assembly loader source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/modules/mono/mono_gd/gd_mono.cpp#L789); [local loader](../../repo-ref/godot/modules/mono/mono_gd/gd_mono.cpp), lines 789-835.
[^E1]: [Extension manager source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/extension/gdextension_manager.cpp#L161); [local manager](../../repo-ref/godot/core/extension/gdextension_manager.cpp), lines 161-218.
[^E2]: [Registration source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/extension/gdextension.cpp#L473); [local extension](../../repo-ref/godot/core/extension/gdextension.cpp), lines 473-561.
[^E3]: [Instance reconstruction source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/extension/gdextension.cpp#L916); same local file, lines 916-1080.
[^E4]: [Method compatibility source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/core/extension/gdextension.cpp#L161); same local file, lines 161-190 and 583-609.
[^S1]: [SceneTree source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/main/scene_tree.h#L92); [local SceneTree](../../repo-ref/godot/scene/main/scene_tree.h); [Node](../../repo-ref/godot/scene/main/node.h), line 54; [frame processing](../../repo-ref/godot/scene/main/scene_tree.cpp), lines 629-720.
[^S2]: [PhysicsBody2D source](https://github.com/godotengine/godot/blob/81ab961f91066a63bcbf3ec340962bb1a1ada558/scene/2d/physics/physics_body_2d.cpp#L48); [local node](../../repo-ref/godot/scene/2d/physics/physics_body_2d.cpp), body creation and server calls.
[^D1]: [Official 4.5 tool-code manual](https://github.com/godotengine/godot-docs/blob/33e08e14c517541fac3d9069e82fb3ee2fc99a10/tutorials/plugins/running_code_in_the_editor.rst#L26), lines 26-34 and 130-138.
[^D2]: [Official 4.5 C# guidance](https://github.com/godotengine/godot-docs/blob/33e08e14c517541fac3d9069e82fb3ee2fc99a10/tutorials/scripting/c_sharp/c_sharp_basics.rst#L33), lines 33-67 and 273-278.
[^D3]: [Official 4.5 reloadable guidance](https://github.com/godotengine/godot-docs/blob/33e08e14c517541fac3d9069e82fb3ee2fc99a10/tutorials/scripting/gdextension/gdextension_file.rst#L30), lines 30-32.
[^D4]: [Official 4.5 server guidance](https://github.com/godotengine/godot-docs/blob/33e08e14c517541fac3d9069e82fb3ee2fc99a10/tutorials/performance/using_servers.rst#L1), outdated marker and layering/RID sections.
[^D5]: [Official 4.5 node alternatives](https://github.com/godotengine/godot-docs/blob/33e08e14c517541fac3d9069e82fb3ee2fc99a10/tutorials/best_practices/node_alternatives.rst#L6).
