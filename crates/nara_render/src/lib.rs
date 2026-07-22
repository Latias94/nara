//! Renderer-facing data and backend seam.

mod pass_plan;
mod prepare;

use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext, RuntimeGeneration};
use nara_asset::Handle;
pub use nara_core::Color;
use nara_core::Vec2;
use nara_ecs::{Component, Entity, Query, Res, ResMut, Resource};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_transform::Transform2d;
use nara_window::{PresentMode, PrimaryWindowId, Window, WindowId, WindowResolution};

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

/// Immutable window state admitted for one render-frame submission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderWindowPacket {
    pub id: WindowId,
    pub resolution: WindowResolution,
    pub present_mode: PresentMode,
}

/// Owned backend-neutral render input captured before any surface acquisition.
#[derive(Debug, PartialEq)]
pub struct RenderFramePacket {
    generation: RuntimeGeneration,
    frame_index: u64,
    window: RenderWindowPacket,
    view: ExtractedView,
    pass_plan: RenderPassPlan,
}

impl RenderFramePacket {
    #[must_use]
    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    #[must_use]
    pub const fn window(&self) -> RenderWindowPacket {
        self.window
    }

    #[must_use]
    pub const fn view(&self) -> &ExtractedView {
        &self.view
    }

    #[must_use]
    pub const fn pass_plan(&self) -> &RenderPassPlan {
        &self.pass_plan
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderFramePacketError {
    #[error("the desktop renderer supports exactly one view, but captured {actual}")]
    UnsupportedViewCount { actual: usize },
    #[error("the desktop renderer supports exactly one window, but captured {actual}")]
    UnsupportedWindowCount { actual: usize },
    #[error("the desktop renderer does not support image render targets")]
    UnsupportedImageTarget,
    #[error("render target window {window_id:?} is not the admitted window")]
    TargetWindowMismatch { window_id: WindowId },
    #[error("the render viewport lies outside window {window_id:?}")]
    ViewportOutsideTarget { window_id: WindowId },
}

/// Captures the supported desktop topology without retaining a `World` borrow.
///
/// No surface operation may occur until this function has returned an admitted packet.
pub fn build_render_frame_packet<'a>(
    generation: RuntimeGeneration,
    frame_index: u64,
    primary_window_id: Option<WindowId>,
    windows: impl IntoIterator<Item = &'a Window>,
    views: &ExtractedViews,
    phases: impl IntoIterator<Item = RenderPhaseInput>,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    if views.is_empty() {
        return Ok(None);
    }
    if views.len() != 1 {
        return Err(RenderFramePacketError::UnsupportedViewCount {
            actual: views.len(),
        });
    }

    let windows = windows.into_iter().collect::<Vec<_>>();
    if windows.len() != 1 {
        return Err(RenderFramePacketError::UnsupportedWindowCount {
            actual: windows.len(),
        });
    }

    let view = views.as_slice()[0];
    let window_id = match view.target {
        RenderTarget::PrimaryWindow => primary_window_id.unwrap_or(WindowId::PRIMARY),
        RenderTarget::Window(window_id) => window_id,
        RenderTarget::Image(_) => return Err(RenderFramePacketError::UnsupportedImageTarget),
    };
    let window = windows[0];
    if window.id != window_id {
        return Err(RenderFramePacketError::TargetWindowMismatch { window_id });
    }

    let viewport_right =
        u64::from(view.viewport.physical_x).saturating_add(u64::from(view.viewport.physical_width));
    let viewport_bottom = u64::from(view.viewport.physical_y)
        .saturating_add(u64::from(view.viewport.physical_height));
    if viewport_right > u64::from(window.resolution.physical_width)
        || viewport_bottom > u64::from(window.resolution.physical_height)
    {
        return Err(RenderFramePacketError::ViewportOutsideTarget { window_id });
    }

    Ok(Some(RenderFramePacket {
        generation,
        frame_index,
        window: RenderWindowPacket {
            id: window.id,
            resolution: window.resolution,
            present_mode: window.present_mode,
        },
        view,
        pass_plan: build_render_pass_plan(views, phases),
    }))
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
    InvalidTopology,
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

pub const RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.render");
pub const RENDER_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.render.components");
pub const RENDER_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        RENDER_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.render.components.native", 1),
        validate_render_components,
        register_render_components,
    );
pub const RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[RENDER_SCHEMA_PROVIDER_ID]);

impl Plugin for RenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &RENDER_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.render.Camera2d");
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            RENDER_PLUGIN_ID,
            component_id.as_str(),
        )?;
        RENDER_SCHEMA_PROVIDER.preflight(registry).map_err(|error| {
            PluginError::component_registration(RENDER_PLUGIN_ID, component_id.as_str(), error)
        })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.render.Camera2d");
        RENDER_SCHEMA_PROVIDER
            .register_or_validate_into(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(RENDER_PLUGIN_ID, component_id.as_str(), error)
            })?;
        app.init_resource::<ClearColor>()?;
        app.init_resource::<ExtractedViews>()?;
        app.init_resource::<RenderFrame>()?;
        app.init_resource::<FrameStats>()?;
        app.init_resource::<RenderBackendStatus>()?;
        app.init_resource::<RenderPrepareInvalidations>()?;
        app.add_systems(CoreStage::Extract, extract_views)?;
        app.add_systems(CoreStage::Render, begin_render_frame)?;
        Ok(())
    }
}

pub fn register_render_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_render_components(registry)?;
    let component_id = ComponentTypeId::new("nara.render.Camera2d");
    let schema = ComponentSchema::new(component_id, "Camera 2D", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(camera_fields());
    registry.register_persistent_component_with_codec::<Camera2d, _, _>(
        schema,
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
    )?;
    Ok(())
}

fn validate_render_components(registry: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry
        .validate_component_registration::<Camera2d>(&ComponentTypeId::new("nara.render.Camera2d"))
}

fn camera_fields() -> [ComponentFieldSchema; 5] {
    [
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::String,
            ComponentValue::String("primary_window".to_string()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("viewport"),
            "Viewport",
            ComponentFieldPath::from_fields(["viewport"]),
            ComponentValueKind::Map,
            ComponentValue::Null,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("clear_color"),
            "Clear color",
            ComponentFieldPath::from_fields(["clear_color"]),
            ComponentValueKind::Map,
            ComponentValue::Null,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("viewport_height"),
            "Viewport height",
            ComponentFieldPath::from_fields(["viewport_height"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("order"),
            "Order",
            ComponentFieldPath::from_fields(["order"]),
            ComponentValueKind::I64,
            ComponentValue::I64(0),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
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

    use nara_app::{App, RuntimeCandidate, RuntimeCandidateRetirementState};
    use nara_asset::AssetId;
    use nara_reflect::ComponentRegistryPlugin;
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
    fn frame_packet_admits_one_window_view() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(test_extracted_view());

        let packet = build_render_frame_packet(
            generation,
            9,
            Some(WindowId::PRIMARY),
            [&window],
            &views,
            [RenderPhaseInput {
                view_index: 0,
                phase: RenderPhaseLabel::TRANSPARENT_2D,
            }],
        )
        .unwrap()
        .unwrap();

        assert_eq!(packet.generation(), generation);
        assert_eq!(packet.frame_index(), 9);
        assert_eq!(packet.window().id, WindowId::PRIMARY);
        assert_eq!(packet.view(), &test_extracted_view());
        assert_eq!(packet.pass_plan().len(), 2);
    }

    #[test]
    fn frame_packet_rejects_extra_or_unsupported_topology() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(test_extracted_view());
        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(2).unwrap(),
            ..test_extracted_view()
        });
        assert!(matches!(
            build_render_frame_packet(generation, 1, None, [&window], &views, []),
            Err(RenderFramePacketError::UnsupportedViewCount { actual: 2 })
        ));

        let mut views = ExtractedViews::default();
        views.push(test_extracted_view());
        let second_window = Window::default().with_id(WindowId::new(2));
        assert!(matches!(
            build_render_frame_packet(generation, 1, None, [&window, &second_window], &views, [],),
            Err(RenderFramePacketError::UnsupportedWindowCount { actual: 2 })
        ));

        let mut image_views = ExtractedViews::default();
        image_views.push(ExtractedView {
            target: RenderTarget::Image(Handle::new(AssetId::from_raw(17))),
            ..test_extracted_view()
        });
        assert!(matches!(
            build_render_frame_packet(generation, 1, None, [&window], &image_views, []),
            Err(RenderFramePacketError::UnsupportedImageTarget)
        ));
    }

    #[test]
    fn frame_packet_rejects_mismatched_window_and_out_of_bounds_viewport() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let target_window = WindowId::new(2);
        let mut mismatched = ExtractedViews::default();
        mismatched.push(ExtractedView {
            target: RenderTarget::Window(target_window),
            ..test_extracted_view()
        });
        assert_eq!(
            build_render_frame_packet(generation, 1, None, [&window], &mismatched, []),
            Err(RenderFramePacketError::TargetWindowMismatch {
                window_id: target_window,
            })
        );

        let mut outside = ExtractedViews::default();
        outside.push(ExtractedView {
            viewport: ViewportRect::new(1_279, 719, 2, 2).unwrap(),
            ..test_extracted_view()
        });
        assert_eq!(
            build_render_frame_packet(generation, 1, None, [&window], &outside, []),
            Err(RenderFramePacketError::ViewportOutsideTarget {
                window_id: WindowId::PRIMARY,
            })
        );
    }

    #[test]
    fn captured_packet_ignores_later_topology_mutation_and_next_capture_rejects() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(test_extracted_view());
        let packet = build_render_frame_packet(generation, 1, None, [&window], &views, [])
            .unwrap()
            .unwrap();

        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(2).unwrap(),
            ..test_extracted_view()
        });

        assert_eq!(
            packet.view().camera_entity,
            Entity::from_raw_u32(1).unwrap()
        );
        assert!(matches!(
            build_render_frame_packet(generation, 2, None, [&window], &views, []),
            Err(RenderFramePacketError::UnsupportedViewCount { actual: 2 })
        ));
    }

    #[test]
    fn extracts_primary_window_camera_view() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.add_plugin(WindowPlugin::default()).unwrap();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut()
            .expect("app should allow world mutation")
            .spawn(Camera2d::default());

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

    fn test_extracted_view() -> ExtractedView {
        ExtractedView {
            camera_entity: Entity::from_raw_u32(1).unwrap(),
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 1280, 720).unwrap(),
            world_position: Vec2::ZERO,
            viewport_height: 720.0,
            order: 0,
            clear_color: Color::BLACK,
        }
    }

    fn test_runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeCandidate::admit(App::new().seal().unwrap()).unwrap();
        let ready = candidate.complete_startup().unwrap();
        let runtime = ready.promote();
        let generation = runtime.generation();
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        generation
    }

    #[test]
    fn extracts_explicit_image_target_view_without_window() {
        let mut app = App::new();
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut()
            .expect("app should allow world mutation")
            .spawn(Camera2d {
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
        app.add_plugin(ComponentRegistryPlugin).unwrap();
        app.add_plugin(RenderPlugin).unwrap();
        app.world_mut()
            .expect("app should allow world mutation")
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
        register_render_components(&mut registry).expect("component registration should succeed");
        registry.freeze().expect("component registry should freeze");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.render.Camera2d"))
            .unwrap();
        let mut fields = schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            fields,
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
