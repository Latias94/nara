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
pub use nara_scene as scene;
pub use nara_tooling as tooling;
pub use nara_transform as transform;

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
        app.add_plugin(nara_input::InputPlugin)
            .expect("InputPlugin should be unique");
        app.add_plugin(nara_render::RenderPlugin)
            .expect("RenderPlugin should be unique");
    }
}

pub mod prelude {
    pub use crate::MinimalPlugins;
    pub use nara_app::{App, CoreStage, Plugin, PluginError, StartupStage};
    pub use nara_asset::{Asset, AssetId, AssetPath, AssetServer, Assets, Handle};
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Time, Vec2, Vec3};
    pub use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity};
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_input::{ButtonInput, InputPlugin, InputState, KeyCode, MouseButton};
    pub use nara_reflect::{
        ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    };
    pub use nara_render::{
        Camera2d, ClearColor, Extent2d, FrameStats, RenderBackend, RenderError, RenderPlugin,
        Sprite, Texture2d,
    };
    pub use nara_scene::{
        Children, HierarchyPlugin, Name, Parent, Scene, SceneAsset, SceneNode, Visibility,
        spawn_child, sync_children,
    };
    pub use nara_tooling::{ToolingPlugin, WorldSnapshot};
    pub use nara_transform::{GlobalTransform2d, Transform2d};
}
