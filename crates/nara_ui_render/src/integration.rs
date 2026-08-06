use std::{collections::BTreeMap, sync::Arc};

use nara_app::{App, Plugin, PluginError};
use nara_asset::AssetServer;
use nara_ecs::{Res, Resource, World, system::SystemParam};
use nara_image::{IMAGE_PREPARE_PLUGIN_ID, PreparedImageResource};
use nara_render::{PreparedRenderResources, RenderTarget};
use nara_ui_kernel::{UiFrame, UiSurfaceId};
use nara_window::WindowId;
use thiserror::Error;

use crate::{
    UiPaintPacket, UiPaintPacketBuildError, UiPaintPacketBuilder, UiPaintPacketIdentity,
    UiPreparedImages, UiPresentationOutcome,
};

const MAX_PRESENTATION_OUTCOMES: usize = 64;

/// Host-owned explicit mapping from one stable UI surface to one render window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPresentationRoute {
    surface_id: UiSurfaceId,
    window_id: WindowId,
}

impl UiPresentationRoute {
    pub fn new(
        surface_id: UiSurfaceId,
        window_id: WindowId,
    ) -> Result<Self, UiPresentationRouteError> {
        if window_id.get() == 0 {
            return Err(UiPresentationRouteError::ZeroWindowId);
        }
        Ok(Self {
            surface_id,
            window_id,
        })
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationRouteError {
    #[error("UI presentation routes require a non-zero window identifier")]
    ZeroWindowId,
}

#[derive(Debug, Default, Resource)]
struct UiPresentationRoutes {
    routes: BTreeMap<UiSurfaceId, UiPresentationRoute>,
}

#[derive(Resource)]
struct UiPacketCompiler(UiPaintPacketBuilder);

#[derive(Debug, Default, Resource)]
struct UiPaintPacketStore {
    packets: Vec<Arc<UiPaintPacket>>,
}

#[doc(hidden)]
#[derive(Debug, Default, Resource)]
pub struct UiPresentationOutcomes {
    outcomes: Vec<UiPresentationOutcome>,
}

impl UiPresentationOutcomes {
    pub fn push(&mut self, outcome: UiPresentationOutcome) {
        if self.outcomes.len() < MAX_PRESENTATION_OUTCOMES {
            self.outcomes.push(outcome);
        }
    }

    fn replace(&mut self, outcomes: Vec<UiPresentationOutcome>) {
        self.outcomes = outcomes;
        self.outcomes.truncate(MAX_PRESENTATION_OUTCOMES);
    }

    fn drain(&mut self) -> Vec<UiPresentationOutcome> {
        self.outcomes.drain(..).collect()
    }
}

/// Summary of one runtime-frame to paint-packet compilation pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiPaintCompileStats {
    pub admitted_packets: u32,
    pub rejected_packets: u32,
}

pub fn install_ui_presentation_route(
    world: &mut World,
    route: UiPresentationRoute,
) -> Result<Option<UiPresentationRoute>, UiPresentationIntegrationError> {
    let Some(mut routes) = world.get_resource_mut::<UiPresentationRoutes>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    Ok(routes.routes.insert(route.surface_id(), route))
}

pub fn retire_ui_presentation_route(
    world: &mut World,
    surface_id: UiSurfaceId,
) -> Result<Option<UiPresentationRoute>, UiPresentationIntegrationError> {
    let Some(mut routes) = world.get_resource_mut::<UiPresentationRoutes>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    let removed = routes.routes.remove(&surface_id);
    if let Some(mut store) = world.get_resource_mut::<UiPaintPacketStore>() {
        store
            .packets
            .retain(|packet| packet.identity().surface_id() != surface_id);
    }
    Ok(removed)
}

#[doc(hidden)]
pub fn replace_ui_presentation_routes(
    world: &mut World,
    routes: impl IntoIterator<Item = UiPresentationRoute>,
) -> Result<(), UiPresentationIntegrationError> {
    let replacement = routes
        .into_iter()
        .map(|route| (route.surface_id(), route))
        .collect::<BTreeMap<_, _>>();
    let Some(mut installed) = world.get_resource_mut::<UiPresentationRoutes>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    installed.routes = replacement.clone();
    drop(installed);

    if let Some(mut store) = world.get_resource_mut::<UiPaintPacketStore>() {
        store.packets.retain(|packet| {
            replacement
                .get(&packet.identity().surface_id())
                .is_some_and(|route| packet.target() == RenderTarget::Window(route.window_id()))
        });
    }
    Ok(())
}

/// Compiles exact immutable runtime frames without querying ECS UI authoring components.
pub fn compile_ui_paint_packets(
    world: &mut World,
    frames: &[Arc<UiFrame>],
) -> Result<UiPaintCompileStats, UiPresentationIntegrationError> {
    if !world.contains_resource::<UiPacketCompiler>()
        || !world.contains_resource::<UiPaintPacketStore>()
        || !world.contains_resource::<UiPresentationOutcomes>()
        || !world.contains_resource::<UiPresentationRoutes>()
    {
        return Err(UiPresentationIntegrationError::Unavailable);
    }

    let routes = world.resource::<UiPresentationRoutes>().routes.clone();
    let image_index = UiPreparedImages::capture(
        world.get_resource::<AssetServer>(),
        world.get_resource::<PreparedRenderResources<PreparedImageResource>>(),
    );
    let mut packets = world
        .resource::<UiPaintPacketStore>()
        .packets
        .iter()
        .map(|packet| (packet.identity().surface_id(), Arc::clone(packet)))
        .collect::<BTreeMap<_, _>>();
    let mut outcomes = Vec::new();
    let mut stats = UiPaintCompileStats::default();
    world.resource_scope(|_world, mut compiler: nara_ecs::Mut<UiPacketCompiler>| {
        for frame in frames {
            let identity = UiPaintPacketIdentity::from_frame(frame);
            let Some(route) = routes.get(&identity.surface_id()).copied() else {
                packets.remove(&identity.surface_id());
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(UiPresentationOutcome::ContentRejected {
                    attempt: identity,
                    message: "the Host has not installed an explicit presentation route for this UI surface"
                        .to_owned(),
                });
                continue;
            };
            match compiler.0.build(
                Arc::clone(frame),
                RenderTarget::Window(route.window_id()),
                &image_index,
            ) {
                Ok(packet) => {
                    stats.admitted_packets = stats.admitted_packets.saturating_add(1);
                    packets.insert(identity.surface_id(), packet);
                }
                Err(error) => {
                    stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                    outcomes.push(build_rejection(identity, error));
                }
            }
        }
    });
    world.resource_mut::<UiPaintPacketStore>().packets = packets.into_values().collect();
    world
        .resource_mut::<UiPresentationOutcomes>()
        .replace(outcomes);
    Ok(stats)
}

pub fn drain_ui_presentation_outcomes(
    world: &mut World,
) -> Result<Vec<UiPresentationOutcome>, UiPresentationIntegrationError> {
    let Some(mut outcomes) = world.get_resource_mut::<UiPresentationOutcomes>() else {
        return Err(UiPresentationIntegrationError::Unavailable);
    };
    Ok(outcomes.drain())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationIntegrationError {
    #[error("UI presentation integration is unavailable because UiRenderPlugin is not installed")]
    Unavailable,
}

fn build_rejection(
    attempt: UiPaintPacketIdentity,
    error: UiPaintPacketBuildError,
) -> UiPresentationOutcome {
    UiPresentationOutcome::ContentRejected {
        attempt,
        message: error.to_string(),
    }
}

#[doc(hidden)]
#[derive(SystemParam)]
pub struct UiPaintPacketReader<'w> {
    packets: Option<Res<'w, UiPaintPacketStore>>,
}

impl UiPaintPacketReader<'_> {
    #[must_use]
    pub fn snapshot(&self) -> Vec<Arc<UiPaintPacket>> {
        self.packets
            .as_deref()
            .map_or_else(Vec::new, |store| store.packets.clone())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UiRenderPlugin;

pub const UI_RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui-render");
const UI_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[nara_render::RENDER_PLUGIN_ID, IMAGE_PREPARE_PLUGIN_ID];
const UI_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("presentation-ui")];
pub const UI_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(UI_RENDER_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_RENDER_PRODUCT_REQUIREMENTS);

impl Plugin for UiRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let compiler = UiPaintPacketBuilder::new(Default::default()).map_err(|error| {
            PluginError::SetupFailed {
                plugin: UI_RENDER_PLUGIN_ID,
                message: error.to_string(),
            }
        })?;
        app.insert_resource(UiPacketCompiler(compiler))?;
        app.init_resource::<UiPresentationRoutes>()?;
        app.init_resource::<UiPaintPacketStore>()?;
        app.init_resource::<UiPresentationOutcomes>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_text::bundled_font_face;
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiImageId, UiLayer,
        UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle, UiTheme, UiVisual,
    };

    fn frame(source_revision: u64, visual: UiVisual) -> Arc<UiFrame> {
        let node = UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
            .with_key(UiLocalKey::runtime("packet-retention", "node").unwrap())
            .with_visual(visual);
        let snapshot = UiAuthoringSnapshotBuilder::new(source_revision)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("packet-retention", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(node),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            64.0,
            64.0,
            1.0,
            source_revision,
            1,
            1,
            1,
        )
        .unwrap();
        UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap()
    }

    fn packet_world() -> World {
        let surface_id = UiSurfaceId::new(1).unwrap();
        let route = UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap();
        let mut routes = UiPresentationRoutes::default();
        routes.routes.insert(surface_id, route);
        let mut world = World::new();
        world.insert_resource(UiPacketCompiler(UiPaintPacketBuilder::default()));
        world.insert_resource(routes);
        world.insert_resource(UiPaintPacketStore::default());
        world.insert_resource(UiPresentationOutcomes::default());
        world
    }

    #[test]
    fn rejected_candidate_keeps_the_last_good_packet() {
        let mut world = packet_world();
        let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
        let admitted = frame(1, UiVisual::solid(white));
        let first = compile_ui_paint_packets(&mut world, &[admitted]).unwrap();
        assert_eq!(first.admitted_packets, 1);
        let first_identity = world.resource::<UiPaintPacketStore>().packets[0].identity();

        let missing_image = UiImageId::new("project://ui/missing.png").unwrap();
        let rejected = frame(2, UiVisual::image(missing_image, white));
        let second = compile_ui_paint_packets(&mut world, &[rejected]).unwrap();

        assert_eq!(second.rejected_packets, 1);
        assert_eq!(world.resource::<UiPaintPacketStore>().packets.len(), 1);
        assert_eq!(
            world.resource::<UiPaintPacketStore>().packets[0].identity(),
            first_identity
        );
    }
}
