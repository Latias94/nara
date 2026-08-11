use std::{
    collections::{BTreeMap, VecDeque},
    mem,
};

use nara_app::{
    __RuntimeDriverPort, App, PluginError, RuntimeDriverScope, RuntimeWorldAccessError,
};
use nara_ecs::{Resource, World};
use nara_ui_kernel::{
    UiAccessibilityInput, UiSemanticSnapshot, UiSurfaceId, UiSurfaceRetirementToken,
};
use nara_window::WindowId;
use thiserror::Error;

const MAX_ACCESSIBILITY_TARGETS: usize = 16;
pub(super) const MAX_PENDING_ACCESSIBILITY_INPUTS: usize = 64;

/// One latest-only platform update for a Host-owned native window.
#[derive(Clone, Debug, PartialEq)]
pub enum WinitAccessibilityPlatformUpdate {
    Present {
        window_id: WindowId,
        snapshot: UiSemanticSnapshot,
    },
    Invalidate {
        window_id: WindowId,
        surface_id: UiSurfaceId,
        successor_slot_generation: u64,
    },
    Suspend {
        window_id: WindowId,
        surface_id: UiSurfaceId,
        slot_generation: u64,
    },
    Detach {
        window_id: WindowId,
        surface_id: UiSurfaceId,
        successor_slot_generation: u64,
    },
    Retire {
        window_id: WindowId,
        retirement: UiSurfaceRetirementToken,
    },
    RetireAndPresent {
        window_id: WindowId,
        retirement: UiSurfaceRetirementToken,
        snapshot: UiSemanticSnapshot,
    },
    DetachAndPresent {
        window_id: WindowId,
        detached_surface_id: UiSurfaceId,
        successor_slot_generation: u64,
        snapshot: UiSemanticSnapshot,
    },
}

impl WinitAccessibilityPlatformUpdate {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        match self {
            Self::Present { window_id, .. }
            | Self::Invalidate { window_id, .. }
            | Self::Suspend { window_id, .. }
            | Self::Detach { window_id, .. }
            | Self::Retire { window_id, .. }
            | Self::RetireAndPresent { window_id, .. }
            | Self::DetachAndPresent { window_id, .. } => *window_id,
        }
    }
}

#[derive(Debug, Default, Resource)]
struct WinitAccessibilityPlatformUpdates {
    latest_by_window: BTreeMap<WindowId, WinitAccessibilityPlatformUpdate>,
}

impl __RuntimeDriverPort for WinitAccessibilityPlatformUpdates {
    type Input = ();
    type Output = Vec<WinitAccessibilityPlatformUpdate>;

    fn apply_driver_input(&mut self, (): Self::Input) -> Self::Output {
        mem::take(&mut self.latest_by_window)
            .into_values()
            .collect()
    }
}

#[derive(Debug, Default, Resource)]
struct WinitAccessibilityInputs {
    pending: VecDeque<UiAccessibilityInput>,
    rejected: u64,
}

impl __RuntimeDriverPort for WinitAccessibilityInputs {
    type Input = WinitAccessibilityInputBatch;
    type Output = ();

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.rejected = self.rejected.saturating_add(input.rejected);
        for input in input.inputs {
            if self.pending.len() == MAX_PENDING_ACCESSIBILITY_INPUTS {
                self.rejected = self.rejected.saturating_add(1);
            } else {
                self.pending.push_back(input);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct WinitAccessibilityInputBatch {
    inputs: Vec<UiAccessibilityInput>,
    rejected: u64,
}

impl WinitAccessibilityInputBatch {
    pub(super) const fn new(inputs: Vec<UiAccessibilityInput>, rejected: u64) -> Self {
        Self { inputs, rejected }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<UiAccessibilityInput>, u64) {
        (self.inputs, self.rejected)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WinitAccessibilityBridgeError {
    #[error("the Winit accessibility bridge is not installed")]
    NotInstalled,
    #[error("the Winit accessibility bridge target limit {maximum} was exceeded")]
    TargetLimitExceeded { maximum: usize },
    #[error(
        "surface {surface} cannot be presented after its retirement in the same platform update"
    )]
    RetiredSurfaceReused { surface: u64 },
    #[error(
        "surface changed from {expected} to {actual} on one native target without a retirement or detach handoff"
    )]
    SurfaceChangedWithoutHandoff { expected: u64, actual: u64 },
    #[error("platform snapshot slot generation {candidate} predates transition floor {floor}")]
    SnapshotPredatesTransition { floor: u64, candidate: u64 },
}

pub fn install_winit_accessibility_bridge(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<WinitAccessibilityPlatformUpdates>()?
        .init_resource::<WinitAccessibilityInputs>()?;
    Ok(())
}

pub fn publish_winit_accessibility_update(
    world: &mut World,
    update: WinitAccessibilityPlatformUpdate,
) -> Result<(), WinitAccessibilityBridgeError> {
    let mut updates = world
        .get_resource_mut::<WinitAccessibilityPlatformUpdates>()
        .ok_or(WinitAccessibilityBridgeError::NotInstalled)?;
    let window_id = update.window_id();
    if !updates.latest_by_window.contains_key(&window_id)
        && updates.latest_by_window.len() == MAX_ACCESSIBILITY_TARGETS
    {
        return Err(WinitAccessibilityBridgeError::TargetLimitExceeded {
            maximum: MAX_ACCESSIBILITY_TARGETS,
        });
    }
    let update = updates.latest_by_window.get(&window_id).map_or_else(
        || Ok(update.clone()),
        |current| coalesce_updates(current, &update),
    )?;
    updates.latest_by_window.insert(window_id, update);
    Ok(())
}

fn coalesce_updates(
    current: &WinitAccessibilityPlatformUpdate,
    next: &WinitAccessibilityPlatformUpdate,
) -> Result<WinitAccessibilityPlatformUpdate, WinitAccessibilityBridgeError> {
    match (current, next) {
        (
            WinitAccessibilityPlatformUpdate::Retire {
                window_id,
                retirement,
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        )
        | (
            WinitAccessibilityPlatformUpdate::RetireAndPresent {
                window_id,
                retirement,
                ..
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        ) => {
            if retirement.surface_id() == snapshot.surface_id() {
                return Err(WinitAccessibilityBridgeError::RetiredSurfaceReused {
                    surface: snapshot.surface_id().get(),
                });
            }
            Ok(WinitAccessibilityPlatformUpdate::RetireAndPresent {
                window_id: *window_id,
                retirement: *retirement,
                snapshot: snapshot.clone(),
            })
        }
        (
            WinitAccessibilityPlatformUpdate::Detach {
                window_id,
                surface_id,
                successor_slot_generation,
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        )
        | (
            WinitAccessibilityPlatformUpdate::DetachAndPresent {
                window_id,
                detached_surface_id: surface_id,
                successor_slot_generation,
                ..
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        ) => Ok(WinitAccessibilityPlatformUpdate::DetachAndPresent {
            window_id: *window_id,
            detached_surface_id: *surface_id,
            successor_slot_generation: *successor_slot_generation,
            snapshot: snapshot.clone(),
        }),
        (
            WinitAccessibilityPlatformUpdate::Invalidate {
                surface_id,
                successor_slot_generation,
                ..
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        ) => {
            if *surface_id != snapshot.surface_id() {
                return Err(
                    WinitAccessibilityBridgeError::SurfaceChangedWithoutHandoff {
                        expected: surface_id.get(),
                        actual: snapshot.surface_id().get(),
                    },
                );
            }
            validate_transition_floor(*successor_slot_generation, snapshot)?;
            Ok(next.clone())
        }
        (
            WinitAccessibilityPlatformUpdate::Present {
                snapshot: current_snapshot,
                ..
            },
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. },
        ) if current_snapshot.surface_id() != snapshot.surface_id() => Err(
            WinitAccessibilityBridgeError::SurfaceChangedWithoutHandoff {
                expected: current_snapshot.surface_id().get(),
                actual: snapshot.surface_id().get(),
            },
        ),
        _ => Ok(next.clone()),
    }
}

fn validate_transition_floor(
    floor: u64,
    snapshot: &UiSemanticSnapshot,
) -> Result<(), WinitAccessibilityBridgeError> {
    let candidate = snapshot.route().slot_generation().get();
    if candidate < floor {
        return Err(WinitAccessibilityBridgeError::SnapshotPredatesTransition { floor, candidate });
    }
    Ok(())
}

pub fn take_winit_accessibility_updates(
    scope: &mut RuntimeDriverScope<'_>,
) -> Result<Vec<WinitAccessibilityPlatformUpdate>, RuntimeWorldAccessError> {
    scope.__apply_port::<WinitAccessibilityPlatformUpdates>(())
}

pub fn queue_winit_accessibility_inputs(
    scope: &mut RuntimeDriverScope<'_>,
    input: WinitAccessibilityInputBatch,
) -> Result<(), RuntimeWorldAccessError> {
    scope.__apply_port::<WinitAccessibilityInputs>(input)
}

pub fn drain_winit_accessibility_inputs(world: &mut World) -> WinitAccessibilityInputBatch {
    world
        .get_resource_mut::<WinitAccessibilityInputs>()
        .map_or_else(WinitAccessibilityInputBatch::default, |mut inputs| {
            let pending = inputs.pending.drain(..).collect();
            let rejected = mem::take(&mut inputs.rejected);
            WinitAccessibilityInputBatch::new(pending, rejected)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use accesskit::{Action, ActionRequest, TreeId};
    use nara_ui_kernel::{
        UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiInteraction,
        UiInteractionSession, UiKey, UiLayer, UiLocalKey, UiNode, UiRevision, UiRoot, UiRootKey,
        UiRouteStamp, UiSeatId, UiSemanticRole, UiSemantics, UiSize, UiStyle,
        UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
    };

    use super::super::{WinitAccessibilityError, WinitAccessibilityPublisher};
    use super::*;

    #[test]
    fn platform_updates_keep_only_the_latest_fact_per_window() {
        let mut world = World::new();
        world.insert_resource(WinitAccessibilityPlatformUpdates::default());
        let window_id = WindowId::PRIMARY;
        let surface_id = UiSurfaceId::new(1).unwrap();

        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Suspend {
                window_id,
                surface_id,
                slot_generation: 2,
            },
        )
        .unwrap();
        let latest = WinitAccessibilityPlatformUpdate::Suspend {
            window_id,
            surface_id,
            slot_generation: 3,
        };
        publish_winit_accessibility_update(&mut world, latest.clone()).unwrap();

        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert_eq!(updates, vec![latest]);
    }

    #[test]
    fn retirement_and_successor_presentation_coalesce_into_one_typed_handoff() {
        let mut world = World::new();
        world.insert_resource(WinitAccessibilityPlatformUpdates::default());
        let window_id = WindowId::PRIMARY;
        let retired_surface = UiSurfaceId::new(1).unwrap();
        let successor_surface = UiSurfaceId::new(2).unwrap();
        let mut journal = nara_ui_kernel::UiFrameJournal::default();
        journal.install_surface(retired_surface, 1).unwrap();
        let retirement = journal.retire_surface(retired_surface, 2).unwrap().unwrap();
        let snapshot = test_semantics(successor_surface, 1);

        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Retire {
                window_id,
                retirement,
            },
        )
        .unwrap();
        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Present {
                window_id,
                snapshot: snapshot.clone(),
            },
        )
        .unwrap();

        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert!(matches!(
            updates.as_slice(),
            [WinitAccessibilityPlatformUpdate::RetireAndPresent {
                retirement: actual_retirement,
                snapshot: actual_snapshot,
                ..
            }] if *actual_retirement == retirement && actual_snapshot == &snapshot
        ));
    }

    #[test]
    fn invalidation_floor_rejects_a_late_platform_snapshot() {
        let surface = UiSurfaceId::new(3).unwrap();
        let snapshot = test_semantics(surface, 1);
        let mut publisher = WinitAccessibilityPublisher::new(surface).unwrap();
        publisher.publish_update(&snapshot).unwrap();
        let floor = UiRevision::new(2).unwrap();
        publisher.invalidate_before(surface, floor).unwrap();

        assert_eq!(
            publisher.publish_update(&snapshot).unwrap_err(),
            WinitAccessibilityError::SnapshotPredatesInvalidation {
                floor,
                candidate: snapshot.route(),
            }
        );
        assert!(publisher.committed_update().is_none());
    }

    #[test]
    fn same_frame_invalidation_rejects_a_snapshot_below_its_floor() {
        let mut world = World::new();
        world.insert_resource(WinitAccessibilityPlatformUpdates::default());
        let window_id = WindowId::PRIMARY;
        let surface = UiSurfaceId::new(4).unwrap();
        let invalidation = WinitAccessibilityPlatformUpdate::Invalidate {
            window_id,
            surface_id: surface,
            successor_slot_generation: 2,
        };
        publish_winit_accessibility_update(&mut world, invalidation.clone()).unwrap();

        assert_eq!(
            publish_winit_accessibility_update(
                &mut world,
                WinitAccessibilityPlatformUpdate::Present {
                    window_id,
                    snapshot: test_semantics(surface, 1),
                },
            ),
            Err(WinitAccessibilityBridgeError::SnapshotPredatesTransition {
                floor: 2,
                candidate: 1,
            })
        );
        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert_eq!(updates, vec![invalidation]);
    }

    #[test]
    fn detach_and_successor_presentation_coalesce_into_one_typed_handoff() {
        let mut world = World::new();
        world.insert_resource(WinitAccessibilityPlatformUpdates::default());
        let window_id = WindowId::PRIMARY;
        let detached_surface = UiSurfaceId::new(5).unwrap();
        let successor_surface = UiSurfaceId::new(6).unwrap();
        let snapshot = test_semantics(successor_surface, 1);
        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Detach {
                window_id,
                surface_id: detached_surface,
                successor_slot_generation: 2,
            },
        )
        .unwrap();
        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Present {
                window_id,
                snapshot: snapshot.clone(),
            },
        )
        .unwrap();

        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert!(matches!(
            updates.as_slice(),
            [WinitAccessibilityPlatformUpdate::DetachAndPresent {
                detached_surface_id,
                successor_slot_generation: 2,
                snapshot: actual_snapshot,
                ..
            }] if *detached_surface_id == detached_surface && actual_snapshot == &snapshot
        ));
    }

    #[test]
    fn window_node_identifiers_do_not_repeat_across_publishers() {
        let first_surface = UiSurfaceId::new(7).unwrap();
        let mut first = WinitAccessibilityPublisher::new(first_surface).unwrap();
        first
            .publish_update(&test_semantics(first_surface, 1))
            .unwrap();
        let first_node = actionable_node(&first);
        let floor = first.next_node_id_floor();

        let second_surface = UiSurfaceId::new(8).unwrap();
        let mut second =
            WinitAccessibilityPublisher::with_node_id_floor(second_surface, floor).unwrap();
        let receipt = second
            .publish_update(&test_semantics(second_surface, 1))
            .unwrap();
        let second_node = actionable_node(&second);
        assert_ne!(first_node, second_node);
        assert!(second_node.0 >= floor);

        assert_eq!(
            second
                .callback_mapping(receipt)
                .map_action(ActionRequest {
                    action: Action::Click,
                    target_tree: TreeId::ROOT,
                    target_node: first_node,
                    data: None,
                })
                .unwrap_err(),
            WinitAccessibilityError::UnknownNode { node: first_node }
        );
    }

    #[test]
    fn exact_newer_retirement_closes_an_older_platform_route() {
        let surface = UiSurfaceId::new(9).unwrap();
        let mut publisher = WinitAccessibilityPublisher::new(surface).unwrap();
        publisher
            .publish_update(&test_semantics(surface, 1))
            .unwrap();
        let mut journal = nara_ui_kernel::UiFrameJournal::default();
        journal.install_surface(surface, 2).unwrap();
        let retirement = journal.retire_surface(surface, 3).unwrap().unwrap();

        publisher
            .retire(
                retirement,
                super::super::WinitAccessibilityEndpointRetirement::PlatformTreeRemoved,
            )
            .unwrap();
        assert!(publisher.committed_update().is_none());
    }

    #[test]
    fn accessibility_input_queue_bounds_entries_and_counts_rejections() {
        let input = test_input();
        let mut inputs = WinitAccessibilityInputs::default();
        inputs.apply_driver_input(WinitAccessibilityInputBatch::new(
            vec![input.clone(); MAX_PENDING_ACCESSIBILITY_INPUTS + 2],
            3,
        ));

        assert_eq!(inputs.pending.len(), MAX_PENDING_ACCESSIBILITY_INPUTS);
        assert!(inputs.pending.iter().all(|queued| queued == &input));
        assert_eq!(inputs.rejected, 5);
    }

    fn test_input() -> UiAccessibilityInput {
        let revision = UiRevision::new(1).unwrap();
        let route = UiRouteStamp::new(
            UiSurfaceId::new(1).unwrap(),
            revision,
            revision,
            revision,
            1,
        );
        let key = UiKey::root(UiRootKey::runtime("test", "root").unwrap()).unwrap();
        UiAccessibilityInput::focus_target_from_route(route, UiSeatId::PRIMARY, key)
    }

    fn test_semantics(surface: UiSurfaceId, slot_generation: u64) -> UiSemanticSnapshot {
        let action = UiActionId::new("activate").unwrap();
        let authoring = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime(
                        "platform-bridge-test",
                        format!("surface-{}", surface.get()),
                    )
                    .unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(10.0, 10.0, 80.0, 40.0))
                        .with_key(UiLocalKey::runtime("platform-bridge-test", "action").unwrap())
                        .with_interaction(UiInteraction::focus_target())
                        .with_semantics(
                            UiSemantics::new(UiSemanticRole::Button, "Action")
                                .unwrap()
                                .with_action(action.clone())
                                .unwrap(),
                        ),
                ),
            )
            .build()
            .unwrap();
        let mut compiler =
            UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default()).unwrap();
        let frame = compiler
            .compile(
                &authoring,
                UiEnvironment::for_surface(
                    surface,
                    UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 1.0)
                        .unwrap(),
                    UiSurfaceGenerations::new(
                        slot_generation,
                        slot_generation,
                        slot_generation,
                        slot_generation,
                    )
                    .unwrap(),
                    1,
                    1,
                )
                .unwrap(),
                UiTheme::default(),
            )
            .unwrap();
        let (mut journal, authority) =
            nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
        journal
            .install_surface(surface, frame.environment().slot_generation().get())
            .unwrap();
        let receipt = authority
            .submit(&mut journal, Arc::clone(&frame), 1)
            .unwrap();
        UiInteractionSession::default()
            .semantic_snapshot_with_action_authority(
                &journal,
                UiRouteStamp::from_receipt(receipt),
                UiSeatId::PRIMARY,
                |_, candidate| candidate == &action,
            )
            .unwrap()
    }

    fn actionable_node(publisher: &WinitAccessibilityPublisher) -> accesskit::NodeId {
        publisher
            .committed_update()
            .unwrap()
            .accesskit_update()
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Action"))
            .map(|(node, _)| *node)
            .unwrap()
    }
}
