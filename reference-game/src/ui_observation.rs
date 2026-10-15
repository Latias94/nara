use nara::{
    prelude::{Resource, World},
    scene::SceneEntitySource,
    ui::{UiButton, UiDimension, UiNode, UiSurfaceId, active_ui_surface_slots},
};

#[cfg(feature = "desktop")]
use std::sync::Arc;

#[cfg(feature = "desktop")]
use nara::{
    ui::advanced::{UiFrame, UiKey},
    ui::latest_ui_frame,
    ui_render::advanced::PresentedUiObservation,
};

#[cfg(feature = "desktop")]
use crate::REFERENCE_RETRY_ACTION;
use crate::{REFERENCE_UI_SURFACE_ID, WaveSnapshot, resources::WaveRunOwner};

/// Structured product facts projected from ordinary UI authoring components.
///
/// Headless and presented products share these facts. Backend-only evidence remains in the
/// optional presentation record instead of pretending that a headless run submitted pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceUiObservation {
    pub surface_id: u64,
    pub authored_width: f32,
    pub authored_height: f32,
    pub retry_entity_id: String,
    pub retry_source_document: String,
    pub retry_source_entity: String,
    pub retry_action: String,
    pub retry_name: String,
    pub retry_enabled: bool,
    pub retry_authored_width: f32,
    pub retry_authored_height: f32,
    presentation: Option<ReferenceUiPresentationObservation>,
}

/// Immutable compiled and physically mapped facts available only to presented products.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceUiPresentationObservation {
    pub backend_instance_id: u64,
    pub device_epoch: u64,
    pub app_frame_index: u64,
    pub render_transaction_frame_index: u64,
    pub ui_admission_id: u64,
    pub presentation_epoch: u64,
    pub frame_revision: u64,
    pub source_revision: u64,
    pub slot_generation: u64,
    pub route_revision: u64,
    pub geometry_generation: u64,
    pub presentation_revision: u64,
    pub theme_revision: u64,
    pub font_revision: u64,
    pub physical_width: f32,
    pub physical_height: f32,
    pub content_origin_x: f32,
    pub content_origin_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub platform_scale_factor: f32,
    pub content_scale_factor: f32,
    pub retry_rect: ReferenceUiRect,
    pub retry_physical_rect: ReferenceUiPhysicalRect,
    pub semantic_nodes: usize,
    pub paint_primitives: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceUiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceUiPhysicalRect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

/// Product-owned outcome captured at the gameplay fixed-step boundary.
#[derive(Clone, Debug, Default, PartialEq, Resource)]
pub struct ReferenceProductObservation {
    wave: WaveSnapshot,
    ui: Option<ReferenceUiObservation>,
}

impl ReferenceProductObservation {
    #[must_use]
    pub const fn new(wave: WaveSnapshot, ui: Option<ReferenceUiObservation>) -> Self {
        Self { wave, ui }
    }

    #[must_use]
    pub const fn wave(&self) -> &WaveSnapshot {
        &self.wave
    }

    #[must_use]
    pub const fn ui(&self) -> Option<&ReferenceUiObservation> {
        self.ui.as_ref()
    }
}

impl ReferenceUiObservation {
    /// Captures ordinary authored adapter facts without requiring a presentation backend.
    #[must_use]
    pub fn from_authoring_world(world: &World) -> Option<Self> {
        let surface = UiSurfaceId::new(REFERENCE_UI_SURFACE_ID)?;
        let slot = active_ui_surface_slots(world)
            .ok()?
            .into_iter()
            .find(|slot| slot.surface_id() == surface)?;
        let transform = slot.transform();
        let owner = world.get_resource::<WaveRunOwner>()?;
        let retry = world.iter_entities().find(|entity| {
            entity.get::<SceneEntitySource>().is_some_and(|source| {
                reference_ui_source_entity(source) == Some("retry-button")
                    && owner.owns_scene_entity(entity.id(), source)
            }) && entity.contains::<UiNode>()
                && entity.contains::<UiButton>()
        })?;
        let source = retry.get::<SceneEntitySource>()?;
        let source_step = source.projection_path.as_ref()?.source_chain().last()?;
        let node = retry.get::<UiNode>()?;
        let button = retry.get::<UiButton>()?;
        let (retry_authored_width, retry_authored_height) =
            match (node.style().width(), node.style().height()) {
                (UiDimension::Px(width), UiDimension::Px(height)) => (width, height),
                _ => return None,
            };
        let authored = transform.authored_logical_size();

        Some(Self {
            surface_id: surface.get(),
            authored_width: authored.width(),
            authored_height: authored.height(),
            retry_entity_id: source.entity_id.as_str().to_owned(),
            retry_source_document: source_step.source_document().to_string(),
            retry_source_entity: source.source_entity_id.as_str().to_owned(),
            retry_action: button.action().as_str().to_owned(),
            retry_name: button.label().to_owned(),
            retry_enabled: button.enabled(),
            retry_authored_width,
            retry_authored_height,
            presentation: None,
        })
    }

    /// Joins actual scene identity with the current owner-produced presented frame.
    #[must_use]
    #[cfg(feature = "desktop")]
    pub fn from_presented_observation(
        world: &World,
        presented: &PresentedUiObservation,
    ) -> Option<Self> {
        let frame = presented.frame();
        if frame.environment().surface_id().get() != REFERENCE_UI_SURFACE_ID {
            return None;
        }
        let current = latest_ui_frame(world, frame.environment().surface_id()).ok()??;
        if !Arc::ptr_eq(&current, &frame) {
            return None;
        }
        let authored = Self::from_authoring_world(world)?;
        let mut retry_nodes = frame.semantics().iter().filter(|node| {
            node.actions()
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        });
        let retry = retry_nodes.next()?;
        if retry_nodes.next().is_some() {
            return None;
        }
        let [action] = retry.actions() else {
            return None;
        };
        let transform = frame.environment().content_transform();
        let authored_size = transform.authored_logical_size();
        let retry_rect = retry.rect();
        let retry_origin = retry_rect.origin();
        let retry_size = retry_rect.size();
        let presentation = ReferenceUiPresentationObservation::from_frame(
            &frame,
            retry.key(),
            ReferenceUiRect {
                x: retry_origin.x(),
                y: retry_origin.y(),
                width: retry_size.width(),
                height: retry_size.height(),
            },
            presented,
        )?;

        Some(Self {
            surface_id: frame.environment().surface_id().get(),
            authored_width: authored_size.width(),
            authored_height: authored_size.height(),
            retry_entity_id: authored.retry_entity_id,
            retry_source_document: authored.retry_source_document,
            retry_source_entity: authored.retry_source_entity,
            retry_action: action.as_str().to_owned(),
            retry_name: retry.name().to_owned(),
            retry_enabled: retry.enabled(),
            retry_authored_width: retry_size.width(),
            retry_authored_height: retry_size.height(),
            presentation: Some(presentation),
        })
    }

    #[must_use]
    pub const fn presentation(&self) -> Option<&ReferenceUiPresentationObservation> {
        self.presentation.as_ref()
    }

    /// Emits one bounded, human-readable JSON record for package and desktop probes.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let presentation = self.presentation.as_ref().map_or_else(
            || "null".to_owned(),
            ReferenceUiPresentationObservation::to_json,
        );
        format!(
            concat!(
                "{{\"schema\":\"nara-reference-game.ui-observation-v2\",",
                "\"surface_id\":{},\"authored_size\":[{},{}],\"retry\":{{",
                "\"entity_id\":\"{}\",\"source_document\":\"{}\",",
                "\"source_entity\":\"{}\",\"action\":\"{}\",\"name\":\"{}\",",
                "\"enabled\":{},\"authored_size\":[{},{}]}},",
                "\"presentation\":{}}}\n"
            ),
            self.surface_id,
            self.authored_width,
            self.authored_height,
            escape_json(&self.retry_entity_id),
            escape_json(&self.retry_source_document),
            escape_json(&self.retry_source_entity),
            escape_json(&self.retry_action),
            escape_json(&self.retry_name),
            self.retry_enabled,
            self.retry_authored_width,
            self.retry_authored_height,
            presentation,
        )
    }
}

pub(crate) fn reference_ui_source_entity(source: &SceneEntitySource) -> Option<&str> {
    let projection = source.projection_path.as_ref()?;
    let source_step = projection.source_chain().last()?;
    (projection.root_anchor().as_str() == "reference-ui"
        && source_step
            .source_document()
            .as_path()
            .is_some_and(|path| path.as_str() == "reference-ui.prefab.json")
        && source_step.source_entity() == &source.source_entity_id)
        .then_some(source.source_entity_id.as_str())
}

impl ReferenceUiPresentationObservation {
    #[cfg(feature = "desktop")]
    fn from_frame(
        frame: &UiFrame,
        retry_key: &UiKey,
        retry_rect: ReferenceUiRect,
        presented: &PresentedUiObservation,
    ) -> Option<Self> {
        let layout = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(retry_key))?;
        let transform = frame.environment().content_transform();
        let physical_rect = layout.physical_rect();
        let receipt = presented.receipt();
        let submission = presented.submission();
        let backend_generation = presented.backend_generation();

        Some(Self {
            backend_instance_id: backend_generation.backend_instance_id(),
            device_epoch: backend_generation.device_epoch(),
            app_frame_index: presented.wgpu()?.app_frame_index(),
            render_transaction_frame_index: presented.wgpu()?.render_transaction_frame_index(),
            ui_admission_id: submission.admission_id().get(),
            presentation_epoch: receipt.presentation_epoch(),
            frame_revision: frame.revision().get(),
            source_revision: frame.source_revision().get(),
            slot_generation: frame.environment().slot_generation().get(),
            route_revision: frame.environment().route_revision().get(),
            geometry_generation: frame.environment().geometry_generation().get(),
            presentation_revision: frame.environment().presentation_revision().get(),
            theme_revision: frame.environment().theme_revision().get(),
            font_revision: frame.environment().font_revision().get(),
            physical_width: transform.physical_surface_size().width(),
            physical_height: transform.physical_surface_size().height(),
            content_origin_x: transform.physical_content_origin().x(),
            content_origin_y: transform.physical_content_origin().y(),
            content_width: transform.physical_content_size().width(),
            content_height: transform.physical_content_size().height(),
            platform_scale_factor: transform.platform_scale_factor(),
            content_scale_factor: transform.content_scale_factor(),
            retry_rect,
            retry_physical_rect: ReferenceUiPhysicalRect {
                min_x: physical_rect.min_x(),
                min_y: physical_rect.min_y(),
                max_x: physical_rect.max_x(),
                max_y: physical_rect.max_y(),
            },
            semantic_nodes: frame.semantics().len(),
            paint_primitives: frame.paint().len(),
        })
    }

    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"backend_instance_id\":{},\"device_epoch\":{},",
                "\"app_frame_index\":{},\"render_transaction_frame_index\":{},",
                "\"ui_admission_id\":{},\"presentation_epoch\":{},",
                "\"frame_revision\":{},\"source_revision\":{},",
                "\"slot_generation\":{},\"route_revision\":{},",
                "\"geometry_generation\":{},\"presentation_revision\":{},",
                "\"theme_revision\":{},\"font_revision\":{},",
                "\"physical_size\":[{},{}],\"content_origin\":[{},{}],",
                "\"content_size\":[{},{}],\"platform_scale_factor\":{},",
                "\"content_scale_factor\":{},\"retry_rect\":[{},{},{},{}],",
                "\"retry_physical_rect\":[{},{},{},{}],",
                "\"semantic_nodes\":{},\"paint_primitives\":{}}}"
            ),
            self.backend_instance_id,
            self.device_epoch,
            self.app_frame_index,
            self.render_transaction_frame_index,
            self.ui_admission_id,
            self.presentation_epoch,
            self.frame_revision,
            self.source_revision,
            self.slot_generation,
            self.route_revision,
            self.geometry_generation,
            self.presentation_revision,
            self.theme_revision,
            self.font_revision,
            self.physical_width,
            self.physical_height,
            self.content_origin_x,
            self.content_origin_y,
            self.content_width,
            self.content_height,
            self.platform_scale_factor,
            self.content_scale_factor,
            self.retry_rect.x,
            self.retry_rect.y,
            self.retry_rect.width,
            self.retry_rect.height,
            self.retry_physical_rect.min_x,
            self.retry_physical_rect.min_y,
            self.retry_physical_rect.max_x,
            self.retry_physical_rect.max_y,
            self.semantic_nodes,
            self.paint_primitives,
        )
    }
}

pub(crate) fn capture_reference_product_observation(world: &mut World) {
    let wave = world.resource::<WaveSnapshot>().clone();
    let ui = ReferenceUiObservation::from_authoring_world(world);
    *world.resource_mut::<ReferenceProductObservation>() = ReferenceProductObservation { wave, ui };
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                let _ = write!(escaped, "\\u{:04x}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped
}
