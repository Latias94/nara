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

## Strict tools use the compiled project

The project owns one recipe function shared by headless, desktop, Check, Export, and startup-Scene
Repair. Add the `project-tooling` Nara feature to the headless dependency, declare the tool entry in
the project's Cargo manifest, and dispatch it before normal argument parsing or gameplay startup:

```toml
[package.metadata.nara]
project-tool-protocol = 1
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
an old headless entry before launch, since old entries may ignore arguments and start gameplay.
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
for the first-party desktop Host, or pass the recipe through
`EditorProjectIntent::new().with_recipe(recipe()?)` when opening an editor project session.

`PluginDefinition`, raw slot edits, parallel provider lists, runtime candidates, and retirement
ledgers remain advanced embedding APIs. Do not put file I/O, dynamic package lookup, runner
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
