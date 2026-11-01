#![forbid(unsafe_code)]

//! Manual real Editor Scene View consumer; close the Editor through its ordinary UI.

use nara::project_host::TrustedCompiledEditorProjectIntent;
use nara_editor::{EditorLaunchOptions, run_trusted_compiled};
use nara_stock_composition_consumer::{CompositionConfiguration, recipe};

#[path = "../tests/support/scene.rs"]
#[allow(dead_code)]
mod scene;

fn main() -> scene::Result<()> {
    let backend = std::env::var("WGPU_BACKEND")?;
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let recipe = recipe(CompositionConfiguration::default())?.add_plugin::<scene::HudPlugin>()?;
    let fixture = scene::SourceFixture::new(recipe.clone())?;
    // The actual Scene View obtains a fresh adapter from this same recipe factory. No stock
    // SpriteBatch construction, private preview generation or private Scene View path is used.
    run_trusted_compiled(
        EditorLaunchOptions::new(fixture.root()),
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe),
    )?;
    // Closing the Editor is not evidence that pixels, picking or presentation were correct.
    Ok(())
}
