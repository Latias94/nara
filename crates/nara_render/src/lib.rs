//! Renderer-facing data and backend seam.

use nara_app::{App, CoreStage, Plugin};
use nara_asset::Handle;
pub use nara_core::Color;
use nara_core::Vec2;
use nara_ecs::{Component, Entity, Query, Res, ResMut, Resource, World};
use nara_transform::Transform2d;
use nara_window::{PrimaryWindowId, Window, WindowId, WindowResolution};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}

impl Extent2d {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            None
        } else {
            Some(Self { width, height })
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClearColor(pub Color);

impl Default for ClearColor {
    fn default() -> Self {
        Self(Color::BLACK)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RenderImage2d {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderTarget {
    PrimaryWindow,
    Window(WindowId),
    Image(Handle<RenderImage2d>),
}

impl Default for RenderTarget {
    fn default() -> Self {
        Self::PrimaryWindow
    }
}

impl RenderTarget {
    #[must_use]
    pub fn window_id(self, primary_window_id: Option<WindowId>) -> Option<WindowId> {
        match self {
            Self::PrimaryWindow => Some(primary_window_id.unwrap_or(WindowId::PRIMARY)),
            Self::Window(window_id) => Some(window_id),
            Self::Image(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ViewportRect {
    pub physical_x: u32,
    pub physical_y: u32,
    pub physical_width: u32,
    pub physical_height: u32,
}

impl ViewportRect {
    #[must_use]
    pub const fn new(
        physical_x: u32,
        physical_y: u32,
        physical_width: u32,
        physical_height: u32,
    ) -> Option<Self> {
        if physical_width == 0 || physical_height == 0 {
            None
        } else {
            Some(Self {
                physical_x,
                physical_y,
                physical_width,
                physical_height,
            })
        }
    }

    #[must_use]
    pub const fn from_extent(extent: Extent2d) -> Option<Self> {
        Self::new(0, 0, extent.width, extent.height)
    }

    #[must_use]
    pub const fn from_window_resolution(resolution: WindowResolution) -> Option<Self> {
        Self::new(0, 0, resolution.physical_width, resolution.physical_height)
    }

    #[must_use]
    pub const fn extent(&self) -> Extent2d {
        Extent2d {
            width: self.physical_width,
            height: self.physical_height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderPhaseLabel(&'static str);

impl RenderPhaseLabel {
    pub const OPAQUE_2D: Self = Self("opaque_2d");
    pub const TRANSPARENT_2D: Self = Self("transparent_2d");
    pub const TILEMAP_2D: Self = Self("tilemap_2d");
    pub const GIZMO: Self = Self("gizmo");
    pub const UI: Self = Self("ui");

    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Camera2d {
    pub target: RenderTarget,
    pub viewport: Option<ViewportRect>,
    pub clear_color: Option<Color>,
    pub viewport_height: f32,
    pub order: i32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            target: RenderTarget::PrimaryWindow,
            viewport: None,
            clear_color: None,
            viewport_height: 720.0,
            order: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedView {
    pub camera_entity: Entity,
    pub target: RenderTarget,
    pub viewport: ViewportRect,
    pub world_position: Vec2,
    pub viewport_height: f32,
    pub order: i32,
    pub clear_color: Color,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ExtractedViews {
    views: Vec<ExtractedView>,
}

impl ExtractedViews {
    pub fn clear(&mut self) {
        self.views.clear();
    }

    pub fn push(&mut self, view: ExtractedView) {
        self.views.push(view);
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ExtractedView] {
        &self.views
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.views.len()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RenderFrameState {
    #[default]
    Idle,
    Extracting,
    Rendering,
    Submitted,
    Skipped,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct RenderFrame {
    pub index: u64,
    pub state: RenderFrameState,
}

impl RenderFrame {
    pub fn begin_extract(&mut self) {
        self.index = self.index.saturating_add(1);
        self.state = RenderFrameState::Extracting;
    }

    pub fn begin_render(&mut self) {
        self.state = RenderFrameState::Rendering;
    }

    pub fn mark_submitted(&mut self) {
        self.state = RenderFrameState::Submitted;
    }

    pub fn mark_skipped(&mut self) {
        self.state = RenderFrameState::Skipped;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub struct FrameStats {
    pub draw_calls: u32,
    pub sprites: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RenderError {
    #[error("render backend unavailable")]
    BackendUnavailable,
    #[error("surface unavailable")]
    SurfaceUnavailable,
}

pub trait RenderBackend {
    fn resize(&mut self, size: Extent2d);

    fn render(&mut self, world: &World) -> Result<FrameStats, RenderError>;
}

#[derive(Debug, Default)]
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClearColor>();
        app.init_resource::<ExtractedViews>();
        app.init_resource::<RenderFrame>();
        app.init_resource::<FrameStats>();
        app.add_systems(CoreStage::Extract, extract_views);
        app.add_systems(CoreStage::Render, begin_render_frame);
    }
}

pub fn extract_views(
    mut frame: ResMut<RenderFrame>,
    mut extracted_views: ResMut<ExtractedViews>,
    clear_color: Res<ClearColor>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    cameras: Query<(Entity, &Camera2d, Option<&Transform2d>)>,
) {
    frame.begin_extract();
    extracted_views.clear();

    let primary_window_id = primary_window_id.map(|resource| resource.0);

    for (camera_entity, camera, transform) in cameras.iter() {
        let Some(viewport) = camera
            .viewport
            .or_else(|| viewport_for_target(camera.target, primary_window_id, &windows))
        else {
            continue;
        };

        extracted_views.push(ExtractedView {
            camera_entity,
            target: camera.target,
            viewport,
            world_position: transform.map_or(Vec2::ZERO, |transform| transform.translation),
            viewport_height: camera.viewport_height,
            order: camera.order,
            clear_color: camera.clear_color.unwrap_or(clear_color.0),
        });
    }
}

pub fn begin_render_frame(mut frame: ResMut<RenderFrame>) {
    frame.begin_render();
}

fn viewport_for_target(
    target: RenderTarget,
    primary_window_id: Option<WindowId>,
    windows: &Query<&Window>,
) -> Option<ViewportRect> {
    let window_id = target.window_id(primary_window_id)?;
    windows
        .iter()
        .find(|window| window.id == window_id)
        .and_then(|window| ViewportRect::from_window_resolution(window.resolution))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use nara_app::App;
    use nara_asset::AssetId;
    use nara_window::WindowPlugin;

    #[test]
    fn default_camera_targets_primary_window() {
        let camera = Camera2d::default();

        assert_eq!(camera.target, RenderTarget::PrimaryWindow);
        assert_eq!(camera.viewport, None);
        assert_eq!(camera.viewport_height, 720.0);
        assert_eq!(camera.order, 0);
    }

    #[test]
    fn viewport_rejects_empty_extents() {
        assert_eq!(ViewportRect::new(0, 0, 0, 720), None);
        assert_eq!(ViewportRect::new(0, 0, 1280, 0), None);
        assert_eq!(
            ViewportRect::new(0, 0, 1280, 720).unwrap().extent(),
            Extent2d {
                width: 1280,
                height: 720,
            }
        );
    }

    #[test]
    fn extracts_primary_window_camera_view() {
        let mut app = App::new();
        app.add_plugin(WindowPlugin::default()).unwrap();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut().spawn(Camera2d::default());

        app.run_once(Duration::ZERO).unwrap();

        let views = app.world().resource::<ExtractedViews>();
        assert_eq!(views.len(), 1);
        assert_eq!(views.as_slice()[0].target, RenderTarget::PrimaryWindow);
        assert_eq!(views.as_slice()[0].world_position, Vec2::ZERO);
        assert_eq!(views.as_slice()[0].viewport_height, 720.0);
        assert_eq!(
            views.as_slice()[0].viewport,
            ViewportRect::new(0, 0, 1280, 720).unwrap()
        );
        assert_eq!(views.as_slice()[0].clear_color, Color::BLACK);
    }

    #[test]
    fn extracts_explicit_image_target_view_without_window() {
        let mut app = App::new();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut().spawn(Camera2d {
            target: RenderTarget::Image(Handle::new(AssetId::from_raw(7))),
            viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
            clear_color: Some(Color::WHITE),
            ..Camera2d::default()
        });

        app.run_once(Duration::ZERO).unwrap();

        let views = app.world().resource::<ExtractedViews>();
        assert_eq!(views.len(), 1);
        assert_eq!(
            views.as_slice()[0].viewport,
            ViewportRect::new(0, 0, 320, 180).unwrap()
        );
        assert_eq!(views.as_slice()[0].clear_color, Color::WHITE);
    }

    #[test]
    fn extraction_clears_stale_views_when_camera_or_window_is_missing() {
        let mut app = App::new();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut()
            .resource_mut::<ExtractedViews>()
            .push(ExtractedView {
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::PrimaryWindow,
                viewport: ViewportRect::new(0, 0, 1, 1).unwrap(),
                world_position: Vec2::ZERO,
                viewport_height: 1.0,
                order: 0,
                clear_color: Color::WHITE,
            });

        app.run_once(Duration::ZERO).unwrap();

        assert!(app.world().resource::<ExtractedViews>().is_empty());
    }

    #[test]
    fn render_frame_lifecycle_advances() {
        let mut frame = RenderFrame::default();

        frame.begin_extract();
        assert_eq!(frame.index, 1);
        assert_eq!(frame.state, RenderFrameState::Extracting);

        frame.begin_render();
        assert_eq!(frame.state, RenderFrameState::Rendering);

        frame.mark_submitted();
        assert_eq!(frame.state, RenderFrameState::Submitted);
    }
}
