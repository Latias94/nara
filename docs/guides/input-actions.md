# Input Actions

`nara_input` maps normalized keyboard and mouse transitions to named actions. Gameplay reads
`SemanticActions` for aggregate held state and start/release edges. `ActionOutcomes` exposes each
physical binding edge, including its route and event provenance. Gameplay command mapping remains
game-owned through `nara_gameplay::ActionCommandMap`.

## Code-First Definitions

Build a complete finite definition, declare its contexts and install independent World state:

```rust
use nara_core::ByteLimit;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    InputPlugin, KeyCode,
};

let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
builder.declare_context(ActionContext::gameplay(), true)?;
builder.bind(ActionBinding::key(ActionId::new("jump")?, KeyCode::Space))?;
let bindings = builder.compile()?.into_bindings();
let map = ActionMap::new(
    bindings,
    ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
)?;

let mut app = nara_app::App::new();
app.add_plugin(InputPlugin)?;
app.insert_resource(map)?;
```

The builder starts with no contexts. `ActionBinding::key` and `ActionBinding::mouse` select
`gameplay` by default; declare it explicitly or use `with_context`. Unknown contexts, duplicate
context declarations, duplicate complete bindings and exceeded limits fail during construction.
Different actions may share a physical input within the fanout limit. Binding order is preserved.

Definitions are immutable. A clone of `ActionMapBindings` shares its actual definition owner;
`ActionMap` itself is move-only. Create a separately admitted `ActionMap` for another independently
mutable World. `required_state_bytes()` is the conservative logical allowance for contexts and
retained lifetime observations; it is not a measurement of the entire input pipeline's memory.
`ButtonInput` and `SemanticActions` also own live state. Capture their public transitions, pressed
observations or semantic outcomes for recording instead of copying the runtime resources.

## Context Changes And Rebinding

Enable and disable only declared contexts. Both operations report whether the state changed and
borrow the context identity, rejecting an unknown name. Disabling a held context emits one semantic release with
`SemanticActionCause::ContextChanged` and no physical provenance. Re-enabling that context does
not synthesize a press: release the physical input and press it again.

Compile a complete replacement before calling `map.replace_bindings(bindings)`. A compilation
error leaves the current map unchanged. Installing compiled bindings begins a new mapping lifetime
even when restoring a previously used handle. Queued presses and held contributions from the old
lifetime cannot revive the action. Rebinding uses the state's existing fixed allowance, including
the period when older queued lifetime observations remain alive.

## Source Project Selection

Select one global file in `nara.toml`:

```toml
[input]
action_map = "input/player.actions.json"
```

This path is relative to the project root and is read with exact spelling. It does not inherit
`paths.assets`. Profiles can explicitly clear the selection using their existing Input overlay.
The selected Rust recipe must include `InputPlugin`.

The file is a strict canonical version-one JSON envelope with kind `action_map`. It contains a
complete intrinsic primary Product, explicit contexts and their initial enable states, and ordered
key/mouse bindings. The [nonempty golden](../../crates/nara_input/tests/fixtures/formats/v1/action-map.json)
shows the complete shape. Construct and validate these bytes through `ActionMapFileCandidate`;
author its non-nil source UUID once and preserve that identity across content edits. Loading never
mints identity. A sidecar is optional; when present its identity and canonical location must agree
with the file and other admitted project sources.

Source capture retains the actual compiled definition and file receipt. The Host installs the
selection before Scene materialization and the first Startup stage. A plugin that explicitly
installs a physical map, including an empty one, competes with the file selection and is rejected.
Keep game-specific action-to-command bindings in the recipe when using file-defined physical
bindings. With no file selected, code-first physical configuration remains valid.

Scene travel preserves the original global owner, context state and delivered held lifetimes.
Preparation verifies the original Input source and optional sidecar before reusing that owner.
Travel refuses changed or missing source. Restart the Runtime to use changed global bindings;
restoring the original admitted source permits a retry. Once a target is Ready, commit
uses its captured facts. Check and source Export include the selected file in the same content
receipt as the actual Scene closure.

## Processed Runtime Selection

Source-independent Runtime configuration selects a complete primary Product through
`RuntimeConfiguration::input.action_map`. This is a global startup root, independent of Scene
roots and travel permissions. The executable recipe must select both `InputPlugin` and an actual
`ActionMapDomain` decoder through `add_runtime_asset_codec`. The first-party codec is
`ActionMapRuntimeCodec`; a recipe may explicitly select another implementation of the same
typed family and format. The base preserves the actual selected implementation and its frozen
limits. Product data cannot choose a different decoder.

Cook an admitted source definition by consuming it into the processed CPU asset:

```rust
use nara_asset::RuntimeAssetCodec;
use nara_input::{ActionMapAsset, ActionMapFileCandidate, ActionMapRuntimeCodec};

let source = ActionMapFileCandidate::decode_json_bytes(source_bytes)?;
let (product, definition) = source.into_parts();
let asset = ActionMapAsset::new(product, definition)?;
let processed_bytes = ActionMapRuntimeCodec::encode_runtime(&asset)?;
```

The processed envelope uses kind `runtime_action_map`, version one. Source and processed readers
reject each other's kinds; both use the same finite declaration compiler. The processed asset
has no Required dependencies, filesystem capability or mutable World state. See the
[processed golden](../../crates/nara_input/tests/fixtures/formats/v1/runtime-action-map.json).
Construct the verified catalog and independent expectation as described in the
[Runtime base guide](runtime-content-base.md#global-startup-input).

`HeadlessRun::from_runtime_content` and `DesktopRun::from_runtime_content` install the global
selection before Scene materialization and Startup, with the same explicit-code conflict rule
as the source path. Scene travel preserves its actual acquisition, context state and held mapping
lifetimes. A Scene may independently read the same Product as Required typed CPU data, with its
own acquisition charge; that read cannot install or reconfigure global Input. With no selected
Product, code-first Input remains valid.

## Ownership And Scope

`ProjectContentSnapshot::input_action_map()` borrows the source owner. Its `bindings()` handle
retains actual compiled data and the original reservation after the snapshot is dropped. The
last escaped handle releases that custody. World-local context/revision state has a separate
reservation; queued lifetime observations can retain it after a resource is replaced.

`RuntimeContentBase::acquire_startup_input()` returns the optional `RuntimeInputContent` owner.
Its `bindings()` handle retains the actual typed acquisition and original base reservation after
the content handle or base variable is dropped. A second acquisition of equal Product data has
its own charge and owner identity. First-party startup separately reserves World-local state
from that same base; replacing Scene collections does not release or recreate that state.

Independent Hosts install a prepared global selection through `finalize_input_selection` and the
existing `RuntimeCandidateScope`. This receiving operation finalizes once, distinguishes pristine
Input defaults from explicit configuration, and is unavailable through a post-Startup runtime
scope. Ordinary source Hosts perform this step as part of startup.

Current action bindings cover keys and mouse buttons. Analog bindings, chords and a visual
action-map editor remain separate workflows. The current state allowance does not bound all
physical endpoint or semantic-contribution growth; that mutation-boundary correction remains
open under foundation R10. See [ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
for the owning contracts, the [source plan](../plans/2026-10-30-input-definition-and-source-admission-plan.md)
and [processed plan](../plans/2026-10-30-processed-input-startup-and-process-proof-plan.md) for their
actual qualification scopes. Target-build selection and complete packaged delivery remain separate.
