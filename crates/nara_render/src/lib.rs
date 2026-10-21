//! Renderer-facing data and backend seam.

extern crate nara_ecs as bevy_ecs;

mod canvas;
mod frame;
mod prepare;
mod presentation;
mod target;
mod view;
mod world_pointer;

pub use canvas::{WorldCanvas, WorldCanvasFrame};
use frame::build_render_frame_packet_for_source;
pub use frame::{
    MAX_VIEWS_PER_TARGET, RenderFramePacket, RenderFramePacketError, RenderFrameSourceGeneration,
    build_render_frame_packet, build_render_frame_packet_with_world_canvas,
};
pub use presentation::{DisplayMapping2d, DisplayRect, DisplaySampling, DisplayScale};
pub use target::{
    Extent2d, RenderFrameTarget, RenderImage2d, RenderImagePacket, RenderTarget,
    RenderWindowPacket, ViewportRect,
};
pub use view::{
    Camera2d, ExtractedView, ExtractedViews, ViewLoadOp, ViewMapping2d, viewport_clip_to_physical,
    viewport_physical_to_clip,
};
pub use world_pointer::WorldPointer;

#[derive(Debug, Clone, Copy, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClearColor(pub Color);

impl Default for ClearColor {
    fn default() -> Self {
        Self(Color::BLACK)
    }
}

#[cfg(test)]
use nara_app::RuntimeGeneration;
use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext};
pub use nara_core::Color;
#[cfg(test)]
use nara_core::Vec2;
use nara_ecs::{
    Entity, Query, Res, ResMut, Resource, error::BevyError, schedule::IntoScheduleConfigs,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_transform::{GlobalTransform2d, Transform2d};
use nara_window::{PrimaryWindowId, Window, WindowId};

pub use prepare::{
    PreparedRenderResource, PreparedRenderResourceRecord, PreparedRenderResources,
    RenderPrepareError, RenderPrepareStatus, RenderResourceKey, RenderResourceKind,
    RenderResourceSnapshot,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::world_pointer::{
        PresentedWorldFrame, WorldPresentationGuard, WorldPresentationRevoker,
        prepare_world_presentation,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    use nara_ecs::SystemSet;

    use super::{
        Extent2d, ExtractedViews, RenderFramePacket, RenderFramePacketError,
        RenderFrameSourceGeneration, RenderFrameTarget, RenderImage2d, RenderImagePacket,
        build_render_frame_packet_for_source,
    };
    use nara_asset::Handle;

    static NEXT_EDITOR_RENDER_GENERATION: AtomicU64 = AtomicU64::new(1);

    /// Process-local identity for one private Editor preview producer.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct EditorRenderGeneration(u64);

    impl EditorRenderGeneration {
        pub fn try_acquire() -> Option<Self> {
            NEXT_EDITOR_RENDER_GENERATION
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    current.checked_add(1)
                })
                .ok()
                .map(Self)
        }
    }

    /// Captures one private Editor preview image without pretending it is a managed runtime.
    pub fn build_editor_render_frame_packet(
        generation: EditorRenderGeneration,
        frame_index: u64,
        views: &ExtractedViews,
        target: RenderImagePacket,
    ) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
        let packet = build_render_frame_packet_for_source(
            RenderFrameSourceGeneration::editor(generation.0),
            frame_index,
            None,
            std::iter::empty(),
            views,
            Some(target),
            None,
        )?;
        if packet
            .as_ref()
            .is_some_and(|packet| matches!(packet.target(), RenderFrameTarget::Window(_)))
        {
            return Err(RenderFramePacketError::EditorPreviewRequiresImageTarget);
        }
        Ok(packet)
    }

    /// Creates the private image-target identity owned by the first-party Editor viewport.
    #[doc(hidden)]
    pub fn editor_viewport_image_target(
        image: Handle<RenderImage2d>,
        extent: Extent2d,
    ) -> Result<RenderImagePacket, RenderFramePacketError> {
        if extent.is_empty() {
            return Err(RenderFramePacketError::ImageTargetHasZeroExtent);
        }
        Ok(RenderImagePacket { image, extent })
    }

    /// Maps one managed Runtime logical-window packet into the private Editor image target.
    ///
    /// The extracted view deliberately retains its logical window target so sprite/UI lowering
    /// continues to use the Runtime's existing target semantics. Only the backend execution target
    /// becomes the private image owned by the Editor.
    pub fn map_editor_game_view_packet(
        mut packet: RenderFramePacket,
        target: RenderImagePacket,
    ) -> Result<RenderFramePacket, RenderFramePacketError> {
        let RenderFrameTarget::Window(window) = packet.target else {
            return Err(RenderFramePacketError::EditorGameViewRequiresWindowTarget);
        };
        let logical_extent = Extent2d::new(
            window.resolution.physical_width,
            window.resolution.physical_height,
        )
        .ok_or(RenderFramePacketError::ImageTargetHasZeroExtent)?;
        if target.extent != logical_extent {
            return Err(RenderFramePacketError::EditorGameViewTargetExtentMismatch {
                logical: logical_extent,
                image: target.extent,
            });
        }
        packet.target = RenderFrameTarget::Image(target);
        Ok(packet)
    }

    /// Assembles the closed first-party draw order without exposing topology construction.
    pub fn assemble_first_party_draws<T>(mut content: Vec<T>, ui: Vec<T>) -> Vec<T> {
        content.extend(ui);
        content
    }

    /// Provisional first-party ordering boundary after camera extraction completes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
    pub enum RenderExtractSet {
        Views,
    }
}

#[derive(Debug, Default, Resource)]
struct ExtractedViewScratch(Vec<ExtractedView>);

/// Failure to publish the world-space camera projection for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RenderViewExtractionError {
    #[error("the completed 2D transform projection is unavailable")]
    ProjectionIncomplete,
    #[error("camera entity {entity:?} is missing its authored Transform2d")]
    MissingLocalTransform { entity: Entity },
    #[error("camera entity {entity:?} is missing its completed GlobalTransform2d")]
    MissingGlobalTransform { entity: Entity },
    #[error("camera entity {entity:?} has a non-finite completed transform")]
    NonFiniteTransform { entity: Entity },
    #[error("camera entity {entity:?} has a singular completed linear transform")]
    SingularTransform { entity: Entity },
    #[error("camera entity {entity:?} has an invalid orthographic projection")]
    InvalidProjection { entity: Entity },
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
    ContentRejected,
    Deferred,
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
pub struct RenderComponentsPlugin;

pub const RENDER_COMPONENTS_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-components");
pub const RENDER_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.render.components");
pub const RENDER_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.render.components");
pub const RENDER_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        RENDER_SCHEMA_OWNER_ID,
        RENDER_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.render.components.native", 1),
        render_schema_catalog,
        validate_render_components,
        register_render_components,
    );
const RENDER_COMPONENTS_PLUGIN_PRODUCT_ALTERNATIVES: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("runtime-2d"),
    nara_app::PluginProductCapability::new("presentation-ui"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
pub const RENDER_COMPONENTS_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        RENDER_COMPONENTS_PLUGIN_ID,
        nara_app::PluginCategory::Runtime,
    )
    .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
    .requires_any_product_capabilities(RENDER_COMPONENTS_PLUGIN_PRODUCT_ALTERNATIVES)
    .provides_schema(&[RENDER_SCHEMA_PROVIDER_ID]);

impl Plugin for RenderComponentsPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &RENDER_COMPONENTS_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.render.Camera2d");
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            RENDER_COMPONENTS_PLUGIN_ID,
            component_id.as_str(),
        )?;
        RENDER_SCHEMA_PROVIDER.preflight(registry).map_err(|error| {
            PluginError::component_registration(
                RENDER_COMPONENTS_PLUGIN_ID,
                component_id.as_str(),
                error,
            )
        })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.render.Camera2d");
        nara_reflect::register_schema_provider_for_plugin(
            app,
            RENDER_COMPONENTS_PLUGIN_ID,
            component_id.as_str(),
            &RENDER_SCHEMA_PROVIDER,
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RenderPlugin {
    pub world_canvas: Option<WorldCanvas>,
}

/// Creates a repeatable Render plugin with an optional fixed world canvas.
#[must_use]
pub fn plugin(world_canvas: Option<WorldCanvas>) -> nara_app::PluginDefinition {
    let mut bytes = b"nara.render.plugin-config.v1\0".to_vec();
    match world_canvas {
        None => bytes.push(0),
        Some(canvas) => {
            bytes.push(1);
            bytes.extend_from_slice(&canvas.extent().width.to_le_bytes());
            bytes.extend_from_slice(&canvas.extent().height.to_le_bytes());
            let color = canvas.bar_color();
            for value in [color.r, color.g, color.b, color.a] {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
    }
    nara_app::PluginDefinition::infallible::<RenderPlugin, _>(
        nara_app::PluginDefinitionId::new("nara.render.configured", 1),
        bytes,
        move || RenderPlugin { world_canvas },
    )
}

pub const RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.render");
const RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    RENDER_COMPONENTS_PLUGIN_ID,
    nara_transform::TRANSFORM_PLUGIN_ID,
];
const RENDER_PLUGIN_PRODUCT_ALTERNATIVES: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-2d"),
    nara_app::PluginProductCapability::new("presentation-ui"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
pub const RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(RENDER_PLUGIN_REQUIREMENTS)
        .requires_any_product_capabilities(RENDER_PLUGIN_PRODUCT_ALTERNATIVES);

impl Plugin for RenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara_transform::__private::install_render_projection(app)?;
        if let Some(canvas) = self.world_canvas {
            app.insert_resource(canvas)?;
        }
        app.init_resource::<ClearColor>()?;
        app.init_resource::<ExtractedViews>()?;
        app.init_resource::<ExtractedViewScratch>()?;
        app.init_resource::<RenderFrame>()?;
        app.init_resource::<FrameStats>()?;
        app.init_resource::<RenderBackendStatus>()?;
        app.add_systems(
            CoreStage::First,
            world_pointer::invalidate_window_presentation
                .after(nara_window::WindowSet::ApplyEvents),
        )?;
        app.add_systems(
            CoreStage::Extract,
            extract_views
                .after(nara_transform::__private::TransformSet::Present)
                .in_set(__private::RenderExtractSet::Views),
        )?;
        app.add_systems(CoreStage::Render, begin_render_frame)?;
        Ok(())
    }
}

pub fn register_render_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_render_components(registry)?;
    registry.register_persistent_component_with_codec::<Camera2d, _, _>(
        camera_schema(),
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

fn render_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![camera_schema()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn camera_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.render.Camera2d"),
        "Camera 2D",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(camera_fields())
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

fn extract_views(
    mut frame: ResMut<RenderFrame>,
    mut extracted_views: ResMut<ExtractedViews>,
    mut scratch: ResMut<ExtractedViewScratch>,
    clear_color: Res<ClearColor>,
    completed_projection: Option<Res<nara_transform::__private::CompletedTransformProjection>>,
    completed_hierarchy: Option<Res<nara_hierarchy::__private::CompletedHierarchyProjection>>,
    presentation: Res<nara_transform::__private::RenderTransformProjection>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    canvas: Option<Res<WorldCanvas>>,
    cameras: Query<(
        Entity,
        &Camera2d,
        Option<&Transform2d>,
        Option<&GlobalTransform2d>,
    )>,
) -> Result<(), BevyError> {
    let completed_projection = completed_projection
        .ok_or(RenderViewExtractionError::ProjectionIncomplete)
        .map_err(BevyError::error)?;
    let completed_hierarchy = completed_hierarchy
        .ok_or(RenderViewExtractionError::ProjectionIncomplete)
        .map_err(BevyError::error)?;
    if completed_projection.hierarchy_generation() != completed_hierarchy.generation() {
        return Err(BevyError::error(
            RenderViewExtractionError::ProjectionIncomplete,
        ));
    }
    scratch.0.clear();

    let primary_window_id = primary_window_id.map(|resource| resource.0);

    for (camera_entity, camera, local, global) in cameras.iter() {
        local
            .ok_or(RenderViewExtractionError::MissingLocalTransform {
                entity: camera_entity,
            })
            .map_err(BevyError::error)?;
        let global = global
            .ok_or(RenderViewExtractionError::MissingGlobalTransform {
                entity: camera_entity,
            })
            .map_err(BevyError::error)?;
        let Some(viewport) = camera.viewport.or_else(|| {
            viewport_for_target(
                camera.target,
                primary_window_id,
                &windows,
                canvas.as_deref().copied(),
            )
        }) else {
            continue;
        };

        let matrix = presentation
            .matrix(camera_entity, global.matrix())
            .ok_or(RenderViewExtractionError::ProjectionIncomplete)
            .map_err(BevyError::error)?;
        let mapping = camera_mapping(camera_entity, matrix, camera.viewport_height, viewport)
            .map_err(BevyError::error)?;
        scratch.0.push(ExtractedView {
            camera_entity,
            target: camera.target,
            viewport,
            mapping,
            order: camera.order,
            load: camera
                .clear_color
                .map_or(ViewLoadOp::Load, ViewLoadOp::Clear),
        });
    }

    scratch
        .0
        .sort_unstable_by_key(|view| (view.order, view.camera_entity.to_bits()));
    for index in 0..scratch.0.len() {
        let (previous, remaining) = scratch.0.split_at_mut(index);
        let view = &mut remaining[0];
        if view.load == ViewLoadOp::Load
            && !previous.iter().any(|previous| {
                frame::targets_match(previous.target, view.target, primary_window_id)
            })
        {
            view.load = ViewLoadOp::Clear(clear_color.0);
        }
    }
    extracted_views.replace(&mut scratch.0, canvas.as_deref().copied());
    frame.begin_extract();
    Ok(())
}

fn camera_mapping(
    entity: Entity,
    matrix: nara_core::Mat3,
    viewport_height: f32,
    viewport: ViewportRect,
) -> Result<ViewMapping2d, RenderViewExtractionError> {
    if !matrix.is_finite() {
        return Err(RenderViewExtractionError::NonFiniteTransform { entity });
    }

    if matrix.determinant() == 0.0 {
        return Err(RenderViewExtractionError::SingularTransform { entity });
    }
    ViewMapping2d::orthographic(matrix, viewport_height, viewport)
        .ok_or(RenderViewExtractionError::InvalidProjection { entity })
}

pub fn begin_render_frame(mut frame: ResMut<RenderFrame>) {
    frame.begin_render();
}

fn viewport_for_target(
    target: RenderTarget,
    primary_window_id: Option<WindowId>,
    windows: &Query<&Window>,
    canvas: Option<WorldCanvas>,
) -> Option<ViewportRect> {
    let window_id = target.window_id(primary_window_id)?;
    windows
        .iter()
        .find(|window| window.id == window_id)
        .filter(|window| !window.resolution.is_zero_sized())
        .and_then(|window| match canvas {
            Some(canvas) => ViewportRect::from_extent(canvas.extent()),
            None => ViewportRect::from_window_resolution(window.resolution),
        })
}

#[cfg(test)]
mod tests {
    mod world_canvas;
    use super::*;
    use std::time::Duration;

    use nara_app::{
        App, CoreStage, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
        RuntimeClosePolicy, RuntimeInstance, RuntimeObligationLedger,
    };
    use nara_ecs::{Entity, Resource, World, schedule::IntoScheduleConfigs};
    use nara_hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
        HierarchyWriter,
    };
    use nara_reflect::ComponentRegistryPlugin;
    use nara_transform::TransformPlugin;
    use nara_window::WindowPlugin;

    fn render_app(with_window: bool) -> App {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            HierarchyPlugin,
            TransformPlugin,
            RenderComponentsPlugin,
        ))
        .unwrap();
        if with_window {
            app.add_plugin(WindowPlugin::default()).unwrap();
        }
        app.add_plugin(RenderPlugin::default()).unwrap();
        app
    }

    fn place_parented(world: &mut World, child: Entity, parent: Entity) {
        HierarchyWriter::new(world)
            .apply_batch(&[
                HierarchyOperation::Place {
                    entity: parent,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                },
                HierarchyOperation::Place {
                    entity: child,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
                },
            ])
            .expect("the ordered hierarchy should be valid");
    }

    fn start_runtime(app: App) -> RuntimeInstance {
        RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap()
            .complete_startup()
            .unwrap()
            .promote()
    }

    fn retire_runtime(runtime: RuntimeInstance) {
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
    }

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

        let packet =
            build_render_frame_packet(generation, 9, Some(WindowId::PRIMARY), [&window], &views)
                .unwrap()
                .unwrap();

        assert_eq!(
            packet.source_generation().runtime_generation(),
            Some(generation)
        );
        assert_eq!(packet.frame_index(), 9);
        assert!(matches!(
            packet.target(),
            RenderFrameTarget::Window(window) if window.id == WindowId::PRIMARY
        ));
        assert_eq!(packet.views().first().unwrap(), &test_extracted_view());
    }

    #[test]
    fn frame_packet_admits_two_ordered_split_views_on_one_window_target() {
        let mut app = render_app(true);
        let world = app.world_mut().unwrap();
        let right = world
            .spawn((
                Camera2d {
                    viewport: Some(ViewportRect::new(640, 0, 640, 720).unwrap()),
                    order: 10,
                    ..Camera2d::default()
                },
                Transform2d::default(),
            ))
            .id();
        let left = world
            .spawn((
                Camera2d {
                    viewport: Some(ViewportRect::new(0, 0, 640, 720).unwrap()),
                    order: 0,
                    ..Camera2d::default()
                },
                Transform2d::default(),
            ))
            .id();
        let mut runtime = start_runtime(app);
        runtime.drive(Duration::ZERO).unwrap();
        let views = runtime.world().resource::<ExtractedViews>();
        let ordered_cameras = views
            .as_slice()
            .iter()
            .map(|view| view.camera_entity)
            .collect::<Vec<_>>();
        let window = Window::default();
        let captured = build_render_frame_packet(
            runtime.generation(),
            runtime.world().resource::<RenderFrame>().index,
            Some(WindowId::PRIMARY),
            [&window],
            views,
        );
        retire_runtime(runtime);

        assert_eq!(ordered_cameras, [left, right]);
        let packet = captured
            .expect("two ordered split views on one target must be admitted")
            .expect("two extracted views must produce a render frame packet");
        assert!(matches!(
            packet.target(),
            RenderFrameTarget::Window(window) if window.id == WindowId::PRIMARY
        ));
    }

    #[test]
    fn editor_game_view_mapping_changes_only_the_execution_target() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(test_extracted_view());
        let packet =
            build_render_frame_packet(generation, 9, Some(WindowId::PRIMARY), [&window], &views)
                .unwrap()
                .unwrap();
        let target = RenderImagePacket {
            image: nara_asset::allocate_anonymous_handle().unwrap(),
            extent: Extent2d {
                width: 1280,
                height: 720,
            },
        };

        let mapped = __private::map_editor_game_view_packet(packet, target).unwrap();

        assert_eq!(mapped.target(), RenderFrameTarget::Image(target));
        assert_eq!(
            mapped.views().first().unwrap().target,
            RenderTarget::PrimaryWindow
        );
        assert_eq!(
            mapped.views().first().unwrap().viewport,
            test_extracted_view().viewport
        );

        let packet =
            build_render_frame_packet(generation, 10, Some(WindowId::PRIMARY), [&window], &views)
                .unwrap()
                .unwrap();
        assert!(matches!(
            __private::map_editor_game_view_packet(
                packet,
                RenderImagePacket {
                    image: target.image,
                    extent: Extent2d {
                        width: 640,
                        height: 360,
                    },
                },
            ),
            Err(RenderFramePacketError::EditorGameViewTargetExtentMismatch { .. })
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
            build_render_frame_packet(generation, 1, None, [&window], &mismatched),
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
            build_render_frame_packet(generation, 1, None, [&window], &outside),
            Err(RenderFramePacketError::ViewportOutsideTarget)
        );
    }

    #[test]
    fn captured_packet_retains_order_and_clear_once_load_next() {
        let mut app = render_app(true);
        for x in [0, 640] {
            app.world_mut().unwrap().spawn((
                Camera2d {
                    viewport: Some(ViewportRect::new(x, 0, 640, 720).unwrap()),
                    ..Default::default()
                },
                Transform2d::default(),
            ));
        }
        let mut runtime = start_runtime(app);
        runtime.drive(Duration::ZERO).unwrap();
        let window = Window::default();
        let mut first_views = runtime.world().resource::<ExtractedViews>().clone();
        let captured = build_render_frame_packet(
            runtime.generation(),
            runtime.world().resource::<RenderFrame>().index,
            None,
            [&window],
            &first_views,
        );
        runtime.drive(Duration::ZERO).unwrap();
        let next_views = runtime.world().resource::<ExtractedViews>().clone();
        let next = build_render_frame_packet(
            runtime.generation(),
            runtime.world().resource::<RenderFrame>().index,
            None,
            [&window],
            &next_views,
        );
        first_views.clear();
        retire_runtime(runtime);

        let packet = captured.unwrap().unwrap();
        let next = next.unwrap().unwrap();
        assert!(first_views.is_empty());
        assert_eq!(packet.views().len(), 2);
        assert_eq!(packet.views(), next_views.as_slice());
        assert_eq!(next.views(), packet.views());
        assert_eq!(packet.views()[0].load, ViewLoadOp::Clear(Color::BLACK));
        assert_eq!(packet.views()[1].load, ViewLoadOp::Load);
    }

    #[test]
    fn split_image_uses_explicit_physical_extent_and_rejects_mixed_targets() {
        let generation = test_runtime_generation();
        let image = nara_asset::allocate_anonymous_handle().unwrap();
        let target =
            __private::editor_viewport_image_target(image, Extent2d::new(320, 180).unwrap())
                .unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            target: RenderTarget::Image(image),
            viewport: ViewportRect::new(0, 0, 160, 180).unwrap(),
            ..test_extracted_view()
        });
        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(2).unwrap(),
            target: RenderTarget::Image(image),
            viewport: ViewportRect::new(160, 0, 160, 180).unwrap(),
            order: 1,
            load: ViewLoadOp::Load,
            ..test_extracted_view()
        });
        let packet = __private::build_editor_render_frame_packet(
            __private::EditorRenderGeneration::try_acquire().unwrap(),
            1,
            &views,
            target,
        )
        .unwrap()
        .unwrap();
        assert_eq!(packet.target(), RenderFrameTarget::Image(target));
        assert_eq!(packet.views(), views.as_slice());
        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(3).unwrap(),
            ..test_extracted_view()
        });
        assert_eq!(
            build_render_frame_packet(generation, 2, None, [&Window::default()], &views),
            Err(RenderFramePacketError::MixedRenderTargets)
        );
    }

    #[test]
    fn packet_view_limit_order_and_initialization_are_explicit() {
        let generation = test_runtime_generation();
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            load: ViewLoadOp::Load,
            ..test_extracted_view()
        });
        assert_eq!(
            build_render_frame_packet(generation, 1, None, [&window], &views),
            Err(RenderFramePacketError::UninitializedTarget)
        );
        views.clear();
        for index in 0..=MAX_VIEWS_PER_TARGET {
            views.push(ExtractedView {
                camera_entity: Entity::from_raw_u32(index as u32 + 1).unwrap(),
                ..test_extracted_view()
            });
        }
        assert_eq!(
            build_render_frame_packet(generation, 1, None, [&window], &views),
            Err(RenderFramePacketError::TooManyViews {
                actual: MAX_VIEWS_PER_TARGET + 1,
                maximum: MAX_VIEWS_PER_TARGET
            })
        );
        views.clear();
        views.push(ExtractedView {
            order: 1,
            ..test_extracted_view()
        });
        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(2).unwrap(),
            ..test_extracted_view()
        });
        assert_eq!(
            build_render_frame_packet(generation, 1, None, [&window], &views),
            Err(RenderFramePacketError::UnorderedViews)
        );
    }

    #[test]
    fn parented_affine_camera_shares_drawing_and_picking_mapping() {
        let mut app = render_app(false);
        let world = app.world_mut().unwrap();
        let parent = world
            .spawn(Transform2d {
                translation: Vec2::new(4.0, -3.0),
                scale: Vec2::new(2.0, 1.0),
                ..Default::default()
            })
            .id();
        let camera = world
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
                    viewport: Some(ViewportRect::new(40, 20, 320, 180).unwrap()),
                    viewport_height: 180.0,
                    ..Default::default()
                },
                Transform2d {
                    rotation: std::f32::consts::FRAC_PI_4,
                    ..Default::default()
                },
            ))
            .id();
        place_parented(world, camera, parent);
        let mut runtime = start_runtime(app);
        runtime.drive(Duration::ZERO).unwrap();
        let view = runtime.world().resource::<ExtractedViews>().as_slice()[0];
        let camera_matrix = runtime
            .world()
            .get::<GlobalTransform2d>(camera)
            .unwrap()
            .matrix();
        let world_point = camera_matrix.transform_point2(Vec2::new(80.0, 45.0));
        let physical = view.world_to_viewport(world_point);
        let restored = view.viewport_to_world(physical).unwrap();
        retire_runtime(runtime);
        assert!(physical.abs_diff_eq(Vec2::new(280.0, 65.0), 1.0e-4));
        assert!(restored.abs_diff_eq(world_point, 1.0e-4));
    }

    #[test]
    fn singular_camera_faults_without_partial_publication() {
        let mut app = render_app(false);
        let prior = test_extracted_view();
        app.world_mut()
            .unwrap()
            .resource_mut::<ExtractedViews>()
            .push(prior);
        app.world_mut().unwrap().spawn((
            Camera2d {
                target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
                ..Default::default()
            },
            Transform2d {
                scale: Vec2::ZERO,
                ..Default::default()
            },
        ));
        let mut runtime = start_runtime(app);
        let result = runtime.drive(Duration::ZERO);
        let retained = runtime.world().resource::<ExtractedViews>().clone();
        retire_runtime(runtime);
        assert!(result.is_err());
        assert_eq!(retained.as_slice(), &[prior]);
    }

    #[test]
    fn extracts_primary_window_camera_view() {
        let mut app = render_app(true);
        app.world_mut()
            .expect("app should allow world mutation")
            .spawn((Camera2d::default(), Transform2d::default()));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let views = runtime.world().resource::<ExtractedViews>();
        assert_eq!(views.len(), 1);
        assert_eq!(views.as_slice()[0].target, RenderTarget::PrimaryWindow);
        assert_eq!(
            views.as_slice()[0].mapping.clip_to_world(Vec2::ZERO),
            Vec2::ZERO
        );
        assert!(
            views.as_slice()[0]
                .mapping
                .world_to_clip(Vec2::new(0.0, 360.0))
                .abs_diff_eq(Vec2::Y, 1.0e-5)
        );
        assert_eq!(
            views.as_slice()[0].viewport,
            ViewportRect::new(0, 0, 1280, 720).unwrap()
        );
        assert_eq!(views.as_slice()[0].load, ViewLoadOp::Clear(Color::BLACK));
        runtime.finish().unwrap();
    }

    fn test_extracted_view() -> ExtractedView {
        ExtractedView {
            camera_entity: Entity::from_raw_u32(1).unwrap(),
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 1280, 720).unwrap(),
            mapping: ViewMapping2d::orthographic(
                nara_core::Mat3::IDENTITY,
                720.0,
                ViewportRect::new(0, 0, 1280, 720).unwrap(),
            )
            .unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        }
    }

    fn test_runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                App::new().seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
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
        let mut app = render_app(false);
        app.world_mut()
            .expect("app should allow world mutation")
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
                    viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
                    clear_color: Some(Color::WHITE),
                    ..Camera2d::default()
                },
                Transform2d::default(),
            ));

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let views = runtime.world().resource::<ExtractedViews>();
        assert_eq!(views.len(), 1);
        assert_eq!(
            views.as_slice()[0].viewport,
            ViewportRect::new(0, 0, 320, 180).unwrap()
        );
        assert_eq!(views.as_slice()[0].load, ViewLoadOp::Clear(Color::WHITE));
        runtime.finish().unwrap();
    }

    #[test]
    fn extraction_clears_stale_views_when_camera_or_window_is_missing() {
        let mut app = render_app(false);
        app.world_mut()
            .expect("app should allow world mutation")
            .resource_mut::<ExtractedViews>()
            .push(ExtractedView {
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::PrimaryWindow,
                viewport: ViewportRect::new(0, 0, 1, 1).unwrap(),
                mapping: ViewMapping2d::orthographic(
                    nara_core::Mat3::IDENTITY,
                    1.0,
                    ViewportRect::new(0, 0, 1, 1).unwrap(),
                )
                .unwrap(),
                order: 0,
                load: ViewLoadOp::Clear(Color::WHITE),
            });

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        assert!(runtime.world().resource::<ExtractedViews>().is_empty());
        runtime.finish().unwrap();
    }

    #[test]
    fn parented_camera_extracts_completed_world_translation() {
        let mut app = render_app(false);
        let world = app.world_mut().unwrap();
        let root = world
            .spawn(Transform2d::from_translation(Vec2::new(100.0, 40.0)))
            .id();
        let camera = world
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
                    viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
                    ..Camera2d::default()
                },
                Transform2d::from_translation(Vec2::new(10.0, -5.0)),
            ))
            .id();
        place_parented(world, camera, root);

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();

        let views = runtime.world().resource::<ExtractedViews>();
        assert_eq!(views.len(), 1);
        assert_eq!(
            views.as_slice()[0].mapping.clip_to_world(Vec2::ZERO),
            Vec2::new(110.0, 35.0)
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn missing_camera_transform_faults_without_replacing_prior_views() {
        let mut app = render_app(false);
        let prior = test_extracted_view();
        app.world_mut()
            .unwrap()
            .resource_mut::<ExtractedViews>()
            .push(prior);
        app.world_mut().unwrap().spawn(Camera2d {
            target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
            viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
            ..Camera2d::default()
        });

        let mut runtime = start_runtime(app);
        runtime
            .drive(Duration::ZERO)
            .expect_err("world-space cameras require explicit local and completed transforms");

        assert_eq!(
            runtime.world().resource::<ExtractedViews>().as_slice(),
            &[prior]
        );
        retire_runtime(runtime);
    }

    #[test]
    fn missing_camera_global_transform_faults_without_replacing_prior_views() {
        let mut app = render_app(false);
        let prior = test_extracted_view();
        app.world_mut()
            .unwrap()
            .resource_mut::<ExtractedViews>()
            .push(prior);
        let camera = app
            .world_mut()
            .unwrap()
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(nara_asset::allocate_anonymous_handle().unwrap()),
                    viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
                    ..Camera2d::default()
                },
                Transform2d::default(),
            ))
            .id();
        app.insert_resource(RemoveCameraGlobal(camera)).unwrap();
        app.add_systems(
            CoreStage::Extract,
            remove_camera_global
                .after(nara_transform::__private::TransformSet::Propagate)
                .before(__private::RenderExtractSet::Views),
        )
        .unwrap();

        let mut runtime = start_runtime(app);
        runtime
            .drive(Duration::ZERO)
            .expect_err("a camera without its completed global projection must fault");

        assert_eq!(
            runtime.world().resource::<ExtractedViews>().as_slice(),
            &[prior]
        );
        retire_runtime(runtime);
    }

    #[derive(Resource)]
    struct RemoveCameraGlobal(Entity);

    fn remove_camera_global(world: &mut World) {
        let camera = world.resource::<RemoveCameraGlobal>().0;
        world.entity_mut(camera).remove::<GlobalTransform2d>();
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
            .map(|field| (field.path.to_string(), field.value_kind(), field.required))
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
