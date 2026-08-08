use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use nara_app::{App, Plugin, PluginError};
use nara_asset::AssetServer;
use nara_ecs::{Res, Resource, World, system::SystemParam};
use nara_image::{IMAGE_PREPARE_PLUGIN_ID, PreparedImageResource};
use nara_render::{PreparedRenderResources, RenderTarget};
use nara_ui_kernel::{UiFrame, UiSurfaceId};
use nara_window::WindowId;
use thiserror::Error;

use crate::planner::collect_referenced_image_ids;
use crate::{
    UiPaintPacket, UiPaintPacketIdentity, UiPreparedImages, UiPresentationAttemptIdentity,
    UiPresentationContentError, UiPresentationOutcome, UiPresentationPlanner,
};

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

    #[must_use]
    const fn target(self) -> RenderTarget {
        RenderTarget::Window(self.window_id)
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
struct UiPresentationState(UiPresentationPlanner);

#[doc(hidden)]
#[derive(Debug, Default, Resource)]
pub struct UiPresentationOutcomes {
    outcomes: Vec<UiPresentationOutcome>,
}

impl UiPresentationOutcomes {
    pub fn push(&mut self, outcome: UiPresentationOutcome) {
        self.outcomes.push(outcome);
    }

    fn replace(&mut self, outcomes: Vec<UiPresentationOutcome>) {
        self.outcomes = outcomes;
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
    let previous = {
        let Some(mut routes) = world.get_resource_mut::<UiPresentationRoutes>() else {
            return Err(UiPresentationIntegrationError::Unavailable);
        };
        routes.routes.insert(route.surface_id(), route)
    };
    if let Some(previous) = previous.filter(|previous| previous.target() != route.target())
        && let Some(mut planner) = world.get_resource_mut::<UiPresentationState>()
    {
        planner
            .0
            .retire_surface_target(previous.surface_id(), previous.target());
    }
    Ok(previous)
}

pub fn retire_ui_presentation_route(
    world: &mut World,
    surface_id: UiSurfaceId,
) -> Result<Option<UiPresentationRoute>, UiPresentationIntegrationError> {
    let removed = {
        let Some(mut routes) = world.get_resource_mut::<UiPresentationRoutes>() else {
            return Err(UiPresentationIntegrationError::Unavailable);
        };
        routes.routes.remove(&surface_id)
    };
    if let (Some(route), Some(mut planner)) =
        (removed, world.get_resource_mut::<UiPresentationState>())
    {
        planner
            .0
            .retire_surface_target(route.surface_id(), route.target());
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
    let previous = {
        let Some(mut installed) = world.get_resource_mut::<UiPresentationRoutes>() else {
            return Err(UiPresentationIntegrationError::Unavailable);
        };
        std::mem::replace(&mut installed.routes, replacement.clone())
    };
    let retired_targets = previous
        .iter()
        .filter_map(|(surface, route)| {
            (!replacement
                .get(surface)
                .is_some_and(|replacement| replacement.target() == route.target()))
            .then_some((*surface, route.target()))
        })
        .collect::<HashSet<_>>();
    if let Some(mut planner) = world.get_resource_mut::<UiPresentationState>() {
        for (surface_id, target) in retired_targets {
            planner.0.retire_surface_target(surface_id, target);
        }
    }
    Ok(())
}

/// Compiles exact immutable runtime frames without querying ECS UI authoring components.
pub fn compile_ui_paint_packets(
    world: &mut World,
    frames: &[Arc<UiFrame>],
) -> Result<UiPaintCompileStats, UiPresentationIntegrationError> {
    if !world.contains_resource::<UiPresentationState>()
        || !world.contains_resource::<UiPresentationOutcomes>()
        || !world.contains_resource::<UiPresentationRoutes>()
    {
        return Err(UiPresentationIntegrationError::Unavailable);
    }

    let routes = world.resource::<UiPresentationRoutes>().routes.clone();
    let target_conflicts = target_conflicts(&routes);
    let duplicate_surfaces = duplicate_frame_surfaces(frames);
    let referenced_images = collect_referenced_image_ids(frames);
    let image_index = UiPreparedImages::capture(
        &referenced_images,
        world.get_resource::<AssetServer>(),
        world.get_resource::<PreparedRenderResources<PreparedImageResource>>(),
    );
    let mut outcomes = Vec::new();
    let mut stats = UiPaintCompileStats::default();
    world.resource_scope(|_world, mut planner: nara_ecs::Mut<UiPresentationState>| {
        for frame in frames {
            let identity = UiPaintPacketIdentity::from_frame(frame);
            if duplicate_surfaces.contains(&identity.surface_id()) {
                let error = UiPresentationContentError::DuplicateSurfaceFrame {
                    surface_id: identity.surface_id(),
                };
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(match routes.get(&identity.surface_id()).copied() {
                    Some(route) => build_rejection(identity, route.target(), error),
                    None => UiPresentationOutcome::ContentRejected {
                        attempt: UiPresentationAttemptIdentity::unrouted(identity),
                        message: error.to_string(),
                    },
                });
                continue;
            }
            let Some(route) = routes.get(&identity.surface_id()).copied() else {
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(UiPresentationOutcome::ContentRejected {
                    attempt: UiPresentationAttemptIdentity::unrouted(identity),
                    message: "the Host has not installed an explicit presentation route for this UI surface"
                        .to_owned(),
                });
                continue;
            };
            let target = route.target();
            if let Some((first_surface, second_surface)) = target_conflicts.get(&target).copied() {
                let error = UiPresentationContentError::TargetRouteConflict {
                    target,
                    first_surface,
                    second_surface,
                };
                stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                outcomes.push(build_rejection(identity, target, error));
                continue;
            }
            match planner.0.plan(Arc::clone(frame), target, &image_index) {
                Ok(_) => {
                    stats.admitted_packets = stats.admitted_packets.saturating_add(1);
                }
                Err(error) => {
                    stats.rejected_packets = stats.rejected_packets.saturating_add(1);
                    outcomes.push(build_rejection(identity, target, error));
                }
            }
        }
    });
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

fn target_conflicts(
    routes: &BTreeMap<UiSurfaceId, UiPresentationRoute>,
) -> HashMap<RenderTarget, (UiSurfaceId, UiSurfaceId)> {
    let mut first_owner = HashMap::<RenderTarget, UiSurfaceId>::new();
    let mut conflicts = HashMap::new();
    for route in routes.values().copied() {
        match first_owner.get(&route.target()).copied() {
            Some(first_surface) if first_surface != route.surface_id() => {
                conflicts
                    .entry(route.target())
                    .or_insert((first_surface, route.surface_id()));
            }
            Some(_) => {}
            None => {
                first_owner.insert(route.target(), route.surface_id());
            }
        }
    }
    conflicts
}

fn duplicate_frame_surfaces(frames: &[Arc<UiFrame>]) -> HashSet<UiSurfaceId> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for frame in frames {
        let surface_id = frame.environment().surface_id();
        if !seen.insert(surface_id) {
            duplicates.insert(surface_id);
        }
    }
    duplicates
}

fn build_rejection(
    identity: UiPaintPacketIdentity,
    target: RenderTarget,
    error: UiPresentationContentError,
) -> UiPresentationOutcome {
    UiPresentationOutcome::ContentRejected {
        attempt: UiPresentationAttemptIdentity::routed(identity, target),
        message: error.to_string(),
    }
}

#[doc(hidden)]
#[derive(SystemParam)]
pub struct UiPaintPacketReader<'w> {
    state: Option<Res<'w, UiPresentationState>>,
}

impl UiPaintPacketReader<'_> {
    #[must_use]
    pub fn snapshot(&self) -> Vec<Arc<UiPaintPacket>> {
        self.state
            .as_deref()
            .map_or_else(Vec::new, |state| state.0.snapshot_packets())
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
        let planner = UiPresentationPlanner::new(Default::default()).map_err(|error| {
            PluginError::SetupFailed {
                plugin: UI_RENDER_PLUGIN_ID,
                message: error.to_string(),
            }
        })?;
        app.insert_resource(UiPresentationState(planner))?;
        app.init_resource::<UiPresentationRoutes>()?;
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

    fn frame(surface: u64, source_revision: u64, visual: UiVisual) -> Arc<UiFrame> {
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
            UiSurfaceId::new(surface).unwrap(),
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
        world.insert_resource(UiPresentationState(UiPresentationPlanner::default()));
        world.insert_resource(routes);
        world.insert_resource(UiPresentationOutcomes::default());
        world
    }

    #[test]
    fn rejected_candidate_keeps_the_exact_last_good_packet_arc() {
        let mut world = packet_world();
        let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
        let admitted = frame(1, 1, UiVisual::solid(white));
        compile_ui_paint_packets(&mut world, &[admitted]).unwrap();
        let first = world.resource::<UiPresentationState>().0.snapshot_packets()[0].clone();

        let missing_image = UiImageId::new("project://ui/missing.png").unwrap();
        let rejected = frame(1, 2, UiVisual::image(missing_image, white));
        let stats = compile_ui_paint_packets(&mut world, &[rejected]).unwrap();

        assert_eq!(stats.rejected_packets, 1);
        assert!(Arc::ptr_eq(
            &world.resource::<UiPresentationState>().0.snapshot_packets()[0],
            &first
        ));
    }

    #[test]
    fn two_surfaces_cannot_compete_for_one_target() {
        let mut world = packet_world();
        let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
        compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
        let original = world.resource::<UiPresentationState>().0.snapshot_packets()[0].clone();
        install_ui_presentation_route(
            &mut world,
            UiPresentationRoute::new(UiSurfaceId::new(2).unwrap(), WindowId::PRIMARY).unwrap(),
        )
        .unwrap();
        let stats = compile_ui_paint_packets(
            &mut world,
            &[
                frame(1, 2, UiVisual::solid(white)),
                frame(2, 2, UiVisual::solid(white)),
            ],
        )
        .unwrap();

        assert_eq!(stats.admitted_packets, 0);
        assert_eq!(stats.rejected_packets, 2);
        assert!(Arc::ptr_eq(
            &world.resource::<UiPresentationState>().0.snapshot_packets()[0],
            &original
        ));
    }

    #[test]
    fn retiring_a_conflicting_route_does_not_release_the_target_owner() {
        let mut world = packet_world();
        let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
        compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
        let original = world.resource::<UiPresentationState>().0.snapshot_packets()[0].clone();
        let second_surface = UiSurfaceId::new(2).unwrap();
        install_ui_presentation_route(
            &mut world,
            UiPresentationRoute::new(second_surface, WindowId::PRIMARY).unwrap(),
        )
        .unwrap();

        retire_ui_presentation_route(&mut world, second_surface).unwrap();

        let packets = world.resource::<UiPresentationState>().0.snapshot_packets();
        assert_eq!(packets.len(), 1);
        assert!(Arc::ptr_eq(&packets[0], &original));
    }

    #[test]
    fn duplicate_surface_frames_reject_the_whole_surface_attempt() {
        let mut world = packet_world();
        let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
        compile_ui_paint_packets(&mut world, &[frame(1, 1, UiVisual::solid(white))]).unwrap();
        let original = world.resource::<UiPresentationState>().0.snapshot_packets()[0].clone();

        let stats = compile_ui_paint_packets(
            &mut world,
            &[
                frame(1, 2, UiVisual::solid(white)),
                frame(1, 3, UiVisual::solid(white)),
            ],
        )
        .unwrap();

        assert_eq!(stats.admitted_packets, 0);
        assert_eq!(stats.rejected_packets, 2);
        assert!(Arc::ptr_eq(
            &world.resource::<UiPresentationState>().0.snapshot_packets()[0],
            &original
        ));
        let outcomes = drain_ui_presentation_outcomes(&mut world).unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|outcome| {
            outcome
                .message()
                .is_some_and(|message| message.contains("more than once"))
        }));
    }

    #[test]
    fn outcome_journal_does_not_silently_truncate_backend_results() {
        let frame = frame(1, 1, UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)));
        let identity = UiPaintPacketIdentity::from_frame(&frame);
        let mut outcomes = UiPresentationOutcomes::default();
        for index in 0..65 {
            outcomes.push(UiPresentationOutcome::ContentRejected {
                attempt: UiPresentationAttemptIdentity::unrouted(identity),
                message: index.to_string(),
            });
        }

        assert_eq!(outcomes.drain().len(), 65);
    }
}
