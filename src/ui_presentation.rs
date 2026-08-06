use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::{World, schedule::IntoScheduleConfigs};

#[derive(Debug, Default, nara_ecs::Resource)]
pub(crate) struct UiPresentationBridgeStats {
    pub(crate) last_admitted_packets: u32,
    pub(crate) last_rejected_packets: u32,
    pub(crate) presented_packets: u64,
    pub(crate) deferred_packets: u64,
    pub(crate) content_rejections: u64,
    pub(crate) backend_faults: u64,
    pub(crate) receipt_rejections: u64,
    pub(crate) last_error: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct UiPresentationBridgePlugin;

pub(crate) const UI_PRESENTATION_BRIDGE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.ui-presentation-bridge");
const UI_PRESENTATION_BRIDGE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[nara_ui::UI_PLUGIN_ID, nara_ui_render::UI_RENDER_PLUGIN_ID];
const UI_PRESENTATION_BRIDGE_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("presentation-ui")];
pub(crate) const UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        UI_PRESENTATION_BRIDGE_PLUGIN_ID,
        nara_app::PluginCategory::Render,
    )
    .requires_plugins(UI_PRESENTATION_BRIDGE_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(UI_PRESENTATION_BRIDGE_PRODUCT_REQUIREMENTS);

impl Plugin for UiPresentationBridgePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<UiPresentationBridgeStats>()?;
        app.add_systems(
            CoreStage::First,
            synchronize_runtime_ui_surface_metrics
                .after(nara_window::WindowSet::ApplyEvents)
                .before(nara_input::InputSet::RouteUi),
        )?;
        app.add_systems(CoreStage::Queue, compile_runtime_ui_packets)?;
        app.add_systems(CoreStage::Cleanup, commit_runtime_ui_presentations)?;
        Ok(())
    }
}

fn synchronize_runtime_ui_surface_metrics(world: &mut World) {
    let windows = {
        let mut query = world.query::<&nara_window::Window>();
        query
            .iter(world)
            .map(|window| (window.id, window.resolution))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let descriptors = match nara_ui::active_ui_surface_descriptors(world) {
        Ok(descriptors) => descriptors,
        Err(error) => {
            record_bridge_error(world, error.to_string());
            return;
        }
    };

    for descriptor in descriptors {
        let window_id = nara_window::WindowId::new(descriptor.input_window().get());
        let Some(resolution) = windows.get(&window_id).copied() else {
            continue;
        };
        let Some((scale_factor, input_scale_factor)) = surface_mapping(descriptor, resolution)
        else {
            continue;
        };
        let updated = match descriptor.with_surface_mapping(scale_factor, input_scale_factor) {
            Ok(updated) => updated,
            Err(error) => {
                record_bridge_error(world, error.to_string());
                continue;
            }
        };
        if updated != descriptor
            && let Err(error) = nara_ui::install_ui_surface(world, updated)
        {
            record_bridge_error(world, error.to_string());
        }
    }
}

fn surface_mapping(
    descriptor: nara_ui::UiSurfaceDescriptor,
    resolution: nara_window::WindowResolution,
) -> Option<(f32, f32)> {
    if resolution.is_zero_sized()
        || !resolution.scale_factor.is_finite()
        || resolution.scale_factor <= 0.0
    {
        return None;
    }
    let (logical_width, logical_height) = descriptor.logical_size();
    let scale_factor = (resolution.physical_width as f32 / logical_width)
        .min(resolution.physical_height as f32 / logical_height);
    let input_scale_factor = scale_factor / resolution.scale_factor as f32;
    (scale_factor.is_finite()
        && scale_factor > 0.0
        && input_scale_factor.is_finite()
        && input_scale_factor > 0.0)
        .then_some((scale_factor, input_scale_factor))
}

fn compile_runtime_ui_packets(world: &mut World) {
    if let Err(error) = synchronize_runtime_ui_routes(world) {
        record_bridge_error(world, error);
        return;
    }
    let frames = match nara_ui::latest_ui_frames(world) {
        Ok(frames) => frames,
        Err(error) => {
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    match nara_ui_render::compile_ui_paint_packets(world, &frames) {
        Ok(stats) => {
            let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
            bridge.last_admitted_packets = stats.admitted_packets;
            bridge.last_rejected_packets = stats.rejected_packets;
            bridge.last_error = None;
        }
        Err(error) => record_bridge_error(world, error.to_string()),
    }
}

fn synchronize_runtime_ui_routes(world: &mut World) -> Result<(), String> {
    let descriptors =
        nara_ui::active_ui_surface_descriptors(world).map_err(|error| error.to_string())?;
    let routes = descriptors
        .into_iter()
        .map(|descriptor| {
            nara_ui_render::UiPresentationRoute::new(
                descriptor.surface_id(),
                nara_window::WindowId::new(descriptor.input_window().get()),
            )
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    nara_ui_render::__private::replace_ui_presentation_routes(world, routes)
        .map_err(|error| error.to_string())
}

fn commit_runtime_ui_presentations(world: &mut World) {
    let outcomes = match nara_ui_render::drain_ui_presentation_outcomes(world) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    for outcome in outcomes {
        match outcome {
            nara_ui_render::UiPresentationOutcome::Presented {
                packet,
                presentation_epoch,
            } => match nara_ui::commit_ui_presentation(world, packet.frame(), presentation_epoch) {
                Ok(_) => {
                    world
                        .resource_mut::<UiPresentationBridgeStats>()
                        .presented_packets += 1;
                }
                Err(error) => {
                    let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                    bridge.receipt_rejections += 1;
                    bridge.last_error = Some(error.to_string());
                }
            },
            nara_ui_render::UiPresentationOutcome::ContentRejected { message, .. } => {
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.content_rejections += 1;
                bridge.last_error = Some(message);
            }
            nara_ui_render::UiPresentationOutcome::Deferred { message, .. } => {
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.deferred_packets += 1;
                bridge.last_error = Some(message);
            }
            nara_ui_render::UiPresentationOutcome::BackendFault { message, .. } => {
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.backend_faults += 1;
                bridge.last_error = Some(message);
            }
        }
    }
}

fn record_bridge_error(world: &mut World, message: String) {
    if let Some(mut stats) = world.get_resource_mut::<UiPresentationBridgeStats>() {
        stats.last_error = Some(message);
    }
}

#[cfg(test)]
mod tests {
    use nara_hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara_input::{InputSeatId, InputWindowId};
    use nara_ui::{UiNode, UiRoot, UiRuntimeKey, UiSurfaceDescriptor, UiSurfaceId};

    use super::*;
    use crate::{PresentationUiPlugins, RuntimeUiPlugins};

    #[test]
    fn physical_targets_preserve_one_design_space_and_normalize_platform_logical_input() {
        let descriptor = UiSurfaceDescriptor::new(
            UiSurfaceId::new(7).expect("test surface should be non-zero"),
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            960.0,
            540.0,
            1.0,
            1,
            1,
            1,
        )
        .expect("test surface descriptor should be valid");

        for (resolution, expected_render_scale, expected_input_scale) in [
            (
                nara_window::WindowResolution::new(1_280, 720),
                4.0 / 3.0,
                4.0 / 3.0,
            ),
            (
                nara_window::WindowResolution::new(1_920, 1_080).with_scale_factor(2.0),
                2.0,
                1.0,
            ),
            (
                nara_window::WindowResolution::new(2_560, 1_440).with_scale_factor(2.0),
                8.0 / 3.0,
                4.0 / 3.0,
            ),
        ] {
            let (render_scale, input_scale) =
                surface_mapping(descriptor, resolution).expect("target mapping should be valid");
            assert!((render_scale - expected_render_scale).abs() < 1.0e-6);
            assert!((input_scale - expected_input_scale).abs() < 1.0e-6);
        }

        assert!(surface_mapping(descriptor, nara_window::WindowResolution::new(0, 720)).is_none());
        assert!(
            surface_mapping(
                descriptor,
                nara_window::WindowResolution::new(1_280, 720).with_scale_factor(0.0),
            )
            .is_none()
        );
    }

    #[test]
    fn bridge_mirrors_and_retires_explicit_host_surface_routes() {
        let mut app = App::new();
        app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
            .expect("runtime and presentation UI products should install");
        let surface_id = UiSurfaceId::new(7).expect("test surface should be non-zero");
        let root = {
            let world = app.world_mut().expect("test app should expose its world");
            nara_ui::install_ui_surface(
                world,
                UiSurfaceDescriptor::new(
                    surface_id,
                    InputWindowId::new(11),
                    InputSeatId::PRIMARY,
                    320.0,
                    180.0,
                    1.0,
                    1,
                    1,
                    1,
                )
                .expect("test surface descriptor should be valid"),
            )
            .expect("test surface should install");
            let root = world
                .spawn((
                    UiRoot::default(),
                    UiNode::default(),
                    UiRuntimeKey::new("test.presentation-bridge", "root")
                        .expect("test root key should be valid"),
                ))
                .id();
            HierarchyWriter::new(world)
                .apply_batch(&[HierarchyOperation::Place {
                    entity: root,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                }])
                .expect("test root should enter structural order");
            root
        };

        app.update()
            .expect("the bridge should compile one explicitly routed UI frame");

        let stats = app.world().resource::<UiPresentationBridgeStats>();
        assert_eq!(stats.last_admitted_packets, 1);
        assert_eq!(stats.last_rejected_packets, 0);
        assert_eq!(stats.last_error, None);
        assert!(app.world().get::<UiRoot>(root).is_some());
        let frame = nara_ui::latest_ui_frame(app.world(), surface_id)
            .expect("UI runtime should remain installed")
            .expect("the active surface should have one compiled frame");

        nara_ui::retire_ui_surface(
            app.world_mut().expect("test app should expose its world"),
            surface_id,
        )
        .expect("test surface retirement should succeed");
        app.update()
            .expect("route retirement should be synchronized without a stale packet");

        let compile = nara_ui_render::compile_ui_paint_packets(
            app.world_mut().expect("test app should expose its world"),
            &[frame],
        )
        .expect("the UI packet compiler should remain available");
        assert_eq!(compile.admitted_packets, 0);
        assert_eq!(compile.rejected_packets, 1);
    }
}
