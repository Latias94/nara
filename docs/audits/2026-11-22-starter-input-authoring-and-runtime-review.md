# Desktop 2D starter, authoring and runtime review

This review began at `f93a996` on Windows x64 and includes the shared working tree. It follows
the [tenth-round review](2026-11-21-scene-admission-input-order-and-material-dependencies.md).
At this baseline, seven open findings remain in this review, six P2 and one P3. No Critical or P1
finding was added. The engine direction continues to follow a mature product architecture, with
the remaining risk concentrated at ownership seams between source observations, prepared
candidates, history publication, native input delivery and project identity.

The review also confirms the intended input product boundary. `nara_input` should remain the
official normalized-input and semantic-action foundation. Nara should also ship an official
first-party action-map authoring and rebinding workflow, including backend capability checks,
diagnostics and starter conventions. That layer is the Unity Input System-like product surface.
Game-owned commands and gameplay meaning should remain above it. Runtime UI, Editor shortcuts and
gameplay actions should retain typed policies after Host-level routing, as required by [ADR
0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md).

## P2 · Clearing an effective Prefab field can falsely report stale source

In a document containing two Prefab instances, give the left instance an optional entity
reference that resolves through the other instance. Inspect the current value and submit a local
`Absent` override through a registered panel. Repeat in both a containing Scene and a containing
Prefab. The exact candidate compiled successfully in each case. The panel results depended on
which source files the candidate needed to read.

| Candidate edit | Prefabs read, before → after | Compiled field | Panel outcome |
| --- | ---: | --- | --- |
| Clear the last reference to the other instance | 2 → 1 | `Missing` | `Failed(StaleSource)` |
| Clear a local value so its default first refers to the other instance | 1 → 2 | `Defaulted` | `Failed(StaleSource)` |
| Keep a second reference that already requires the other instance | 2 → 2 | Requested override verified | Reached `Review` |

The source files kept their original bytes. Both refusals left a clean document and empty Undo
and Redo histories. The Host compares the whole candidate read set against the original read set
before checking the requested property. Track freshness separately from membership. Retain the
original observations, acquire the facts the candidate newly depends on, and revalidate both
before committing the exact reviewed edit. Preserve rejection when an observed file or working
copy actually changes.
[Host verification](../../src/project_host/runtime/editor/prefab_inspection.rs#L695) · [Source-set comparison](../../src/project_host/authoring/project/capture/source_observations.rs#L45)

## P2 · Confirming a gesture that returns to the stored value creates history

Change one field, undo it, and confirm that the document has `Undo=0` and `Redo=1`. In two
independent update cycles, prepare a new value and then prepare the original value again. The live
and preview projections are identical at the end, and the forward candidate patch equals the
inverse patch of the undone edit. Confirming this final candidate still reports `applied=true`,
leaves `Undo=1`, clears `Redo`, and marks the document dirty. A subsequent Redo fails. Save writes
bytes identical to the initial document. The result is the same for Scene and Prefab documents.

The history publisher treats every accepted patch as a content change and clears Redo before
recording it. Classify the explicit stored-state transition before publishing. A transition such
as `Defaulted -> Present` remains a real edit even when its value is equal to the contextual
default. A candidate that returns to the existing stored state should preserve the current history
and dirty state. [History publication](../../crates/nara_scene/src/authoring.rs#L427) · [Property
patch construction](../../crates/nara_scene/src/authoring/property.rs#L244)

## P2 · A prepared candidate's public slot can make generic Revert remove another claim

Use the prepared candidate read views on an inactive Prefab. In the live document, slot
`(component_index=1, claim_index=0)` contains `leaf.score = Set(77)`. The prepared candidate changes
that same slot to `leaf.note = Absent`. Passing the candidate's complete property handle to
`prepare_prefab_property_revert` correctly returns `WrongPreparedEdit`. Decomposing the same
candidate handle into its public `basis()`, `path()` and `slot()` and passing those values to the
generic `prepare_prefab_revert(..., PrefabRevert::Intent(slot), ...)` succeeds. The committed edit
removes the live `score` claim while leaving `note` in place. Undo restores `77`. Scene and Prefab
documents reproduce the same result.

`PrefabOverlaySlot` is a structural coordinate. A candidate may replace the claim occupying that
coordinate while retaining the coordinate itself. Public APIs that accept the coordinate without
the candidate-bound claim identity let a prepared view act on a different source state. Keep the
opaque candidate-bound handle through panel submission, or add the declaration identity and exact
candidate basis to every generic Revert admission. [Prepared claim slot](../../crates/nara_tooling/src/authoring_query.rs#L300) · [Panel generic Revert](../../editor/src/panels/context.rs#L176) · [Prefab inspection Revert](../../src/project_host/runtime/editor/prefab_inspection.rs#L558)

## P2 · A legal large Map key can make the property catalog unpageable

Create a Map with two legal string keys of 32,768 bytes each followed by the short key `z`. Under
the maximum 64 KiB metadata budget, every `Properties` query from `max_rows=1` through `256`
returns `MetadataBudgetExceeded` in both the live and prepared views. The first long key is charged
as a returned row and the continuation is charged again. The two long keys also consume the budget
before a useful page can be returned. Value pages still read all three keys, a direct read of the
first long key succeeds with 32,805 bytes of target metadata, and editing that key plus Undo
succeeds. Reducing a key to 32,700 bytes makes a one-row catalog page possible.

The catalog budget should charge the target and each returned row once while retaining bounded
continuation state. A single row that fits the documented value limit should be discoverable, or
the API should expose a bounded direct continuation that does not require duplicating the full
target and cursor metadata. Preserve the existing value pagination and point-edit guarantees.
[Property page accounting](../../crates/nara_tooling/src/workspace/authoring_query.rs#L462) · [Map
cursor accounting](../../crates/nara_scene/src/authoring_decode/read.rs#L255)

### Prepared-candidate and history measurements

An independent Windows probe linked the release libraries for `f93a996` and exercised the prepared
entity, property-directory and value-page consumers. The existing format and field-candidate
controls passed **20/20**. The independent panel controls passed **9/9**, and the new candidate-view
behavior probe passed **5/5**. Entity, property-directory and value-page cursors rejected sibling
candidates with the same basis, and an original candidate remained replayable after Close. No
format admission or Undo payload mismatch was reproduced in that control set.

On a 32,768-row keyed list, changing a short `note` beside the large component measured the
following P50 ranges in milliseconds on two fresh processes per document type. Replacement counts
only old-candidate destruction. Undo and Redo include public `submit_and_drive`.

| Inactive document | Replace old candidate | Cancel final candidate | Undo | Redo |
|---|---:|---:|---:|---:|
| Scene | 4.40–7.00 | 8.42–12.23 | 76.95–86.75 | 74.80–80.34 |
| Prefab | 4.24–4.49 | 7.50–9.66 | 71.65–75.02 | 72.12–77.59 |

Cancelling one candidate released about **32.72 MiB** on the calling thread, with 168,956 frees
for Scene and 168,957 for Prefab. A superseded notification does not release a caller-held old
batch until the caller drops it or submits it late. A late submission returns idle and preserves
history. The 257-row cancellation control measured **0.036–0.048 ms** P50. These are Windows
allocation counts and call timings under concurrent build load. They do not measure process RSS or
native frame latency. [Candidate replacement](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages.rs#L1394) · [Submission custody](../../editor/src/panels/submission.rs#L46)

## P2 · Resuming a Server with a long timestep blocks the next control request

Use the delivery test's 20 s fixed timestep and 60 s frame clamp. Pause at tick 0, submit a
marker for tick 1, then send `resume` and `pause` back to back. In two fresh processes the parent
wrote `pause` 0.014 ms and 0.025 ms after `resume`. The `paused` replies took 20.003 s and
20.004 s respectively. Both reported tick 1 and `markers=[1]`. The initial pause in the same
two running processes took 0.47–2.21 ms. With a 20 ms fixed timestep, the same pair of requests
completed in 21.6 ms. An independent run also closed stdin with an empty command queue. EOF while
paused exited in 0.017 s at tick 0. Sending `resume` immediately before EOF took 20.013 s and
exited at tick 1, with zero accepted commands in both runs.

The `resume` handler resets the frame clock, synchronously sleeps until the next frame is due,
and drives it before returning to the input loop. A complete `pause` request may already be
waiting while the game executes the marker. Apply Resume at the current control boundary,
acknowledge it, and let the live loop wait for input or the next simulation deadline. Verify
that immediate re-pause can leave the tick-1 command pending. The public Headless session already
supports resuming with zero real elapsed time.
[Server Resume](../../samples/two-level-game/src/bin/server/service.rs#L435) · [Blocking wait](../../samples/two-level-game/src/bin/server/service.rs#L451) · [Headless Resume](../../src/project_host/runtime/action/session.rs#L256)

## P2 · Desktop Check accepts keyboard bindings the stock runner cannot deliver

Create a `desktop-2d/v2` game and change only the keyboard binding of `move.right`. The results
below use the generated movement system. Each run holds one key for two fixed ticks and then
releases it. The default and `desktop` Check profiles both admitted every map. Cook revalidated
the original sources, wrote the pack and passed deep validation on both reopened members.

| Authored key | Key sent | Winit conversion | Final player X, source / cooked |
| --- | --- | --- | --- |
| `d` | D | `Character('d')` | 9.6 / 9.6 |
| `j` | J | `Character('j')` | 9.6 / 9.6 |
| `j` | D | `Character('d')` | 0 / 0 |
| `;` | Semicolon | `None` | 0 / 0 |
| `]` | Right bracket | `None` | 0 / 0 |

Injected directly as `KeyCode::Character`, both punctuation keys moved the player to X = 9.6
on both routes. In a separate hidden Windows window, posted Win32 down/up messages reached an
actual Winit event loop as `Code(Semicolon)` and `Code(BracketRight)`. Nara's stock converter
returned `None` for both edges of both keys. D and J survived the same route. The stock keyboard
handler returns early on this conversion failure. The game, map decoding and Cook all preserved
the author's binding.

Support these physical codes in the desktop adapter. For keys outside its supported vocabulary,
let a desktop author discover the limitation at binding or Check time. Generic Input can still
accept additional controls delivered by another backend. Use native physical-key ingress in the
regression alongside the normalized driver. Bevy maps these two physical codes explicitly and
Godot's Windows scancode table also distinguishes both keys.
[Native ingress](../../crates/nara_winit/src/lib.rs#L1790) · [Keyboard conversion](../../crates/nara_winit/src/lib.rs#L2487) · [Game movement](../../starters/desktop-2d/v2/src/gameplay.rs#L89) · [Bevy physical keys](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_winit/src/converters.rs#L110-L155) · [Godot scancodes](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/platform/windows/key_mapping_windows.cpp#L262-L274)

## P3 · Project creation can publish a local game that Cargo cannot resolve

With this checkout selected by `--local-checkout`, `nara new desktop-2d/v2 nara` returned success.
The first `nara check` exited 1 before compiling the game. The newly generated package and the
engine dependency are distinct local packages named `nara` at version `0.1.0`. Cargo reported
`package collision in the lockfile` and could not initialize the lock. An independent
`cargo +1.95.0 generate-lockfile --offline` returned 101 for this name with both v1 and v2.
With `pixel-game`, the same v2 command and checkout generated the lock successfully.

The creation validator checks the spelling of the directory slug and uses it as the Cargo package
name. Catch this known local package identity collision before publishing the project, or give
the generated package a distinct Cargo identity while preserving the requested project name.
The counterexample involves two path sources at the same version.
[Slug validation](../../src/project_workflow/starter.rs#L574) · [Generated package identity](../../src/project_workflow/starter/product_contract.rs#L62)

## Prefab authoring limits observed in the same panel

- **Revert feedback.** One instance hid a `socket` child and pinned `aim` to `Absent`. After the
  base Prefab was edited to point `aim` at `socket`, the effective-property panel reported
  `Reverted` when the pin was removed. The next inspection failed with `missing-source-entity`.
  Undo restored the valid instance. Redo and Save/Reopen retained the invalid composition. Source
  declaration repair intentionally supports Revert on unresolved content. The effective-property
  panel could surface the resulting composition diagnostic during review while preserving that
  repair operation. [Revert review](../../src/project_host/runtime/editor/prefab_inspection.rs#L558) · [Declaration repair contract](../../docs/architecture/adr/0026-editor-command-patch-and-undo-model.md#L133)
- **First value.** For a missing optional scalar, `Set(42)` from the panel returned
  `InvalidProperty`, including after an `Absent` claim masked a newly authored base value.
  Writing the same override in the source produced `Present(42)`. The current Set interface
  explicitly covers leaves available through their existing selector. [Set admission](../../crates/nara_scene/src/prefab/inspection/property_write.rs#L142) · [Current scope](../../docs/architecture/authoring-read-projection-design.md#L368)

## Earlier findings retested

| Workflow | Observed result |
| --- | --- |
| 8× live Server, open and silent stdin | A marker accepted for tick 600 executed exactly once. After three seconds the Server reported tick 1201, an empty queue and zero fixed debt. Two additional running-start processes accepted tick 601 and also exited 0. |
| Zero-scale startup with `--paused --save` | Ready arrived in 91 ms and 65 ms, with the slot configured, the launch acknowledged and the session paused at tick 1. A 20 ms advance preserved tick 1. The no-slot control started at tick 0. |
| Finite region and travel verifiers | All four combinations of `--verify-travel` / `--verify-regions` and initially paused 1× / initially running 8× exited 0. The two ordinary 1× controls also passed. |
| Half-speed fixed-command boundary | The four partitions of 40 ms now execute and acknowledge the marker exactly once at logical tick 1. Both 400 s runs complete 10,000 fixed ticks with zero remaining phase. The parent implementation delivered 9,999 ticks for the three-frame partition. A fresh Server restored the half-nanosecond carry and a pending marker, then delivered it on the next 1 ns advance. |
| Checkpoint handoff and restore count | A candidate could claim the slot only after incumbent retirement. Replacing the file with identical bytes under a new file identity during Restore kept `restores=0`. A successful retry of Restore raised it to one, which survived capture and fresh-process startup. |

The Server probes used Rust 1.95.0, rebuilt local engine crates and game code, cached third-party
libraries, and five binaries linked to the matching prepared packs. Each ran from an empty working
directory. The starter probes used `nara new`, the generated game's sources, release engine
libraries and small observers for input and player position. Cargo lock resolution ran offline.
The existing starter creation binary passed all 12 tests serially. The existing time continuation
integration suite passed 20/20, and an independent integer oracle matched the scale conversion
in 30,394 finite cases. Nine registered-panel regressions and eight Scene controls passed. The
Prefab behavior above was exercised through a registered panel, file saves and fresh inspections.
[Fractional scheduling](../../crates/nara_app/src/scaled_time.rs#L37) · [Continuation tests](../../crates/nara_app/tests/runtime_time_continuation.rs#L73)

## Architecture verdict and input product boundary

Nara is following a credible mature-engine direction. Its current ownership graph has the shape
that production engines need. Platform adapters produce normalized events, the Input owner retains
bounded physical state, routing arbitrates UI and gameplay, action maps produce semantic values,
and the game maps those values to commands. This is aligned with Bevy's low-level
`ButtonInput` and plugin-owned systems together with Godot's project-owned `InputMap` and action
queries. [Nara input contract](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md) · [Bevy InputPlugin](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_input/src/lib.rs#L102-L116) · [Godot InputMap](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input_map.h#L40-L90)

The current findings cluster at seams that mature engines harden over time. They concern prepared
candidate identity, source freshness, explicit stored-state transitions, native key capability,
simulation wake-up and generated package identity. The ECS, normalized Input, immutable action
definitions, Host ownership and headless command boundaries remain coherent in the workflows
covered here. The next architectural work should strengthen those seams instead of introducing a
second runtime model.

The official Input surface should have two product layers.

| Layer | Nara should own | Game should own |
|---|---|---|
| Normalized Input foundation | Platform-independent events, `ButtonInput`, pointer and gamepad state, bounded lifetimes, focus and retirement, physical provenance | The selected native adapter and device-specific policy |
| Semantic action system | Action definitions, contexts, device scopes, analog values, modifiers, rebinding representation, action outcomes and replay boundaries | The meaning of `move.right`, command payloads and gameplay behavior |
| First-party product workflow | Project action-map files, Editor rebinding UI, starter defaults, diagnostics, Check/Cook capability validation and migrations | Project-specific bindings, control schemes and user-facing presets |

The second and third rows are the Unity Input System-like product experience. They deserve first-
party implementation because every complete game needs a stable action identity, rebinding, input
diagnostics and a validated path from authored binding to native delivery. They can live in a
first-party plugin and Editor workflow while the lower contract remains independently consumable.
The current `nara_input` foundation already supplies the correct ownership direction, and the
starter action-map workflow is the right consumer proof.

Keep semantic command mapping above the action system. An engine can provide `ActionMap`,
`SemanticActions`, common contexts and a starter `move.right` declaration. Movement intent,
authority, replay payloads, network policy and game rules belong to the game. This preserves the
same action identity when a player changes from `d` to `j` while keeping the engine independent of
one game's movement model.

Keep gameplay actions, runtime UI actions, Editor shortcuts, text input and accessibility as
separate typed policies after Host-level routing. A universal action abstraction would pull text
composition, focus, editor navigation and gameplay semantics into one lifetime contract. ADR 0041
already selects the healthier boundary. The next implementation should complete each policy's
consumer workflow and share only normalized ingress and routing custody.

The immediate architecture gates are these.

1. Preserve an opaque candidate-bound identity through Prefab panel operations. A structural slot
   should never be sufficient to identify a mutable declaration after a candidate changes it.
2. Separate source freshness from the candidate's dependency-set membership. A newly required file
   needs acquisition and validation, while an unchanged observed file needs freshness validation.
3. Publish history from explicit stored-state transitions. Patch construction and patch acceptance
   do not by themselves prove that the document changed.
4. Make the first-party action workflow own a platform capability matrix. `Check` should reject or
   diagnose a binding that the selected desktop adapter cannot deliver, including physical keys
   that the native Winit path can observe.
5. Keep Server scheduling and project creation on the same product contract as the library APIs.
   A control request should not synchronously wait for the next simulation deadline, and a generated
   local package should be checked against the engine's Cargo identity before creation succeeds.

This leaves Nara with a mature foundation plus a clear product layer. The product layer should be
official, opinionated and replaceable through documented extension points. The core should remain
small enough for embedded and headless consumers, while the Editor and starter provide the default
experience that makes the engine feel complete.
