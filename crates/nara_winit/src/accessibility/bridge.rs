use std::{collections::VecDeque, mem};

use nara_app::{
    __RuntimeDriverPort, App, PluginError, RuntimeDriverScope, RuntimeWorldAccessError,
};
use nara_ecs::{Resource, World};
use nara_ui_kernel::{
    UiAccessibilityInput, UiSemanticSnapshot, UiSurfaceId, UiSurfaceRetirementToken,
};
use nara_window::WindowId;
use thiserror::Error;

/// Marker installed only by the explicit root product contribution.
///
/// The runner treats the absence of this marker as a valid disabled mode. It never infers native
/// accessibility activation from Cargo feature availability alone.
#[derive(Debug, Default, Resource)]
pub(super) struct WinitAccessibilityContribution;

/// The runner-facing activation state captured before native window startup.
///
/// `Disabled` is deliberately valid when the platform-accessibility code is compiled but the
/// product did not install the explicit contribution. `Partial` is a pre-run failure and must
/// never be interpreted as a disabled product.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinitAccessibilityAccess(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinitAccessibilityActivation {
    Disabled,
    Enabled(WinitAccessibilityAccess),
    Partial(WinitAccessibilityActivationError),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum WinitAccessibilityActivationError {
    #[error("the explicit Winit accessibility contribution is missing bridge resources")]
    MissingBridgeResources,
}

impl WinitAccessibilityActivation {
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled(_))
    }
}

pub fn capture_winit_accessibility_activation(world: &World) -> WinitAccessibilityActivation {
    if !world.contains_resource::<WinitAccessibilityContribution>() {
        return WinitAccessibilityActivation::Disabled;
    }
    if world.contains_resource::<WinitAccessibilityPlatformUpdates>()
        && world.contains_resource::<WinitAccessibilityInputs>()
    {
        WinitAccessibilityActivation::Enabled(WinitAccessibilityAccess(()))
    } else {
        WinitAccessibilityActivation::Partial(
            WinitAccessibilityActivationError::MissingBridgeResources,
        )
    }
}

// Two lifecycle synchronizations plus one presentation per active surface fit below this bound.
const MAX_PENDING_ACCESSIBILITY_UPDATES: usize = 64;
pub(super) const MAX_PENDING_ACCESSIBILITY_INPUTS: usize = 64;

/// One ordered platform update for a Host-owned native window.
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
}

impl WinitAccessibilityPlatformUpdate {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        match self {
            Self::Present { window_id, .. }
            | Self::Invalidate { window_id, .. }
            | Self::Suspend { window_id, .. }
            | Self::Detach { window_id, .. }
            | Self::Retire { window_id, .. } => *window_id,
        }
    }
}

#[derive(Debug, Default, Resource)]
struct WinitAccessibilityPlatformUpdates {
    pending: VecDeque<WinitAccessibilityPlatformUpdate>,
}

impl __RuntimeDriverPort for WinitAccessibilityPlatformUpdates {
    type Input = ();
    type Output = Vec<WinitAccessibilityPlatformUpdate>;

    fn apply_driver_input(&mut self, (): Self::Input) -> Self::Output {
        mem::take(&mut self.pending).into_iter().collect()
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
    #[error("the Winit accessibility bridge update limit {maximum} was exceeded")]
    UpdateLimitExceeded { maximum: usize },
}

fn install_winit_accessibility_bridge(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<WinitAccessibilityPlatformUpdates>()?
        .init_resource::<WinitAccessibilityInputs>()?;
    Ok(())
}

pub fn install_winit_accessibility_contribution(app: &mut App) -> Result<(), PluginError> {
    install_winit_accessibility_bridge(app)?;
    app.init_resource::<WinitAccessibilityContribution>()?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn install_partial_winit_accessibility_contribution(
    app: &mut App,
) -> Result<(), PluginError> {
    app.init_resource::<WinitAccessibilityContribution>()?;
    Ok(())
}

pub fn publish_winit_accessibility_update(
    world: &mut World,
    update: WinitAccessibilityPlatformUpdate,
) -> Result<(), WinitAccessibilityBridgeError> {
    let mut updates = world
        .get_resource_mut::<WinitAccessibilityPlatformUpdates>()
        .ok_or(WinitAccessibilityBridgeError::NotInstalled)?;
    if updates.pending.len() == MAX_PENDING_ACCESSIBILITY_UPDATES {
        return Err(WinitAccessibilityBridgeError::UpdateLimitExceeded {
            maximum: MAX_PENDING_ACCESSIBILITY_UPDATES,
        });
    }
    updates.pending.push_back(update);
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
    fn platform_accessibility_activation_is_explicit_and_three_state() {
        let app = App::new();
        assert_eq!(
            capture_winit_accessibility_activation(app.world()),
            WinitAccessibilityActivation::Disabled
        );

        let mut partial = World::new();
        partial.insert_resource(WinitAccessibilityContribution);
        assert!(matches!(
            capture_winit_accessibility_activation(&partial),
            WinitAccessibilityActivation::Partial(_)
        ));

        let mut enabled = App::new();
        install_winit_accessibility_contribution(&mut enabled).unwrap();
        assert!(capture_winit_accessibility_activation(enabled.world()).is_enabled());
    }

    #[test]
    fn platform_updates_preserve_lifecycle_order_per_window() {
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
        let successor = WinitAccessibilityPlatformUpdate::Suspend {
            window_id,
            surface_id,
            slot_generation: 3,
        };
        publish_winit_accessibility_update(&mut world, successor.clone()).unwrap();

        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert_eq!(
            updates,
            vec![
                WinitAccessibilityPlatformUpdate::Suspend {
                    window_id,
                    surface_id,
                    slot_generation: 2,
                },
                successor,
            ]
        );
    }

    #[test]
    fn retirement_precedes_successor_presentation_without_dropping_either_fact() {
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
        assert_eq!(
            updates,
            vec![
                WinitAccessibilityPlatformUpdate::Retire {
                    window_id,
                    retirement,
                },
                WinitAccessibilityPlatformUpdate::Present {
                    window_id,
                    snapshot,
                },
            ]
        );
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
    fn same_frame_invalidation_reaches_the_platform_before_a_stale_snapshot() {
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

        let stale = WinitAccessibilityPlatformUpdate::Present {
            window_id,
            snapshot: test_semantics(surface, 1),
        };
        publish_winit_accessibility_update(&mut world, stale.clone()).unwrap();
        let updates = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert_eq!(updates, vec![invalidation, stale]);
    }

    #[test]
    fn detachment_precedes_successor_presentation_without_losing_its_floor() {
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
        assert_eq!(
            updates,
            vec![
                WinitAccessibilityPlatformUpdate::Detach {
                    window_id,
                    surface_id: detached_surface,
                    successor_slot_generation: 2,
                },
                WinitAccessibilityPlatformUpdate::Present {
                    window_id,
                    snapshot,
                },
            ]
        );
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

    #[test]
    fn platform_update_queue_is_bounded_and_reusable_after_a_drain() {
        let mut world = World::new();
        world.insert_resource(WinitAccessibilityPlatformUpdates::default());
        let surface = UiSurfaceId::new(10).unwrap();
        for value in 1..=MAX_PENDING_ACCESSIBILITY_UPDATES {
            publish_winit_accessibility_update(
                &mut world,
                WinitAccessibilityPlatformUpdate::Suspend {
                    window_id: WindowId::new(value as u64),
                    surface_id: surface,
                    slot_generation: value as u64,
                },
            )
            .unwrap();
        }
        assert_eq!(
            publish_winit_accessibility_update(
                &mut world,
                WinitAccessibilityPlatformUpdate::Suspend {
                    window_id: WindowId::PRIMARY,
                    surface_id: surface,
                    slot_generation: MAX_PENDING_ACCESSIBILITY_UPDATES as u64 + 1,
                },
            ),
            Err(WinitAccessibilityBridgeError::UpdateLimitExceeded {
                maximum: MAX_PENDING_ACCESSIBILITY_UPDATES,
            })
        );

        let drained = world
            .resource_mut::<WinitAccessibilityPlatformUpdates>()
            .apply_driver_input(());
        assert_eq!(drained.len(), MAX_PENDING_ACCESSIBILITY_UPDATES);
        publish_winit_accessibility_update(
            &mut world,
            WinitAccessibilityPlatformUpdate::Suspend {
                window_id: WindowId::new(u64::MAX),
                surface_id: surface,
                slot_generation: 1,
            },
        )
        .unwrap();
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
