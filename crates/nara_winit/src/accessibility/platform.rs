use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use accesskit::{
    ActionHandler, ActivationHandler, DeactivationHandler, Node, NodeId, Role, Tree, TreeId,
    TreeUpdate,
};
use accesskit_winit::Adapter;
use nara_ui_kernel::{UiAccessibilityInput, UiRevision, UiSurfaceRetirementToken};
use nara_window::WindowId;
use thiserror::Error;
use winit::{
    event::WindowEvent as WinitWindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

use super::{
    WinitAccessibilityCallbackMapping, WinitAccessibilityEndpointRetirement,
    WinitAccessibilityError, WinitAccessibilityPublisher,
    bridge::{
        MAX_PENDING_ACCESSIBILITY_INPUTS, WinitAccessibilityInputBatch,
        WinitAccessibilityPlatformUpdate,
    },
};

const ROOT_NODE_ID: NodeId = NodeId(1);

pub(crate) struct WinitPlatformAccessibilitySession {
    adapter: Adapter,
    publisher: Option<WinitAccessibilityPublisher>,
    callbacks: PlatformCallbackState,
    actions: PlatformActionMailbox,
    last_retirement: Option<UiSurfaceRetirementToken>,
    next_node_id_floor: u64,
}

impl WinitPlatformAccessibilitySession {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        window: &Window,
        window_id: WindowId,
        proxy: EventLoopProxy<crate::WinitUserEvent>,
    ) -> Self {
        let callbacks = PlatformCallbackState::default();
        let actions = PlatformActionMailbox::default();
        let adapter = Adapter::with_direct_handlers(
            event_loop,
            window,
            PlatformActivationHandler {
                callbacks: callbacks.clone(),
            },
            PlatformActionHandler {
                callbacks: callbacks.clone(),
                actions: actions.clone(),
                window_id,
                proxy,
            },
            PlatformDeactivationHandler,
        );
        Self {
            adapter,
            publisher: None,
            callbacks,
            actions,
            last_retirement: None,
            next_node_id_floor: super::FIRST_CONTENT_NODE_ID,
        }
    }

    pub(crate) fn process_window_event(&mut self, window: &Window, event: &WinitWindowEvent) {
        self.adapter.process_event(window, event);
    }

    pub(crate) fn drain_inputs(
        &self,
    ) -> Result<WinitAccessibilityInputBatch, WinitPlatformAccessibilityError> {
        self.actions.drain()
    }

    pub(crate) fn apply_update(
        &mut self,
        update: WinitAccessibilityPlatformUpdate,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        match update {
            WinitAccessibilityPlatformUpdate::Present { snapshot, .. } => {
                let mut publisher = match self.publisher.as_ref() {
                    Some(publisher) if publisher.surface_id() == snapshot.surface_id() => {
                        publisher.candidate_clone()
                    }
                    Some(publisher) => {
                        return Err(WinitAccessibilityError::SurfaceMismatch {
                            expected: publisher.surface_id(),
                            actual: snapshot.surface_id(),
                        }
                        .into());
                    }
                    None => WinitAccessibilityPublisher::with_node_id_floor(
                        snapshot.surface_id(),
                        self.next_node_id_floor,
                    )?,
                };
                if self
                    .last_retirement
                    .is_some_and(|retirement| retirement.surface_id() == snapshot.surface_id())
                {
                    return Err(WinitPlatformAccessibilityError::RetiredSurfaceReused {
                        surface: snapshot.surface_id().get(),
                    });
                }
                let receipt = publisher.publish_update(&snapshot)?;
                let tree = publisher
                    .committed_update()
                    .expect("a successful platform publication retains its exact tree")
                    .accesskit_update()
                    .clone();
                let mapping = publisher.callback_mapping(receipt);
                self.publish_transition(tree, Some(mapping))?;
                self.next_node_id_floor = publisher.next_node_id_floor();
                self.publisher = Some(publisher);
            }
            WinitAccessibilityPlatformUpdate::Invalidate {
                surface_id,
                successor_slot_generation,
                ..
            } => {
                let publisher = if let Some(publisher) = self.publisher.as_ref() {
                    if publisher.surface_id() != surface_id {
                        return Err(WinitAccessibilityError::SurfaceMismatch {
                            expected: publisher.surface_id(),
                            actual: surface_id,
                        }
                        .into());
                    }
                    let mut candidate = publisher.candidate_clone();
                    candidate.invalidate_before(
                        surface_id,
                        UiRevision::new(successor_slot_generation)
                            .ok_or(WinitPlatformAccessibilityError::InvalidationGenerationZero)?,
                    )?;
                    Some(candidate)
                } else {
                    None
                };
                self.publish_transition(empty_tree_update(), None)?;
                self.publisher = publisher;
            }
            WinitAccessibilityPlatformUpdate::Detach {
                surface_id,
                successor_slot_generation,
                ..
            } => {
                self.stage_detachment(surface_id, successor_slot_generation)?;
                self.publish_transition(empty_tree_update(), None)?;
                self.publisher = None;
            }
            WinitAccessibilityPlatformUpdate::Suspend {
                surface_id,
                slot_generation,
                ..
            } => {
                let publisher = if let Some(publisher) = self.publisher.as_ref() {
                    let mut candidate = publisher.candidate_clone();
                    candidate.suspend(super::WinitAccessibilitySuspension::new(
                        surface_id,
                        nara_ui_kernel::UiRevision::new(slot_generation)
                            .expect("a committed Surface Slot has a non-zero generation"),
                    ))?;
                    Some(candidate)
                } else {
                    None
                };
                self.publish_transition(empty_tree_update(), None)?;
                self.publisher = publisher;
            }
            WinitAccessibilityPlatformUpdate::Retire { retirement, .. } => {
                self.stage_retirement(retirement)?;
                self.publish_transition(empty_tree_update(), None)?;
                self.publisher = None;
                self.last_retirement = Some(retirement);
            }
            WinitAccessibilityPlatformUpdate::RetireAndPresent {
                retirement,
                snapshot,
                ..
            } => {
                if retirement.surface_id() == snapshot.surface_id() {
                    return Err(WinitPlatformAccessibilityError::RetiredSurfaceReused {
                        surface: snapshot.surface_id().get(),
                    });
                }
                self.stage_retirement(retirement)?;
                let mut publisher = WinitAccessibilityPublisher::with_node_id_floor(
                    snapshot.surface_id(),
                    self.next_node_id_floor,
                )?;
                let receipt = publisher.publish_update(&snapshot)?;
                let tree = publisher
                    .committed_update()
                    .expect("a successful platform handoff retains its exact tree")
                    .accesskit_update()
                    .clone();
                let mapping = publisher.callback_mapping(receipt);
                self.publish_transition(tree, Some(mapping))?;
                self.next_node_id_floor = publisher.next_node_id_floor();
                self.publisher = Some(publisher);
                self.last_retirement = Some(retirement);
            }
            WinitAccessibilityPlatformUpdate::DetachAndPresent {
                detached_surface_id,
                successor_slot_generation,
                snapshot,
                ..
            } => {
                self.stage_detachment(detached_surface_id, successor_slot_generation)?;
                let mut publisher = WinitAccessibilityPublisher::with_node_id_floor(
                    snapshot.surface_id(),
                    self.next_node_id_floor,
                )?;
                let receipt = publisher.publish_update(&snapshot)?;
                let tree = publisher
                    .committed_update()
                    .expect("a successful platform detach handoff retains its exact tree")
                    .accesskit_update()
                    .clone();
                let mapping = publisher.callback_mapping(receipt);
                self.publish_transition(tree, Some(mapping))?;
                self.next_node_id_floor = publisher.next_node_id_floor();
                self.publisher = Some(publisher);
            }
        }
        Ok(())
    }

    fn stage_retirement(
        &self,
        retirement: UiSurfaceRetirementToken,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        let Some(publisher) = self.publisher.as_ref() else {
            return Ok(());
        };
        if publisher.surface_id() != retirement.surface_id() {
            return Err(WinitAccessibilityError::SurfaceMismatch {
                expected: publisher.surface_id(),
                actual: retirement.surface_id(),
            }
            .into());
        }
        let mut candidate = publisher.candidate_clone();
        candidate.retire(
            retirement,
            WinitAccessibilityEndpointRetirement::PlatformTreeRemoved,
        )?;
        Ok(())
    }

    fn stage_detachment(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
        successor_slot_generation: u64,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        let Some(publisher) = self.publisher.as_ref() else {
            return Ok(());
        };
        if publisher.surface_id() != surface_id {
            return Err(WinitAccessibilityError::SurfaceMismatch {
                expected: publisher.surface_id(),
                actual: surface_id,
            }
            .into());
        }
        let mut candidate = publisher.candidate_clone();
        candidate.invalidate_before(
            surface_id,
            UiRevision::new(successor_slot_generation)
                .ok_or(WinitPlatformAccessibilityError::InvalidationGenerationZero)?,
        )?;
        Ok(())
    }

    fn publish_transition(
        &mut self,
        tree: TreeUpdate,
        mapping: Option<WinitAccessibilityCallbackMapping>,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        let platform_tree = tree.clone();
        self.callbacks.prepare_transition(tree, mapping)?;
        self.adapter.update_if_active(move || platform_tree);
        self.callbacks.finish_transition()
    }
}

#[derive(Clone, Default)]
struct PlatformCallbackState {
    inner: Arc<Mutex<PlatformCallbackFacts>>,
}

impl PlatformCallbackState {
    fn prepare_transition(
        &self,
        tree: TreeUpdate,
        mapping: Option<WinitAccessibilityCallbackMapping>,
    ) -> Result<(), WinitPlatformAccessibilityError> {
        let mut facts = self
            .inner
            .lock()
            .map_err(|_| WinitPlatformAccessibilityError::CallbackStatePoisoned)?;
        facts.tree = Some(tree);
        facts.mapping = mapping;
        facts.transitioning = true;
        Ok(())
    }

    fn finish_transition(&self) -> Result<(), WinitPlatformAccessibilityError> {
        let mut facts = self
            .inner
            .lock()
            .map_err(|_| WinitPlatformAccessibilityError::CallbackStatePoisoned)?;
        facts.transitioning = false;
        Ok(())
    }
}

#[derive(Default)]
struct PlatformCallbackFacts {
    tree: Option<TreeUpdate>,
    mapping: Option<WinitAccessibilityCallbackMapping>,
    transitioning: bool,
}

struct PlatformActivationHandler {
    callbacks: PlatformCallbackState,
}

impl ActivationHandler for PlatformActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let facts = self.callbacks.inner.lock().ok()?;
        facts.tree.clone()
    }
}

struct PlatformActionHandler {
    callbacks: PlatformCallbackState,
    actions: PlatformActionMailbox,
    window_id: WindowId,
    proxy: EventLoopProxy<crate::WinitUserEvent>,
}

impl ActionHandler for PlatformActionHandler {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        let input = {
            let Ok(facts) = self.callbacks.inner.lock() else {
                return;
            };
            if facts.transitioning {
                return;
            }
            facts
                .mapping
                .as_ref()
                .and_then(|mapping| mapping.map_action(request).ok())
        };
        if let Some(input) = input {
            let should_wake = self.actions.push(input).unwrap_or(false);
            if should_wake {
                let _ = self
                    .proxy
                    .send_event(crate::WinitUserEvent::AccessibilityWake(self.window_id));
            }
        }
    }
}

#[derive(Clone, Default)]
struct PlatformActionMailbox {
    inner: Arc<Mutex<PlatformActionMailboxFacts>>,
}

impl PlatformActionMailbox {
    fn push(&self, input: UiAccessibilityInput) -> Result<bool, WinitPlatformAccessibilityError> {
        let mut facts = self
            .inner
            .lock()
            .map_err(|_| WinitPlatformAccessibilityError::ActionMailboxPoisoned)?;
        if facts.pending.len() == MAX_PENDING_ACCESSIBILITY_INPUTS {
            facts.rejected = facts.rejected.saturating_add(1);
        } else {
            facts.pending.push_back(input);
        }
        let should_wake = !facts.wake_pending;
        facts.wake_pending = true;
        Ok(should_wake)
    }

    fn drain(&self) -> Result<WinitAccessibilityInputBatch, WinitPlatformAccessibilityError> {
        let mut facts = self
            .inner
            .lock()
            .map_err(|_| WinitPlatformAccessibilityError::ActionMailboxPoisoned)?;
        facts.wake_pending = false;
        let inputs = facts.pending.drain(..).collect();
        let rejected = std::mem::take(&mut facts.rejected);
        Ok(WinitAccessibilityInputBatch::new(inputs, rejected))
    }
}

#[derive(Default)]
struct PlatformActionMailboxFacts {
    pending: VecDeque<UiAccessibilityInput>,
    rejected: u64,
    wake_pending: bool,
}

struct PlatformDeactivationHandler;

impl DeactivationHandler for PlatformDeactivationHandler {
    fn deactivate_accessibility(&mut self) {}
}

#[derive(Debug, Error)]
pub(crate) enum WinitPlatformAccessibilityError {
    #[error(transparent)]
    Publisher(#[from] WinitAccessibilityError),
    #[error("the native accessibility callback state is poisoned")]
    CallbackStatePoisoned,
    #[error("the native accessibility action mailbox is poisoned")]
    ActionMailboxPoisoned,
    #[error("accessibility invalidation slot generation must be non-zero")]
    InvalidationGenerationZero,
    #[error("retired surface {surface} cannot be installed by a platform handoff")]
    RetiredSurfaceReused { surface: u64 },
}

fn empty_tree_update() -> TreeUpdate {
    let mut tree = Tree::new(ROOT_NODE_ID);
    tree.toolkit_name = Some("Nara".to_owned());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
    TreeUpdate {
        nodes: vec![(ROOT_NODE_ID, Node::new(Role::Window))],
        tree: Some(tree),
        tree_id: TreeId::ROOT,
        focus: ROOT_NODE_ID,
    }
}

#[cfg(test)]
mod tests {
    use nara_ui_kernel::{UiKey, UiRevision, UiRootKey, UiRouteStamp, UiSeatId, UiSurfaceId};

    use super::*;

    #[test]
    fn callback_mailbox_is_bounded_and_uses_one_outstanding_wake() {
        let mailbox = PlatformActionMailbox::default();
        let input = test_input();
        assert!(mailbox.push(input.clone()).unwrap());
        for _ in 1..MAX_PENDING_ACCESSIBILITY_INPUTS + 2 {
            assert!(!mailbox.push(input.clone()).unwrap());
        }

        let (inputs, rejected) = mailbox.drain().unwrap().into_parts();
        assert_eq!(inputs.len(), MAX_PENDING_ACCESSIBILITY_INPUTS);
        assert!(inputs.iter().all(|queued| queued == &input));
        assert_eq!(rejected, 2);
        assert!(mailbox.push(input).unwrap());
    }

    #[test]
    fn activation_observes_the_prepared_tree_while_actions_remain_gated() {
        let callbacks = PlatformCallbackState::default();
        let tree = empty_tree_update();
        callbacks.prepare_transition(tree.clone(), None).unwrap();
        let mut handler = PlatformActivationHandler {
            callbacks: callbacks.clone(),
        };

        let observed = handler.request_initial_tree().unwrap();
        assert_eq!(observed.focus, tree.focus);
        assert_eq!(observed.nodes, tree.nodes);
        assert!(callbacks.inner.lock().unwrap().transitioning);
        callbacks.finish_transition().unwrap();
        assert!(!callbacks.inner.lock().unwrap().transitioning);
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
        let key = UiKey::root(UiRootKey::runtime("platform-test", "root").unwrap()).unwrap();
        UiAccessibilityInput::focus_target_from_route(route, UiSeatId::PRIMARY, key)
    }
}
