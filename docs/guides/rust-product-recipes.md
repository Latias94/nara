# Rust Product Recipes

`ProductRecipe` is the ordinary Rust composition surface for a game product. It is pure data: it
does not open project files, create an `App`, acquire a window or GPU, start a runtime, or select a
runner. Build it in ordinary game code, inspect it in tests or tools, then pass the same value to a
direct `App` or a product Host.

## Runtime-only plugins

Use `add_plugin` for a stateless plugin that implements `Default`:

```rust
use nara::prelude::*;

fn recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().add_plugin::<DebugOverlayPlugin>()
}
```

For a replayable plugin with configuration, put every behavior-bearing field in a small typed value
and encode it deterministically. The factory receives that value each time a Host constructs a
runtime.

```rust
use nara::prelude::*;

struct EnemySettings {
    max_active: u32,
}

impl ProductConfiguration for EnemySettings {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.max_active.to_le_bytes());
    }
}

fn recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().add_configured_plugin(
        EnemySettings { max_active: 48 },
        |settings: &EnemySettings| EnemyPlugin::new(settings.max_active),
    )
}
```

Use `configure_plugin` to replace a prior entry of the same Rust plugin type. It rejects a missing
entry, a conflicting plugin identity, or a change from a runtime-only entry to a schema
contribution. Recipe insertion order is not system ordering; plugin declarations own dependencies
and ordering constraints.

## Persistent schema contributions

For the separate Tilemap schema/data and TileSet file-publication plugins, see
[File-Backed TileSets](file-backed-tilesets.md).

A crate that owns persistent components exposes one typed helper. Its callers pass one value, not a
plugin definition plus a separate schema-provider list:

```rust
use nara::prelude::*;

pub fn package(settings: AgentSettings) -> Result<SchemaContribution<AgentPlugin>, ProductRecipeError> {
    SchemaContribution::configured(
        settings,
        |settings: &AgentSettings| AgentPlugin::new(settings.max_agents),
        [AGENT_SCHEMA_PROVIDER],
    )
}
```

`AgentPlugin::declaration()` must name exactly the provider IDs passed to
`SchemaContribution::configured`. The contribution owns those provider definitions in both paths:
direct `App` composition installs them before the schema-owning plugin builds, while the product
Host uses the same definitions to construct and freeze its schema registry before runtime
publication. Keep registration in the contribution's provider callback, not a second runtime
`build` path. A plugin that explicitly validates an installed provider must agree with the same
receipt; a mismatch rejects App or runtime publication. Advanced raw-plugin installation outside a
recipe remains responsible for its own schema registration.

A game adds and reconfigures the contribution through the recipe:

```rust
let recipe = ProductRecipe::new()
    .add_plugin::<DebugOverlayPlugin>()?
    .add_contribution(agent::package(AgentSettings { max_agents: 48 })?)?
    .configure_contribution(agent::package(AgentSettings { max_agents: 64 })?)?;
```

The recipe rejects duplicate plugin IDs and mismatched provider declarations before any `App` or
runtime is created.

## Report Expected Plugin Failures

Return checked static facts for an expected domain failure. The framework records the actual
invoking plugin and hook, so the error does not need to repeat that context:

```rust
use nara::app::{PluginDomainError, PluginError};

fn validate_settings(max_agents: u32) -> Result<(), PluginError> {
    if max_agents == 0 {
        return Err(PluginDomainError::new(
            "example.agents.empty-capacity",
            "Agent capacity must be greater than zero",
        ).expect("the failure facts are safe static literals").into());
    }
    Ok(())
}
```

Use this error from preflight, build, finish or shutdown. Advanced fallible `PluginDefinition`
factories return `PluginDomainError` directly. Codes and summaries accept only static text and
validate length, character and privacy rules. Keep runtime values, paths and third-party error
messages in their typed owner; do not format them into public facts. Existing free-form
`SetupFailed` and component-registration errors use the private fallback diagnostic.

Product runtime and compiled Editor Play preserve the domain code/summary and actual hook context.
Direct `App::add_plugins` returns `AddPluginsError::Plugin(PluginCommitError)` for commit failures;
inspect `error()` for the typed cause and `plugin_failure()` for actual plugin/hook context. The
first rejected preflight has this context even though it leaves App configurable with no failure
report. `PluginPrepareError::Failed` now retains `source: PluginDomainError` instead of a bare code.
The advanced consuming `App::seal` path returns `PluginCommitError` as well, preserving a failing
finish invocation after the App is consumed. Use retained plan instantiation when cleanup and
invocation evidence are required; `instantiate`/`into_error` deliberately project only the cause.
Shutdown failures remain terminal cleanup failures and do not replace the earlier startup error.
Data-only Open and strict Check/Export do not run plugin hooks to discover these errors. Factory
or hook failure still follows the existing retained-retirement contract; an error code never
authorizes retrying a partially committed App.

## Strict tools use the compiled project

Advanced managed-App Scene consumers inspect `CurrentScene` for the committed source and instance
receipt. Use `restart_current_scene_with_product` for Retry and
`replace_current_scene_with_product` for direct data-only replacement at an exclusive simulation
boundary. Both require the complete expected receipt and derive the frozen registry from the World.
The root updates source and receipt together; a product's copied receipt is a membership cache or
expected-state guard. `SceneSource` is move-only and `SceneSourceView` does not retain its owner.
Project-backed restart reuses resident content; direct replacement does not load files.

Hosted products request startup or a declared delivery root through the `ProjectScenes` system
parameter. Keep the returned `ProjectSceneRequestId` and observe its state. `Ready` exposes a
borrowed `prepared(id)` view of the expanded target document, selected path and expected current
instance. Validate game-owned checkpoint references and prepare owned runtime values before calling
`commit_prepared_project_scene` at an exclusive simulation boundary. The Host reuses its original
project/schema authority, loader budget and CPU residency transaction; gameplay does not resolve a
second RuntimePlan or construct direct sources to bypass resource ownership.

For headless games that collect local semantic commands or prepare Scenes in the background, use
`HeadlessRunIntent::frames(count, real_delta)` with `HeadlessRun::new`. It runs complete managed
frames, including TaskUpdate and PreUpdate. App time settings determine fixed ticks per frame.
`HeadlessRunIntent::new(fixed_ticks)` and `HeadlessRun::from_recipe` retain exact fixed stepping for
fixed-transaction consumers; ordinary frame producers do not run on each exact step. The selected
count is an iteration bound, not a wall-clock timeout. Observe terminal outcomes with `stop_when`.

Preparation never commits automatically. A cancelled running request stays `Cancelling` until its
worker physically returns. Inspect `diagnostics(id)` for a rejected request's safe report and
`acknowledge(id)` after a terminal result before requesting another Scene. A wrong or obsolete ID
cannot acknowledge or commit another request. These APIs preserve one current Scene and explicit
game-owned session state; they do not provide additive loading or arbitrary native-state rollback.

The project owns one recipe function shared by headless, desktop, Check, Export, and startup-Scene
Repair. Add the `project-tooling` Nara feature to the headless dependency, declare the tool entry in
the project's Cargo manifest, and dispatch it before normal argument parsing or gameplay startup:

```toml
[package.metadata.nara]
project-tool-protocol = 2
```

```rust
fn main() -> std::process::ExitCode {
    if let Some(exit) = nara::project_tooling::run_project_tool_mode(
        std::env::args_os().skip(1),
        project_recipe,
    ) {
        return exit;
    }
    run_game()
}
```

`project_recipe` returns `Result<ProductRecipe, E>`. Tool mode calls it once after admitting the
request; normal game startup calls the same definition through its ordinary entry. Keep component
and migration registration in the contribution's provider callback. Its runtime plugin does not
register that provider again.

`nara check`, `nara export`, and `nara repair startup-scene` first discover and build the existing
`headless` target. A new project can generate its initial lockfile during bootstrap; existing
lockfiles are respected with `--locked`. Strict checking then runs inside that executable with its
native schema. Export retains the checked closure, build agreement, and publication capabilities in
that process. The parent CLI receives bounded display summaries, never reconstructed authority.

Default tools require no window, graphics backend, or editor toolkit. Explicitly checking the
`desktop` profile builds the matching Cargo feature. Missing or unsupported tool metadata rejects
an old headless entry, including protocol version 1, before launch, since old entries may ignore
arguments and start gameplay.
The declaration is a compatibility contract for trusted Rust and Cargo code, not a sandbox or proof
that arbitrary project code obeys the entry contract. The generated starter includes this wiring.

## Use the same recipe in each path

Direct `App` composition retains raw one-shot plugin support. A one-shot value cannot enter a
`ProductRecipe`, because a Host must reconstruct a fresh plugin value for every runtime.

```rust
let mut app = App::new();
app.add_plugins(recipe()?.with_defaults(MinimalPlugins))?;
app.add_plugins(OneShotDebugPlugin::new())?;
let mut runtime = RuntimeInstance::start(app)?;
runtime.drive(std::time::Duration::ZERO)?;
runtime.finish()?;
```

`App` configures the runtime and has no execution methods. `RuntimeInstance` owns startup, time,
faults, and shutdown. A failed start retains its cleanup owner; an incomplete `finish` returns the
same owner for explicit retirement retry. A terminal plugin shutdown failure is also an error even
after physical retirement completes. Embedded UI hosts request Stop and advance close progress
without waiting for completion on their UI thread.

File-backed product Hosts accept the same recipe while retaining project loading, admission,
publication, and close authority:

```rust
let mut headless = HeadlessRun::<GameOutcome>::from_recipe(
    project_root,
    recipe()?,
    fixed_ticks,
    commands,
);
let report = headless.execute_bounded();
```

With the corresponding features enabled, use `DesktopRun::from_recipe(project_root, recipe()?)`
for the first-party desktop Host. Ordinary generic Editor opening reads the declared authoring
bundle and does not accept or load the game's native recipe. A user-selected local workspace uses
cooperative `ReadWrite + TrustedLocal` filesystem authority for cross-platform Save, independently
of this data-only Open and explicit project-code execution policy. An engine integration that explicitly
needs trusted in-process execution uses
`TrustedCompiledEditorProjectIntent::new().with_recipe(recipe()?)` with the trusted compiled
Editor entry. This is an advanced placement boundary, not the ordinary generic authoring path.

`PluginDefinition`, raw slot edits, parallel provider lists, runtime candidates, and retirement
ledgers remain first-party plumbing. Ordinary external runtime embedding uses
`RuntimeInstance::start`; see [Managed Runtime Runners](managed-runtime-runner.md).
Do not put file I/O, dynamic package lookup, runner
selection, or process authority into a recipe.

## One Custom Runtime UI Font

Admit one immutable face and select it once in the ordinary recipe. The public
`nara::ui::FontFace` type validates the supplied bytes; no system-font lookup or project-font asset
pipeline is implied. Unconfigured defaults use the bundled face.

```rust
let face = nara::ui::FontFace::from_bytes(font_bytes, 0)?;
let recipe = ProductRecipe::new().with_ui_font(face)?;
let mut run = HeadlessRun::<GameOutcome>::from_recipe(
    project_root, recipe, fixed_ticks, commands,
);
```

The selected profile must request `runtime-ui`. A runtime-only profile configures its UI compiler;
when the profile also requests `presentation-ui`, the same face configures its renderer. Repeated
font selection returns `ProductRecipeError::DuplicateDefaultConfiguration`; a profile without the
UI default returns `CompositionError::UnsupportedDefaultConfiguration` during admission. Check,
Export, Desktop and compiled Editor Play receive this setting through the same product recipe.

For a direct `App`, give the recipe the group that owns its defaults. A runtime-only UI app uses
`recipe.with_defaults(RuntimeUiPlugins)`. A product that presents UI can define one group containing
its runtime and presentation domains:

```rust
use nara::app::{PluginGroup, PluginGroupBuilder, PluginGroupId};
use nara::{PresentationUiPlugins, RuntimeUiPlugins};

struct UiDefaults;

impl PluginGroup for UiDefaults {
    const ID: PluginGroupId = PluginGroupId::new("game.ui-defaults");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(RuntimeUiPlugins)
            .add_group(PresentationUiPlugins)
    }
}

let mut app = App::new();
app.add_plugins(recipe.with_defaults(UiDefaults))?;
```

The enclosing group gives the configuration an explicit owner. Adding a configured recipe beside
an unrelated defaults group returns a missing-slot error instead of silently ignoring the setting.
Defaults are configured before the existing resolver checks identity, dependencies, schema and
fingerprints; an explicitly added duplicate plugin still rejects. Each definition's identity
includes the font content ID. Fresh runtime construction shares immutable font bytes while
constructing independent UI and rasterization state. A planner rejects a foreign face before
replacing its retained packet; glyph packets retain their atlas pixels through backend submission.

## Compile-Only WebAssembly Checks

CI checks the portable runtime and backend-neutral presentation libraries for
`wasm32-unknown-unknown`. Reproduce the same library checks from the engine checkout:

```text
rustup target add wasm32-unknown-unknown --toolchain 1.95.0
cargo +1.95.0 check -p nara_fs --locked --lib --target wasm32-unknown-unknown
cargo +1.95.0 check -p nara --locked --no-default-features --features runtime-core --lib --target wasm32-unknown-unknown
cargo +1.95.0 check -p nara --locked --no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib --target wasm32-unknown-unknown
```

These checks cover library compilation and dependency/trait boundaries. They do not run a game,
create browser workers, initialize WebGPU, open a canvas, or qualify input and presentation.
The native task pool still requires OS threads at runtime, and the filesystem adapter reports
unsupported capabilities on this target. Desktop runners, file watching and the Editor remain
outside this compile-only matrix. Browser WebGPU's local ownership and asynchronous initialization
remain an incomplete implementation of
[ADR 0078](../architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md).

Stable asset identity generation enables `getrandom`'s `wasm_js` backend only for this target,
using Web Crypto through `wasm-bindgen`. Native targets keep their existing entropy provider.
Successful compilation does not provide an entropy source to a non-JavaScript Wasm host.
