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
publication. A plugin may still validate the same provider during installation, but a different
receipt is rejected before the App or runtime is published. A raw plugin installed outside a recipe
remains responsible for its own schema registration.

A game adds and reconfigures the contribution through the recipe:

```rust
let recipe = ProductRecipe::new()
    .add_plugin::<DebugOverlayPlugin>()?
    .add_contribution(agent::package(AgentSettings { max_agents: 48 })?)?
    .configure_contribution(agent::package(AgentSettings { max_agents: 64 })?)?;
```

The recipe rejects duplicate plugin IDs and mismatched provider declarations before any `App` or
runtime is created.

## Use the same recipe in each path

Direct `App` composition retains raw one-shot plugin support. A one-shot value cannot enter a
`ProductRecipe`, because a Host must reconstruct a fresh plugin value for every runtime.

```rust
let mut app = App::new();
app.add_plugins((MinimalPlugins, recipe()?))?;
app.add_plugins(OneShotDebugPlugin::new())?;
```

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
