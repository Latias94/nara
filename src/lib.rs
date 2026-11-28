//! Public facade for code-first Nara games.

#[cfg(all(feature = "asset-watch", any(unix, windows)))]
pub use nara_asset::watch as asset_watch;
#[cfg(feature = "audio")]
pub use nara_audio as audio;
#[cfg(feature = "runtime-input")]
pub use nara_input as input;
#[cfg(feature = "physics-2d")]
pub use nara_physics2d as physics2d;
#[cfg(any(
    feature = "runtime-2d",
    feature = "presentation-ui",
    feature = "render-wgpu",
    feature = "material-authoring"
))]
pub use nara_render as render;
#[cfg(feature = "render-wgpu")]
pub use nara_render::gpu as render_wgpu;
#[cfg(all(feature = "save", any(unix, windows)))]
pub use nara_scene::save;
#[cfg(feature = "runtime-ui")]
pub use nara_ui as ui;
#[cfg(feature = "runtime-window")]
pub use nara_window as window;
#[cfg(all(feature = "desktop-winit", not(target_family = "wasm")))]
pub use nara_window::winit;
#[cfg(feature = "runtime-core")]
pub use {nara_app as app, nara_asset as asset, nara_core as core, nara_ecs as ecs};
#[cfg(feature = "runtime-core")]
pub use {nara_app::tasks, nara_reflect as reflect, nara_scene as scene};

#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub mod __macro_support {
    pub use nara_reflect::__macro_support::*;
}

#[cfg(feature = "runtime-core")]
mod plugins;

#[cfg(feature = "runtime-core")]
pub use plugins::{DefaultPlugins, MinimalPlugins};

#[cfg(feature = "runtime-core")]
pub mod prelude {
    pub use crate::{DefaultPlugins, MinimalPlugins};
    pub use nara_app::CoreStage::{
        Extract as RenderExtract, First, FixedUpdate, Last, PostUpdate, PreUpdate, Update,
    };
    pub use nara_app::StartupStage::Scene as Startup;
    pub use nara_app::{
        AddPluginsError, App, AppExit, AppExitRequests, AppRunError, CoreStage, FixedTime,
        FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginGroup,
        PluginId, RealTime, RuntimeInstance, RuntimeState, StartupStage, VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetAppExt, AssetServer, Assets, Handle, RuntimeAssetSource, load_asset_sync,
    };
    #[cfg(feature = "audio")]
    pub use nara_audio::{
        AudioClip, AudioClipSource, AudioFinished, AudioListener, AudioMixer, AudioPlayback,
        AudioPlayer, SpatialAudio,
    };
    pub use nara_core::{AssetId, Color, Vec2, Vec3};
    pub use nara_ecs::{
        Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, SystemParam, World,
    };
    #[cfg(feature = "runtime-input")]
    pub use nara_input::{
        ActionBinding, ActionId, ActionMap, ActionMaps, ActionState, AxisDirection, ButtonInput,
        GamepadAxis, GamepadInput, GamepadInputKind, ImeEvent, ImeEventKind, KeyCode, MouseButton,
        MouseWheel, MouseWheelUnit, PointerState, TextInput, TouchInput, TouchPhase,
    };
    #[cfg(feature = "physics-2d")]
    pub use nara_physics2d::{
        Collider2d, CollisionEvent, CollisionGroups2d, CompoundBox2d, CompoundCollider2d,
        Damping2d, ExternalForce2d, ExternalImpulse2d, GravityScale2d, LockedAxes2d,
        Physics2dPlugin, Physics2dSet, PhysicsMaterial2d, PhysicsPose2d, PhysicsQueryFilter2d,
        PhysicsWorld2d, PlatformerAutostep, PlatformerCharacter, PlatformerConfig,
        PlatformerJumpTimers, PlatformerMovement, RigidBody2d, Sensor2d, Teleport2d,
        TopDownCharacter, Velocity2d,
    };
    pub use nara_reflect::{RegisterSchema, Schema};
    #[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
    pub use nara_render::image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImagePlugin,
    };
    #[cfg(all(feature = "runtime-2d", feature = "scene-cooked"))]
    pub use nara_render::sprite::SpriteSourceHydrationPlugin;
    #[cfg(feature = "runtime-2d")]
    pub use nara_render::sprite::{Sprite, SpriteAnchor, SpriteMaterial, TextureRegion};
    #[cfg(feature = "runtime-2d")]
    pub use nara_render::tilemap::{
        TileAtlasLayout, TileCell, TileCoord, TileDefinition, TileIndex, TileLayer, TileSet,
        TileSetAsset, Tilemap,
    };
    #[cfg(any(
        feature = "runtime-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{Camera2d, RenderImage2d, RenderTarget};
    #[cfg(any(
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{ClearColor, RenderPlugin, WorldCanvas, WorldPointer};
    pub use nara_scene::Name;
    #[cfg(feature = "scene-cooked")]
    pub use nara_scene::components::TilemapSource;
    pub use nara_scene::spatial::{
        ChildOf, Children, GlobalTransform2d, InheritedVisibility, SpatialPlugin, SpatialSet,
        Transform2d, Visibility,
    };
    #[cfg(feature = "scene-cooked")]
    pub use nara_scene::{
        RegisterSceneComponent, SceneAsset, SceneEvent, SceneInstanceId, SceneLoadState,
        SceneManager, SceneMember, ScenePlugin, SceneRegistry, switch_scene_asset_sync,
        switch_scene_sync,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::{UiNode, UiRoot, UiRuntimeKey, UiText};
}
