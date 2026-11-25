use nara::{
    asset::Assets,
    ecs::World,
    image::ImageAsset,
    input::{
        ActionMaps, ActionOutcomes, ButtonInput, InputTimeline, KeyCode, MouseButton, PointerState,
        SemanticActions,
    },
    material::Material2dAsset,
    render::{RenderBackendStatus, RenderFrame},
    tilemap::TileSetAssets,
    window::{
        PrimaryWindowId, WindowEvents,
        backend::{BackendWindowHandles, WindowPresentationRequirement},
    },
};

pub(super) fn verify_absent(world: &World) -> Result<(), &'static str> {
    if world.contains_resource::<InputTimeline>()
        || world.contains_resource::<ActionMaps>()
        || world.contains_resource::<ActionOutcomes>()
        || world.contains_resource::<SemanticActions>()
        || world.contains_resource::<PointerState>()
        || world.contains_resource::<ButtonInput<KeyCode>>()
        || world.contains_resource::<ButtonInput<MouseButton>>()
        || world.contains_resource::<WindowEvents>()
        || world.contains_resource::<PrimaryWindowId>()
        || world.contains_resource::<BackendWindowHandles>()
        || world.contains_resource::<WindowPresentationRequirement>()
        || world.contains_resource::<RenderFrame>()
        || world.contains_resource::<RenderBackendStatus>()
        || world.contains_resource::<Assets<ImageAsset>>()
        || world.contains_resource::<Assets<Material2dAsset>>()
        || world.contains_resource::<TileSetAssets>()
    {
        return Err("Server World contains Input, Window or presentation services");
    }
    Ok(())
}
