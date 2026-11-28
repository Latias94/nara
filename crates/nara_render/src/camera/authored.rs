use nara_app::{App, CoreStage, PluginError};
use nara_ecs::{
    Commands, Component, DetectChangesMut, Entity, Or, Query, With, schedule::IntoScheduleConfigs,
};
use nara_scene::components::Camera2dSource;

use crate::{Camera2d, runtime::CameraSpatial};

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    app.add_systems(CoreStage::Extract, hydrate_cameras.before(CameraSpatial))?;
    Ok(())
}

#[derive(Component)]
pub(super) struct HydratedCamera2d {
    source: Camera2dSource,
    created_camera: bool,
}

impl HydratedCamera2d {
    pub(super) fn is_active(&self) -> bool {
        self.source.active
    }
}

type CameraSources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Camera2dSource>,
        Option<&'static mut Camera2d>,
        Option<&'static mut HydratedCamera2d>,
    ),
    Or<(With<Camera2dSource>, With<HydratedCamera2d>)>,
>;

fn hydrate_cameras(mut commands: Commands, mut sources: CameraSources) {
    for (entity, source, camera, tracked) in &mut sources {
        let Some(&source) = source else {
            let mut entity = commands.entity(entity);
            if tracked.is_some_and(|tracked| tracked.created_camera) {
                entity.try_remove::<Camera2d>();
            }
            entity.try_remove::<HydratedCamera2d>();
            continue;
        };

        let previous = tracked.as_ref().map(|tracked| tracked.source);
        let created_camera = camera.is_none()
            || tracked
                .as_ref()
                .is_some_and(|tracked| tracked.created_camera);
        if let Some(mut camera) = camera {
            if previous != Some(source) {
                let mut updated = *camera;
                if previous
                    .is_none_or(|previous| previous.viewport_height != source.viewport_height)
                {
                    updated.viewport_height = source.viewport_height;
                }
                if previous.is_none_or(|previous| previous.clear_color != source.clear_color) {
                    updated.clear_color = source.clear_color;
                }
                if previous.is_none_or(|previous| previous.order != source.order) {
                    updated.order = source.order;
                }
                camera.set_if_neq(updated);
            }
        } else {
            commands.entity(entity).try_insert(Camera2d {
                viewport_height: source.viewport_height,
                clear_color: source.clear_color,
                order: source.order,
                ..Camera2d::default()
            });
        }
        if let Some(mut tracked) = tracked {
            if previous != Some(source) || tracked.created_camera != created_camera {
                *tracked = HydratedCamera2d {
                    source,
                    created_camera,
                };
            }
        } else {
            commands.entity(entity).try_insert(HydratedCamera2d {
                source,
                created_camera,
            });
        }
    }
}
