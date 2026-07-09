//! Renderer-facing data and backend seam.

mod pass_plan;
mod prepare;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Handle;
pub use nara_core::Color;
use nara_core::Vec2;
use nara_ecs::{Component, Entity, Query, Res, ResMut, Resource};
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldSchema, ComponentRegistry,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_transform::Transform2d;
use nara_window::{PrimaryWindowId, Window, WindowId, WindowResolution};

pub use pass_plan::{
    RenderPassDependency, RenderPassDependencyError, RenderPassNodeId, RenderPassPlan,
    RenderPassStep, RenderPassStepLabel, RenderPhaseInput, build_render_pass_plan,
    render_phase_order,
};
pub use prepare::{
    PreparedRenderResource, PreparedRenderResourceRecord, PreparedRenderResources,
    RenderPrepareApplyResult, RenderPrepareError, RenderPrepareInvalidation,
    RenderPrepareInvalidationReason, RenderPrepareInvalidations, RenderPrepareStatus,
    RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RenderBackendState {
    #[default]
    Uninitialized,
    Initializing,
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFrameSkipReason {
    NoViews,
    NoRenderableTarget,
    SurfaceUnavailable,
    BackendError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrameSkip {
    frame_index: u64,
    reason: RenderFrameSkipReason,
    message: Option<String>,
}

impl RenderFrameSkip {
    #[must_use]
    pub const fn new(frame_index: u64, reason: RenderFrameSkipReason) -> Self {
        Self {
            frame_index,
            reason,
            message: None,
        }
    }

    #[must_use]
    pub fn with_message(
        frame_index: u64,
        reason: RenderFrameSkipReason,
        message: impl Into<String>,
    ) -> Self {
        Self {
            frame_index,
            reason,
            message: Some(message.into()),
        }
    }

    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    #[must_use]
    pub const fn reason(&self) -> RenderFrameSkipReason {
        self.reason
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct RenderBackendStatus {
    backend: Option<&'static str>,
    state: RenderBackendState,
    last_error: Option<String>,
    last_skip: Option<RenderFrameSkip>,
}

impl Default for RenderBackendStatus {
    fn default() -> Self {
        Self {
            backend: None,
            state: RenderBackendState::Uninitialized,
            last_error: None,
            last_skip: None,
        }
    }
}

impl RenderBackendStatus {
    #[must_use]
    pub fn backend(&self) -> Option<&'static str> {
        self.backend
    }

    #[must_use]
    pub const fn state(&self) -> RenderBackendState {
        self.state
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[must_use]
    pub const fn last_skip(&self) -> Option<&RenderFrameSkip> {
        self.last_skip.as_ref()
    }

    pub fn mark_state(&mut self, backend: &'static str, state: RenderBackendState) {
        self.backend = Some(backend);
        self.state = state;
        if state == RenderBackendState::Ready {
            self.last_error = None;
        }
    }

    pub fn mark_ready(&mut self, backend: &'static str) {
        self.mark_state(backend, RenderBackendState::Ready);
    }

    pub fn mark_unavailable(&mut self, backend: &'static str, error: impl Into<String>) {
        self.backend = Some(backend);
        self.state = RenderBackendState::Unavailable;
        self.last_error = Some(error.into());
    }

    pub fn mark_skipped(&mut self, frame_index: u64, reason: RenderFrameSkipReason) {
        self.last_skip = Some(RenderFrameSkip::new(frame_index, reason));
    }

    pub fn mark_skipped_with_message(
        &mut self,
        frame_index: u64,
        reason: RenderFrameSkipReason,
        message: impl Into<String>,
    ) {
        self.last_skip = Some(RenderFrameSkip::with_message(frame_index, reason, message));
    }

    pub fn clear_skip(&mut self) {
        self.last_skip = None;
    }
}

#[derive(Debug, Default)]
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.render"),
            nara_app::PluginCategory::Render,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ComponentRegistry>();
        register_render_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
        app.init_resource::<ClearColor>();
        app.init_resource::<ExtractedViews>();
        app.init_resource::<RenderFrame>();
        app.init_resource::<FrameStats>();
        app.init_resource::<RenderBackendStatus>();
        app.init_resource::<RenderPrepareInvalidations>();
        app.add_systems(CoreStage::Extract, extract_views);
        app.add_systems(CoreStage::Render, begin_render_frame);
        Ok(())
    }
}

pub fn register_render_components(registry: &mut ComponentRegistry) {
    let component_id = ComponentTypeId::new("nara.render.Camera2d");
    registry
        .register_scene_component_with_fields::<Camera2d, _, _>(
            component_id.clone(),
            ComponentSchemaVersion(1),
            camera_fields(),
            |value| {
                Ok(Camera2d {
                    target: read_render_target(value.get("target"))?,
                    viewport: read_optional_viewport(value.get("viewport"))?,
                    clear_color: read_optional_color(value.get("clear_color"))?,
                    viewport_height: read_f32(value.field("viewport_height")?, "viewport_height")?,
                    order: optional_i32(value, "order")?.unwrap_or(0),
                })
            },
            |camera| {
                Ok(ComponentValue::map([
                    ("target", render_target_value(camera.target)?),
                    (
                        "viewport",
                        camera
                            .viewport
                            .map(viewport_value)
                            .transpose()?
                            .unwrap_or(ComponentValue::Null),
                    ),
                    (
                        "clear_color",
                        camera
                            .clear_color
                            .map(color_value)
                            .transpose()?
                            .unwrap_or(ComponentValue::Null),
                    ),
                    (
                        "viewport_height",
                        ComponentValue::f64(f64::from(camera.viewport_height))?,
                    ),
                    ("order", ComponentValue::I64(i64::from(camera.order))),
                ]))
            },
        )
        .expect("nara.render.Camera2d component registration should be unique");
}

fn camera_fields() -> [ComponentFieldSchema; 5] {
    [
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::String,
            ComponentValue::String("primary_window".to_string()),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["viewport"]),
            ComponentValueKind::Map,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["clear_color"]),
            ComponentValueKind::Map,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::required(
            ComponentFieldPath::from_fields(["viewport_height"]),
            ComponentValueKind::F64,
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldPath::from_fields(["order"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        ),
    ]
}

fn read_render_target(value: Option<&ComponentValue>) -> Result<RenderTarget, ComponentCodecError> {
    match value.and_then(ComponentValue::as_str) {
        None | Some("primary_window") => Ok(RenderTarget::PrimaryWindow),
        Some(_) => Err(ComponentCodecError::invalid_field(
            "target",
            "'primary_window'",
        )),
    }
}

fn render_target_value(target: RenderTarget) -> Result<ComponentValue, ComponentCodecError> {
    match target {
        RenderTarget::PrimaryWindow => Ok(ComponentValue::String("primary_window".to_string())),
        RenderTarget::Window(_) | RenderTarget::Image(_) => Err(ComponentCodecError::Message(
            "only primary window camera targets are scene-capable in this slice".to_string(),
        )),
    }
}

fn read_optional_viewport(
    value: Option<&ComponentValue>,
) -> Result<Option<ViewportRect>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => Ok(Some(
            ViewportRect::new(
                read_u32(value, "physical_x", "viewport.physical_x")?,
                read_u32(value, "physical_y", "viewport.physical_y")?,
                read_u32(value, "physical_width", "viewport.physical_width")?,
                read_u32(value, "physical_height", "viewport.physical_height")?,
            )
            .ok_or_else(|| ComponentCodecError::invalid_field("viewport", "non-empty viewport"))?,
        )),
    }
}

fn read_u32(
    value: &ComponentValue,
    field: &str,
    display_field: &str,
) -> Result<u32, ComponentCodecError> {
    let value = value.field_u64(field)?;
    u32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(display_field, "u32"))
}

fn viewport_value(value: ViewportRect) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        (
            "physical_x",
            ComponentValue::U64(u64::from(value.physical_x)),
        ),
        (
            "physical_y",
            ComponentValue::U64(u64::from(value.physical_y)),
        ),
        (
            "physical_width",
            ComponentValue::U64(u64::from(value.physical_width)),
        ),
        (
            "physical_height",
            ComponentValue::U64(u64::from(value.physical_height)),
        ),
    ]))
}

fn read_optional_color(
    value: Option<&ComponentValue>,
) -> Result<Option<Color>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => read_color(value, "clear_color").map(Some),
    }
}

fn read_color(value: &ComponentValue, field: &str) -> Result<Color, ComponentCodecError> {
    Ok(Color::rgba(
        read_f32(value.field("r")?, &format!("{field}.r"))?,
        read_f32(value.field("g")?, &format!("{field}.g"))?,
        read_f32(value.field("b")?, &format!("{field}.b"))?,
        read_f32(value.field("a")?, &format!("{field}.a"))?,
    ))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(field, "finite f32"));
    }
    Ok(value as f32)
}

fn color_value(value: Color) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(value.r))?),
        ("g", ComponentValue::f64(f64::from(value.g))?),
        ("b", ComponentValue::f64(f64::from(value.b))?),
        ("a", ComponentValue::f64(f64::from(value.a))?),
    ]))
}

fn optional_i32(value: &ComponentValue, field: &str) -> Result<Option<i32>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| {
            let value = value
                .as_i64()
                .ok_or_else(|| ComponentCodecError::invalid_field(field, "i32"))?;
            i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(field, "i32"))
        })
        .transpose()
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

    #[test]
    fn backend_status_records_state_errors_and_skipped_frames() {
        let mut status = RenderBackendStatus::default();

        status.mark_state("mock", RenderBackendState::Initializing);
        assert_eq!(status.backend(), Some("mock"));
        assert_eq!(status.state(), RenderBackendState::Initializing);

        status.mark_unavailable("mock", "device lost");
        status.mark_skipped_with_message(7, RenderFrameSkipReason::BackendError, "device lost");

        assert_eq!(status.state(), RenderBackendState::Unavailable);
        assert_eq!(status.last_error(), Some("device lost"));
        let skip = status.last_skip().unwrap();
        assert_eq!(skip.frame_index(), 7);
        assert_eq!(skip.reason(), RenderFrameSkipReason::BackendError);
        assert_eq!(skip.message(), Some("device lost"));

        status.mark_ready("mock");
        status.clear_skip();
        assert_eq!(status.state(), RenderBackendState::Ready);
        assert_eq!(status.last_error(), None);
        assert_eq!(status.last_skip(), None);
    }

    #[test]
    fn camera_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistry::new();
        register_render_components(&mut registry);

        let schema = registry
            .schema(&ComponentTypeId::new("nara.render.Camera2d"))
            .unwrap();

        assert_eq!(
            schema
                .fields
                .iter()
                .map(|field| (field.path.to_string(), field.value_kind, field.required))
                .collect::<Vec<_>>(),
            vec![
                ("clear_color".to_string(), ComponentValueKind::Map, false),
                ("order".to_string(), ComponentValueKind::I64, false),
                ("target".to_string(), ComponentValueKind::String, false),
                ("viewport".to_string(), ComponentValueKind::Map, false),
                ("viewport_height".to_string(), ComponentValueKind::F64, true),
            ]
        );
    }
}
