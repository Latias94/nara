# Nara runtime concepts: depth, locality, and validation placement

Read-only assessment, 2026-10-15. Requested baseline: `b91223a`; inspection observed `757fb6a`, whose only committed delta is the correctness-audit document. Runtime/composition sources cited below are unchanged from `b91223a`; Editor/SPL citations include concurrent work. No code, Cargo, tests, or formatting were run. Reference checkouts are **Bevy `4e6bd141e` (0.20 development)** and **Godot `d824ca9fe2` (4.8 development)**, not stable releases; Nara uses `bevy_ecs = "0.19"` (`Cargo.toml:94`). Recommendations are design judgments, not new verification receipts.

## 1. Select one execution ownership model; do not merely wrap today's choreography

Nara has one World/schedule authority, not a second ECS. Nevertheless, ordinary sprite startup exposes reservation, sealing, admission, startup, promotion, and a manual retirement helper (`examples/windowed_sprites.rs:28`; `examples/support/runtime_retirement.rs:49`). `App` supports raw run/shutdown and owns runtime fields (`crates/nara_app/src/lib.rs:1219`, `:2051`); managed execution transfers obligations into `RuntimeOwner` (`runtime.rs:1520`, `:1567`, `:2724`). Reporter clones share authority: these fields are not evidence of double ownership. The actual duplication is two execution/cleanup contracts and the checks required to keep them separate.

Develop these alternatives against the same consumer:

| Alternative | Ordinary Interface, illustrative | Required internal change | Tradeoff |
|---|---|---|---|
| A: retain managed execution | One game definition, optional project root, `desktop().run()`; an embedding consumer gets `start / advance / request_stop / close_status` | Make `App` configuration-only; consume its World, schedules and plugins into the existing managed owner. Retire raw `App::run/run_once` as a parallel production lifecycle. Keep candidate publication and finite close internal. Reuse/refactor `DesktopRun::from_recipe`, not a new pass-through launcher. | Preserves useful Editor/task ownership with fewer author concepts; breaks low-level consumers deliberately. |
| B: merge runtime into App | `App::new().add_plugins(...).run()`; embedded App exposes update/control/close | Move generation, sticky fault, cleanup and staged-start state into App; remove RuntimeCandidate/Instance and their forwarding scopes. Runner consumes the active App; Editor stages a complete App before replacing its slot. | Fewer nominal types, but App now owns all lifecycle complexity. Requires redoing admission/publication and failed-close semantics, not deleting them. |

**Prefer A**, with replacement rather than layering. `DesktopRun` already hides much of the work (`src/project_host/runtime/action.rs:334`); it should become the common product execution Module. B remains viable if a prototype materially reduces caller knowledge while preserving the same failure cases. Merely replacing `RuntimeInstance` with an App field is not simplification. ADR 0084 already rejected complete-App ownership (`docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md:390`); reopening it requires a task-equivalent comparison.

Bevy gives a useful Interface baseline: `App::run` consumes the configured application into its runner; finish/cleanup are runner-owned (`repo-ref/bevy/crates/bevy_app/src/app.rs:185`, `:1514`). Its PluginGroup applies ordered plugins (`plugin_group.rs:567`). That does not supply Nara's retryable service shutdown: Bevy's plugin `cleanup` occurs **before** schedules, not after game termination (`plugin.rs:71`). Preserve the stronger guarantee where real task/native owners need it.

## 2. Make one game definition own composition and persistence registration

Current extension authors encounter PluginDeclaration, replayable PluginDefinition, ProductRecipe, SchemaContribution, provider/owner/binding identities, callbacks, and catalog versions. The reference plugin preflights then registers its provider (`reference-game/src/lib.rs:134`, `:156`); Host composition separately selects providers and freezes a registry (`src/project_host/composition.rs:773`, `:893`). Frozen-provider validation does not replay registration callbacks (`crates/nara_reflect/src/provider.rs:571`), so do not describe every comparison as duplicate work.

Replace parallel author responsibilities with one definition builder: install systems, register persistent components/migrations, and attach shutdown ownership at the same declaration site. Compile this once into an immutable plan reused by direct launch, headless validation, schema export, and restart. Keep provider IDs as internal provenance when useful; persistent type/field/version identities remain public content contracts. Derive the selected schema-provider set from those registrations instead of asking authors to synchronize separate lists.

Canonical configuration hashes identify **declared configuration**, not arbitrary trusted Rust behavior: factories can read external state while providing identical bytes (`src/product.rs:46`; `crates/nara_app/src/plugin/definition.rs:170`, `:237`). Do not grow factory/source equivalence proofs. One compiled definition and explicit content versions are honest authority. Remove unused presence-only PluginCapability/PluginServiceId promises (`crates/nara_app/src/plugin.rs:60`) rather than implementing dispatch to justify them; retain actual dependency/conflict and product-feature checks.

This also resolves a product obstacle: CLI check/export currently use an empty default recipe (`src/bin/nara/check.rs:42`; `export.rs:87`). Let the project's compiled product perform strict checks with its definition. Preserve data-only Editor recovery. Do not load native gameplay into the Editor to solve discovery.

Bevy illustrates depth without removing asynchronous state: `AssetServer::load` returns a handle, with load/dependency states queried separately (`repo-ref/bevy/crates/bevy_asset/src/server/mod.rs:364`, `:1053`). Scene callers choose immediate fallible spawn or queued dependency resolution (`repo-ref/bevy/crates/bevy_scene/src/spawn.rs:56`, `:99`); the startup-system helper delegates through that same Interface (`repo-ref/bevy/crates/bevy_scene/src/spawn_system.rs:12`). Nara should similarly hide orchestration, while retaining its own versioned persistent schema guarantees.

## 3. Validate at the owner that can change the fact

Registry checks currently combine snapshot identity, instance token, structural mutation revision, authority identity and runtime generation (`crates/nara_reflect/src/plugin.rs:142`). App repeats fault/authority checks around schedule and scope execution (`crates/nara_app/src/lib.rs:2128`, `:2201`). They detect trusted-Rust mistakes, including temporary replacement; they are not a native-code security boundary.

| Fact | Keep in release | Move/delete after ownership migration |
|---|---|---|
| External document/archive/script bytes | Bounded decode, finite/range checks, schema/reference checks, path containment | Do not reparse unchanged immutable candidates in each layer. |
| Async completion, render picking, project switch | Exact request/document/runtime generation at commit | Check once where the owner publishes; downstream accepts the owned validated value. |
| Plugin dependency/schema conflicts | Once when building the immutable definition | Remove repeated self-consistency checks between projections derived from that same definition. |
| Frozen native registry/reporter identity | Content-binding and generation-publication agreement | First remove replaceable authoritative registry ownership from generic World access; then downgrade remaining same-generation mutation fences to debug assertions. Never delete fences while the mutable escape remains. |
| Task/native shutdown | Registered owner, cancellation versus physical completion, retained failed close | Plugins attach cleanup once; ordinary callers never supply an empty obligation ledger. |
| Native Rust plugin behavior | Normal Rust trust and error handling | Delete claims of hostile-code containment or behavioral equivalence from type IDs/hashes. |

A concrete counterexample prevents careless downgrade: a plugin replaces the selected reporter during a schedule and restores it afterward; end-state pointer equality misses it. Existing epoch checks intentionally cover this (`tests/runtime_instance.rs:2796`). Change that contract explicitly with the immutable-owner design. Keep process-local identity separate from persistent schema/mod identity.

## 4. Make editing transactions independent of presentation frames

The current rule is **one batch per published frame and at most one revision-advancing mutation per document**, not one action total (`src/project_host/runtime/editor/actions.rs:29`, `:73`). It rejects stale input correctly but also couples command throughput to redraw and complicates keyboard macros, agents and a replacement UI.

Move authoring concurrency to document revisions and explicit transactions. Reuse existing ScenePatch multi-operation support (`crates/nara_tooling/src/frame.rs:388`): validate all edits against one base, commit once, create one undo entry. Two independent transactions with an old base still conflict. Selection/diagnostic acknowledgments should not consume the only mutation admission opportunity. Keep exact presented-frame identity for picking/drag coordinates, where displayed geometry matters. Do not add OT/CRDT or implicit rebasing.

Counterexample: one command changes a sprite's position and layer; both must commit atomically without waiting for another redraw. Another actor's stale destructive command must still fail. This makes tooling genuinely reusable by dear-imgui-rs/custom UI without building a GUI portability framework.

## 5. Treat supervised Play as one private process Module, with an explicit trust contract

Immutable generations prevent Cargo output replacement races; one-shot first-present READY distinguishes successful spawn from playable startup. Godot's examined run path explicitly passes project path, creates child processes and later kills tracked PIDs (`repo-ref/godot/editor/run/editor_run.cpp:58`, `:190`, `:231`). It does not establish Nara's exact-byte execution guarantee. Godot validates addon configuration/script eligibility at loading (`editor/editor_node.cpp:4600`, `:4645`); native extensions have load/reload states (`core/extension/gdextension_manager.cpp:132`). These demonstrate different trust contracts, not missing Rust borrow checks to copy.

Nara hashes, copies, syncs, verifies and redigests binaries (`editor/src/supervised_play/generation/store/publication.rs:270`); the child measures its executable (`src/project_host/product_launch/child.rs:461`). This is expensive correctness machinery for trusted local development, **not a sandbox**: an approved Cargo project already executes arbitrary build/native code.

Retain ADR 0116's current guarantees while refactoring one private owner to accept requests and publish only changed observations plus deadlines. Publication/hash/launch validation belongs in cancellable work phases with explicit ownership. Close retry must be an admitted transition. The existing audit's F06–F08 are acceptance requirements for this Interface, not reasons to add managers.

A lighter trusted-local launch contract could retain explicit project/profile, immutable output, nonce/session and first-present identity while declining exact-handle/byte attestation. It would reduce portability cost, but **requires an explicit ADR 0116 refinement/version change** and adversarial race examples before selection. Do not silently downgrade v1. Neither path should grow into remote provenance, reproducible-build closure or general filesystem-job infrastructure.

## 6. Migrate with consumer evidence, then remove obsolete proof machinery

Refine ADRs **0003/0084** for lifecycle, **0046/0081** for unified composition/schema ownership, **0110/0114** for project-owned strict checks, and the exact-frame authoring contract before changing admission. ADR **0116** remains binding unless explicitly refined. ADR **0099** already rejected a generic evidence/approval framework; do not recreate it around this refactor.

Validate through existing seams: `tests/runtime_instance.rs`, `runtime_driver_boundary.rs`, `project_runtime_boot.rs`, `project_workflow_check.rs`, `crates/nara_reflect/tests/registry_contract.rs`, `editor/tests/supervised_play_clean_room.rs`, `reference-game/tests/editor_supervised_play.rs`, and `editor/src/supervised_play/generation_tests.rs`. Add a tiny independent consumer that launches custom persisted gameplay, strictly checks/exports it, fails startup after a worker starts, retries failed close, and restarts without old callbacks affecting the new generation. Quiet child means no repeated observation/redraw; slow publication remains cancellable.

Replace parser-like architecture inference in `tests/schedule_extension_contract.rs:426` and `tests/project_host_boundary.rs:331` with compile/privacy consumers and behavioral invariants. A harmless local rename must pass; an exposed forbidden authority must fail. Compare A/B by concepts learned, caller recovery code and locations changed for one plugin addition—not type or line counts. Keep Modules deep and changes local before adding mod scripting or another platform.
