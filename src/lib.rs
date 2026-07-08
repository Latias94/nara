//! Public facade for the nara engine workspace.

pub use nara_app as app;
pub use nara_asset as asset;
pub use nara_audio as audio;
pub use nara_core as core;
pub use nara_diagnostic as diagnostic;
pub use nara_ecs as ecs;
pub use nara_input as input;
pub use nara_reflect as reflect;
pub use nara_render as render;
#[cfg(feature = "wgpu")]
pub use nara_render_wgpu as render_wgpu;
pub use nara_scene as scene;
pub use nara_sprite as sprite;
pub use nara_sprite_render as sprite_render;
pub use nara_tilemap as tilemap;
pub use nara_tooling as tooling;
pub use nara_transform as transform;
pub use nara_window as window;
#[cfg(feature = "winit")]
pub use nara_winit as winit;

use nara_app::{App, Plugin};

/// Minimal runtime defaults for headless examples, tests, and AI-generated scenes.
///
/// Platform windows and GPU backends are intentionally excluded. They should be
/// installed by dedicated backend plugins.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl Plugin for MinimalPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugin(nara_scene::HierarchyPlugin)
            .expect("HierarchyPlugin should be unique");
        app.add_plugin(nara_transform::TransformPlugin)
            .expect("TransformPlugin should be unique");
        app.add_plugin(nara_input::InputPlugin)
            .expect("InputPlugin should be unique");
        app.add_plugin(nara_sprite::SpritePlugin)
            .expect("SpritePlugin should be unique");
        app.add_plugin(nara_tilemap::TilemapPlugin)
            .expect("TilemapPlugin should be unique");
        app.add_plugin(nara_render::RenderPlugin)
            .expect("RenderPlugin should be unique");
        app.add_plugin(nara_sprite_render::SpriteRenderPlugin)
            .expect("SpriteRenderPlugin should be unique");
    }
}

pub mod prelude {
    pub use crate::MinimalPlugins;
    pub use nara_app::{
        App, AppExit, AppRunError, CoreStage, FixedTime, Plugin, PluginError, StartupStage, Time,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetPathError, AssetRef, AssetRefError, AssetServer, Assets,
        Handle,
    };
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity};
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_input::{ButtonInput, InputPlugin, InputState, KeyCode, MouseButton};
    pub use nara_reflect::{
        ComponentCodec, ComponentCodecError, ComponentFloat, ComponentRegistry,
        ComponentRegistryError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueError, PreparedComponent,
    };
    pub use nara_render::{
        Camera2d, ClearColor, Extent2d, ExtractedView, ExtractedViews, FrameStats, RenderBackend,
        RenderError, RenderFrame, RenderFrameState, RenderImage2d, RenderPhaseLabel, RenderPlugin,
        RenderTarget, ViewportRect,
    };
    #[cfg(feature = "wgpu")]
    pub use nara_render_wgpu::{
        SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuBackendState,
        WgpuRenderBackend, WgpuRenderError, WgpuRenderPlugin,
    };
    pub use nara_scene::{
        Children, HierarchyPlugin, Name, Parent, PrefabDocument, PrefabInstance,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityIdError, SceneEntityMap,
        SceneEntityRecord, SceneEntitySource, SceneExportReport, SceneFormatError, SceneInstanceId,
        SceneSpawnReport, SceneSpawner, Visibility, export_scene, spawn_child, spawn_scene,
        sync_children,
    };
    pub use nara_sprite::{Sprite, SpriteAnchor, SpritePlugin, Texture2d, TextureRegion};
    pub use nara_sprite_render::{
        ExtractedSprite, ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItem,
        QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance, SpriteRenderPlugin,
        SpriteRenderStats,
    };
    pub use nara_tilemap::{
        DEFAULT_CHUNK_SIZE, DEFAULT_TILE_SIZE, DirtyTileChunk, TileCell, TileChunkCoord, TileCoord,
        TileIndex, TileLayer, TileSet, Tilemap, TilemapPlugin,
    };
    pub use nara_tooling::{ToolingPlugin, WorldSnapshot};
    pub use nara_transform::{GlobalTransform2d, Transform2d, TransformPlugin};
    pub use nara_window::{
        PresentMode, PrimaryWindow, PrimaryWindowId, Window, WindowEvent, WindowEvents, WindowId,
        WindowMode, WindowPlugin, WindowResolution, apply_window_event, push_window_event,
    };
    #[cfg(feature = "winit")]
    pub use nara_winit::{WinitControlFlow, WinitPlugin, WinitRunner};
}
