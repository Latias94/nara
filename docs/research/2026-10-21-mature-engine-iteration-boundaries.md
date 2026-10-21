# Mature Engine Iteration Boundaries And Nara

Date: 2026-10-21. Nara source basis: `6bb1a355c2e09d60979b063569b1de2237e25163`.
This is a documentation/source study, not a benchmark or an adopted architecture decision.
Unity sources are versioned 6000.2 documentation; the Unreal page identifies itself as 5.8;
Defold sources are the current official manuals retrieved on the date above.

## Recommendation

Keep the change-class separation already accepted by
[ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md).
The next complete Rust iteration consumer should measure rebuild followed by an explicit return
to a useful test situation. It should not start by introducing a universal native plugin ABI,
arbitrary World snapshots, or a second mandatory author language. This is a recommendation from
the comparison below, not evidence that a patching or scripting route cannot be useful.

## What The Engines Actually Provide

| Engine/path | Mechanism and supported boundary | State and failure implications | Nara implication |
| --- | --- | --- | --- |
| Unity script reload | Managed code reload integrated with the asset database and Unity serialization | Supported fields can be restored; static variables are not restored by script hot reload | State preservation needs an explicit supported data contract; reflection alone is insufficient |
| Unity Enter Play Mode settings | Domain and scene reload can be disabled independently | Skipped reset work changes the starting-state behavior and gives application code cleanup responsibilities | Compare fast restart with realistic fresh-start behavior; do not silently weaken reset semantics |
| Unreal Live Coding | Live++ recompiles and patches native binaries; reflected objects can be reinstanced | Structural replacement still needs pointer/cache repair and destructor care | Native patching requires a runtime object/lifetime contract, beyond loading a library successfully |
| Defold development hot reload | Lua scripts and engine resources are reloaded in the running environment | Script initialization is not automatically rerun; module-reference behavior and reload hooks matter | A VM makes some code replacement easier, but still requires explicit state/lifecycle semantics |
| Defold Live Update | Bundled resource archives can be delivered and mounted later | Content/engine compatibility and application-managed mount persistence remain explicit | Delivery updates and development hot reload are separate products |

### Unity

Unity script reload serializes eligible fields before replacement and restores supported values
afterward, including eligible private fields. Static variables are excluded from restoration.
This is a managed serialization contract rather than preservation of every resident object or
reference. Nara can borrow the explicit admission and restoration model without assuming it can
retain arbitrary Rust memory. [How Unity uses serialization](https://docs.unity3d.com/6000.2/Documentation/Manual/script-serialization-how-unity-uses.html).

Unity normally reloads both the scripting domain and scene when entering Play Mode. Disabling
either avoids work but changes how faithfully the session starts like a built application.
[Configuring how Unity enters Play mode](https://docs.unity3d.com/6000.2/Documentation/Manual/configurable-enter-play-mode.html).
With domain reload disabled, static values and static event subscribers persist across sessions;
the application must reset or unregister them. Disabling reload on entry also does not disable
domain reload after detected script changes. Thus a faster no-edit entry is a different operation
from applying a code edit. [Domain reload disabled](https://docs.unity3d.com/6000.2/Documentation/Manual/domain-reloading.html).

Unity also uses assembly boundaries and explicit references to reduce unnecessary recompilation.
The transferable lesson is to measure dependency invalidation, not to imitate a managed assembly
runtime in Rust. [Organizing scripts into assemblies](https://docs.unity3d.com/6000.2/Documentation/Manual/assembly-definition-files.html).

### Unreal Engine

Live Coding integrates Live++ to recompile and patch C++ while the engine runs, including PIE
and an attached packaged desktop application. The documented path excludes console/mobile
launches. Object Reinstancing replaces instances associated with Unreal reflection constructs
and generated objects. Epic warns that larger changes without reinstancing can crash, and that
code retaining affected pointers must repair them or invalidate caches through reload callbacks.
Old/new destructor conflicts are another documented cleanup hazard. Constructor defaults in a
`.cpp` do not simply update existing instances. Preloading more modules trades startup cost for
iteration performance. These are separate constraints even when compilation and patch loading
succeed. [Unreal Live Coding](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-live-coding-to-recompile-unreal-engine-applications-at-runtime).

For Nara, this supports ADR 0093's compatibility checks and quiescent application boundary.
It does not demonstrate that Unreal's reflected-object replacement can migrate arbitrary Rust
components, ECS system state, closure captures, task callbacks, statics, or backend resources.
That limitation is an inference from the different ownership models, not an Unreal benchmark.

### Defold

Defold development hot reload can replace Lua and resource content on desktop and connected
development targets. Reloaded Lua executes again in the running environment. `init()` is not
called automatically; scripts can implement `on_reload()`. Its documented module example shows
that a caller retaining an old locally returned table can continue observing the old table
after a module reload. Therefore even a VM-based reload must specify which references and state
continue. [Hot reloading resources](https://defold.com/manuals/hot-reload/).

Native extensions are a separate route: the documented build creates a project-specific engine
executable with the extensions included. This page does not establish that arbitrary native
extension edits share Lua's hot-reload path. [Native extensions](https://defold.com/manuals/extensions/).

Live Update delivers resource content excluded from the initial bundle, using archives and
mounts. Its default mount behavior checks engine-version compatibility; disabling that check
transfers compatibility responsibility to the developer. Mount choices are session-local and
need application-managed persistence to survive restart. The manual distinguishes testing this
bundled path from ordinary Editor Build. It is consequently unsuitable as evidence of native
code hot patching. [Live update](https://defold.com/manuals/live-update/).

## Nara's Existing Position

- [Strategy](../../STRATEGY.md) requires a complete Rust production path and explicitly separates
  data reload, compatible patching, rebuild, isolated restart, and validated restoration.
- [ADR 0093](../architecture/adr/0093-rust-authoring-hot-iteration-and-optional-scripting-adapters.md)
  already accepts this layered model. Optional adapters own VM/reload semantics; no universal
  behavior host or mandatory scripting runtime is selected.
- [ADR 0116](../architecture/adr/0116-standalone-supervised-editor-play.md),
  [supervisor](../../editor/src/supervised_play/supervisor.rs), and
  [generation store](../../editor/src/supervised_play/generation/store.rs) establish a separate
  product process, admitted source agreement, immutable executable generations, startup
  attestation, and truthful process cleanup. Those are useful foundations for rebuild/restart,
  not proof of live native replacement.
- The [Rust Play report](../benchmarks/2026-10-20-rust-play.md) measures Play through accepted
  first presentation. Its final five-sample cohorts observe medians of 3.295 seconds for no-edit
  Play, 13.961 for a body edit, 10.700 for a runtime-layout edit, and 1.499 for Restart.
  These uncontrolled cohorts do not establish stable tails or a general build-speed gain.
  They also do not measure time to restore a useful test situation.
- The two-level game's [checkpoint DTO](../../samples/two-level-game/src/save/dto.rs) is explicit,
  bounded, versioned game data. It intentionally reconstructs live position, velocity, input,
  and simulation tick. Its fresh-process restoration demonstrates a semantic save path, not
  transparent preservation of every development session.

## Proposed Order Of Investigation

1. **Complete one useful rebuild-and-resume workflow.** In a disposable two-level consumer,
   select a test location/progress state, change a Rust function or layout, rebuild, and return
   to the declared test situation. Let the game own a small versioned record and validation.
   Reuse the existing checkpoint when its semantics suffice; introduce development-only state
   only for a demonstrated need such as a particular live test position. Keep Scene/Prefab
   authoring data distinct from transient runtime state. Do not generalize this to World dumps.
2. **Measure compilation and activation separately.** Record edit completion, Cargo admission,
   build completion, immutable generation publication, process start, state admission, and
   accepted useful presentation. Compare matched dependency/profile/linker experiments only
   after the costly interval is identified. Do not attribute total binary unit time to the linker.
3. **Expose the actual operation and retained state.** Distinguish data reload, rebuilding,
   ordinary fresh start, same-generation restart, and validated resume. Surface incompatible
   resume state as an explicit outcome, with an available fresh-start action. Build failure
   must retain the known-good executable path; that does not imply an old process stayed live.
4. **Prototype optional native patching for a narrow edit class.** Use an isolated development
   experiment only after the baseline above. Test a coarse function boundary, compatible body
   edits, participating task/callback retirement, and explicit rejection of structural/unknown
   edits. Compare complete useful-result latency and memory growth. Retain the route only if
   it earns its toolchain/platform maintenance cost. This is not a current implementation task.
5. **Consider a scripting adapter when a real authoring workflow needs it.** Evaluate the
   binding/debugging/package/state-reload cost with that workflow. Do not infer from Defold's
   Lua experience that Nara must add Lua or that a VM alone solves iteration. ADR 0093 remains
   sufficient until a concrete consumer requires a new shared boundary.

Suggested acceptance for the first investigation: same-declared-situation results for no-edit,
body and structural edits; unsupported record/schema rejection; compiler failure and repair;
asset/source mismatch; repeated resume; complete Stop and fresh-start controls. Report separate
endpoints and sample sizes. No numerical latency promise is selected by this research.

## Evidence Limits

No Unity, Unreal, or Defold binaries were benchmarked. No new Nara code or tests were changed or
run. Official search failed twice with a gateway error; the source pages were retrieved directly
from their official domains. A guessed Defold development subpage returned 404 and is not used.
The comparison establishes mechanism boundaries, not a cross-engine latency ranking, platform
qualification, stable Rust ABI, or proof that a particular native-patching library is safe.
