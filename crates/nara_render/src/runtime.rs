use nara_app::{
    App, CoreStage, Plugin, PluginError, RealTime, RenderTime, RuntimeTimeSettings, VirtualTime,
};
use nara_core::Mat3;
use nara_ecs::{
    DetectChanges, Entity, FromWorld, Mut, Query, QueryState, Ref, Res, ResMut, Resource,
    SystemParam, SystemSet, World, bevy_ecs::entity::EntityHashMap,
    bevy_ecs::entity_disabling::DefaultQueryFilters, bevy_ecs::error::BevyError,
    bevy_ecs::system::SystemState, schedule::IntoScheduleConfigs,
};
use nara_scene::spatial::{
    GlobalTransform2d, InheritedVisibility, SPATIAL_PLUGIN_ID, propagate_spatial,
};
use nara_window::{PrimaryWindowId, Window, WindowId};

use crate::{
    Camera2d, CapturedRenderTime, Color, Extent2d, ExtractedView, ExtractedViews, RenderViewId,
    ViewLoadOp, ViewMapping2d, ViewportRect, WorldCanvas, frame, world_pointer,
};

/// Frame capture and presentation hooks for desktop and other renderer hosts.
pub mod runtime_integration {
    pub use super::RenderExtractSet;
    pub use crate::frame::build_window_overlay_frame_packet;
    pub use crate::world_pointer::{clear_presented_world_frame, publish_presented_world_frame};
}

/// Ordering for systems that consume camera views in [`CoreStage::Extract`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum RenderExtractSet {
    Views,
}

#[derive(Debug, Clone, Copy, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClearColor(pub Color);

impl Default for ClearColor {
    fn default() -> Self {
        Self(Color::BLACK)
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
    if let Some(canvas) = world_canvas {
        bytes.push(1);
        bytes.extend_from_slice(&canvas.extent().width.to_le_bytes());
        bytes.extend_from_slice(&canvas.extent().height.to_le_bytes());
        let color = canvas.bar_color();
        for value in [color.r, color.g, color.b, color.a] {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
    } else {
        bytes.push(0);
    }
    nara_app::PluginDefinition::infallible::<RenderPlugin, _>(
        nara_app::PluginDefinitionId::new("nara.render.configured", 1),
        bytes,
        move || RenderPlugin { world_canvas },
    )
}

pub const RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.render");
pub const RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(&[SPATIAL_PLUGIN_ID]);

impl Plugin for RenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if let Some(canvas) = self.world_canvas {
            app.insert_resource(canvas)?;
        }
        app.init_resource::<ClearColor>()?;
        app.init_resource::<ExtractedViews>()?;
        app.init_resource::<ExtractedViewScratch>()?;
        app.init_resource::<CameraViewLifetimes>()?;
        app.add_systems(
            CoreStage::First,
            world_pointer::invalidate_window_presentation
                .after(nara_window::WindowSet::ApplyEvents),
        )?;
        app.add_systems(
            CoreStage::Extract,
            (
                propagate_spatial,
                extract_views.in_set(RenderExtractSet::Views),
            )
                .chain(),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct ExtractedViewScratch(Vec<ExtractedView>);

/// Camera membership includes hidden and disabled entities, even when they yield no view.
#[derive(Debug, Resource)]
struct CameraViewLifetimes {
    ids: EntityHashMap<RenderViewId>,
    cameras: QueryState<(Entity, Ref<'static, Camera2d>)>,
}

impl FromWorld for CameraViewLifetimes {
    fn from_world(world: &mut World) -> Self {
        let cameras = world
            .try_resource_scope(|world, _: Mut<DefaultQueryFilters>| world.query())
            .unwrap_or_else(|| world.query());
        Self {
            ids: EntityHashMap::default(),
            cameras,
        }
    }
}

impl CameraViewLifetimes {
    fn observe(&mut self, world: &World) -> Result<(), RenderViewExtractionError> {
        let cameras = self.cameras.query(world);
        self.ids.retain(|entity, _| {
            cameras
                .get(*entity)
                .is_ok_and(|(_, camera)| !camera.is_added())
        });
        for (entity, _) in cameras.iter() {
            if !self.ids.contains_key(&entity) {
                let id = RenderViewId::try_acquire()
                    .ok_or(RenderViewExtractionError::IdentityUnavailable { entity })?;
                self.ids.insert(entity, id);
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum RenderViewExtractionError {
    #[error("camera entity {entity:?} has no available view identity")]
    IdentityUnavailable { entity: Entity },
    #[error("camera entity {entity:?} has no GlobalTransform2d")]
    MissingGlobalTransform { entity: Entity },
    #[error("camera entity {entity:?} has a non-finite completed transform")]
    NonFiniteTransform { entity: Entity },
    #[error("camera entity {entity:?} has a singular completed linear transform")]
    SingularTransform { entity: Entity },
    #[error("camera entity {entity:?} has an invalid orthographic projection")]
    InvalidProjection { entity: Entity },
}

#[derive(SystemParam)]
struct ViewExtraction<'w, 's> {
    extracted_views: ResMut<'w, ExtractedViews>,
    scratch: ResMut<'w, ExtractedViewScratch>,
    lifetimes: Res<'w, CameraViewLifetimes>,
    clear_color: Res<'w, ClearColor>,
    real_time: Res<'w, RealTime>,
    virtual_time: Res<'w, VirtualTime>,
    render_time: Res<'w, RenderTime>,
    time_settings: Res<'w, RuntimeTimeSettings>,
    primary_window_id: Option<Res<'w, PrimaryWindowId>>,
    windows: Query<'w, 's, &'static Window>,
    canvas: Option<Res<'w, WorldCanvas>>,
    cameras: Query<
        'w,
        's,
        (
            Entity,
            &'static Camera2d,
            Option<&'static GlobalTransform2d>,
            Option<&'static InheritedVisibility>,
        ),
    >,
}

fn extract_views(
    world: &mut World,
    state: &mut SystemState<ViewExtraction<'static, 'static>>,
) -> Result<(), BevyError> {
    world
        .resource_scope(|world, mut lifetimes: Mut<CameraViewLifetimes>| lifetimes.observe(world))
        .map_err(BevyError::error)?;
    let ViewExtraction {
        mut extracted_views,
        mut scratch,
        lifetimes,
        clear_color,
        real_time,
        virtual_time,
        render_time,
        time_settings,
        primary_window_id,
        windows,
        canvas,
        cameras,
    } = state.get_mut(world).map_err(BevyError::error)?;
    let canvas = canvas.as_deref().copied();
    let primary_window_id = primary_window_id.map(|resource| resource.0);
    scratch.0.clear();

    for (entity, camera, global, visibility) in &cameras {
        if visibility.is_some_and(|visibility| !visibility.get()) {
            continue;
        }
        let global = global
            .ok_or(RenderViewExtractionError::MissingGlobalTransform { entity })
            .map_err(BevyError::error)?;
        let id = lifetimes
            .ids
            .get(&entity)
            .copied()
            .ok_or(RenderViewExtractionError::IdentityUnavailable { entity })
            .map_err(BevyError::error)?;
        let Some(viewport) = viewport_for_camera(camera, primary_window_id, &windows, canvas)
        else {
            continue;
        };
        scratch.0.push(ExtractedView {
            id,
            camera_entity: entity,
            target: camera.target,
            viewport,
            mapping: camera_mapping(entity, global.matrix(), camera.viewport_height, viewport)
                .map_err(BevyError::error)?,
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
    let time = CapturedRenderTime::new(*real_time, *virtual_time, *render_time, *time_settings);
    extracted_views.replace(&mut scratch.0, canvas, time);
    Ok(())
}

fn camera_mapping(
    entity: Entity,
    matrix: Mat3,
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

fn viewport_for_camera(
    camera: &Camera2d,
    primary_window_id: Option<WindowId>,
    windows: &Query<&Window>,
    canvas: Option<WorldCanvas>,
) -> Option<ViewportRect> {
    let resolution = camera.target.window_id(primary_window_id).and_then(|id| {
        windows
            .iter()
            .find(|window| window.id == id)
            .map(|window| window.resolution)
    });
    let Some(resolution) = resolution else {
        return camera.viewport;
    };
    if resolution.is_zero_sized() {
        return None;
    }
    let extent = canvas.map_or(
        Extent2d {
            width: resolution.physical_width,
            height: resolution.physical_height,
        },
        WorldCanvas::extent,
    );
    let Some(viewport) = camera.viewport else {
        return ViewportRect::from_extent(extent);
    };
    ViewportRect::new(
        viewport.physical_x,
        viewport.physical_y,
        viewport
            .physical_width
            .min(extent.width.saturating_sub(viewport.physical_x)),
        viewport
            .physical_height
            .min(extent.height.saturating_sub(viewport.physical_y)),
    )
}

#[cfg(test)]
mod tests;
