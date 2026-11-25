//! Explicit keyboard Button continuation through the original Input owner.

use std::any::type_name;

use nara_app::{RuntimeDriverScope, RuntimeState, RuntimeWorldAccessError};
use nara_ecs::{Mut, World};

use crate::{
    ActionContext, ActionId, ActionMaps, ActionStaging, ActionTransform, ButtonInput,
    InputAdmission, InputBinding, InputIngressError, InputSeatId, InputTimeline, InputWindowId,
    KeyCode, SemanticActions, timeline::InputEndpointClaim,
};

/// One declared key's exact Button action and context in the receiving game configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardActionBinding {
    pub key: KeyCode,
    pub action: ActionId,
    pub context: ActionContext,
}

/// Portable held data; it contains no endpoint claim or old event/member identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardHold {
    pub binding: KeyboardActionBinding,
    pub held: bool,
}

/// Refusal of one complete keyboard continuation before its original-owner admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardContinuationError {
    MissingResources,
    InvalidCount,
    DuplicateBinding,
    BindingMismatch,
    ConfigurationPending,
    PendingInput,
    UnsupportedMembers,
    MembersPresent,
    ProjectionMismatch,
    SemanticCapacity,
    StaleEndpoint,
    Ingress(InputIngressError),
    WorldAccess(RuntimeWorldAccessError),
}

impl std::fmt::Display for KeyboardContinuationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "keyboard continuation refused: {self:?}")
    }
}

impl std::error::Error for KeyboardContinuationError {}

impl From<InputIngressError> for KeyboardContinuationError {
    fn from(error: InputIngressError) -> Self {
        Self::Ingress(error)
    }
}

impl From<RuntimeWorldAccessError> for KeyboardContinuationError {
    fn from(error: RuntimeWorldAccessError) -> Self {
        Self::WorldAccess(error)
    }
}

/// Captures only the declared keyboard inventory after all original consumers completed.
///
/// The caller separately proves its healthy completed paused frame and native isolation policy.
/// A staged Release, stale mapping/context or extra held member refuses capture.
/// An action may have Gamepad Button alternatives alongside its unique declared keyboard source;
/// a held gamepad contribution remains outside this keyboard inventory and refuses capture.
pub fn capture_keyboard_holds(
    world: &World,
    window: InputWindowId,
    seat: InputSeatId,
    bindings: &[KeyboardActionBinding],
) -> Result<Vec<KeyboardHold>, KeyboardContinuationError> {
    let timeline = world
        .get_resource::<InputTimeline>()
        .ok_or(KeyboardContinuationError::MissingResources)?;
    let maps = world
        .get_resource::<ActionMaps>()
        .ok_or(KeyboardContinuationError::MissingResources)?;
    let staging = world
        .get_resource::<ActionStaging>()
        .ok_or(KeyboardContinuationError::MissingResources)?;
    let semantic = world
        .get_resource::<SemanticActions>()
        .ok_or(KeyboardContinuationError::MissingResources)?;
    let keyboard = world
        .get_resource::<ButtonInput<KeyCode>>()
        .ok_or(KeyboardContinuationError::MissingResources)?;
    validate_bindings(timeline, maps, staging, seat, bindings)?;
    let members = timeline.capture_keyboard_members(window, seat, bindings)?;
    semantic.validate_keyboard_capture(maps, staging, seat, bindings, &members)?;
    bindings
        .iter()
        .zip(members)
        .map(|(binding, sequence)| {
            if let Some(sequence) = sequence
                && keyboard
                    .pressed
                    .binary_search(&(binding.key, sequence))
                    .is_err()
            {
                return Err(KeyboardContinuationError::ProjectionMismatch);
            }
            Ok(KeyboardHold {
                binding: binding.clone(),
                held: sequence.is_some(),
            })
        })
        .collect()
}

/// Restores a complete finite keyboard inventory into its exact original endpoint claim.
///
/// The runtime must be paused. The native owner must keep ordinary input isolated until
/// activation reconciliation. This creates fresh local members and semantic holds without
/// emitting physical Press or Started. Original projection/observation maintenance still runs.
pub fn restore_keyboard_holds(
    scope: &mut RuntimeDriverScope<'_>,
    claim: InputEndpointClaim,
    holds: Vec<KeyboardHold>,
) -> Result<InputAdmission, KeyboardContinuationError> {
    require_paused(scope.state())?;
    scope.with_world_mut(|world| restore_keyboard_inventory(world, &claim, &holds))
}

/// Cancels declared gameplay members absent from the native owner's current physical observation.
///
/// This admits terminal records with ActivationMismatch through original reserved credits.
/// The game must consume the resulting semantic Release before activation is complete.
pub fn reconcile_keyboard_holds(
    scope: &mut RuntimeDriverScope<'_>,
    claim: InputEndpointClaim,
    released: Vec<KeyCode>,
) -> Result<InputAdmission, KeyboardContinuationError> {
    require_paused(scope.state())?;
    scope.with_world_mut(|world| {
        require_continuation_resources(world)?;
        world
            .resource_mut::<InputTimeline>()
            .reconcile_keyboard_members(&claim, &released)
    })
}

fn require_paused(state: RuntimeState) -> Result<(), KeyboardContinuationError> {
    if state != RuntimeState::Paused {
        return Err(RuntimeWorldAccessError::Unavailable { state }.into());
    }
    Ok(())
}

fn require_continuation_resources(world: &World) -> Result<(), KeyboardContinuationError> {
    if !world.contains_resource::<ActionStaging>() {
        return Err(RuntimeWorldAccessError::MissingResource {
            type_name: type_name::<ActionStaging>(),
        }
        .into());
    }
    if !world.contains_resource::<InputTimeline>()
        || !world.contains_resource::<ActionMaps>()
        || !world.contains_resource::<SemanticActions>()
        || !world.contains_resource::<ButtonInput<KeyCode>>()
    {
        return Err(KeyboardContinuationError::MissingResources);
    }
    Ok(())
}

fn validate_bindings(
    timeline: &InputTimeline,
    maps: &ActionMaps,
    staging: &ActionStaging,
    seat: InputSeatId,
    bindings: &[KeyboardActionBinding],
) -> Result<(), KeyboardContinuationError> {
    if bindings.is_empty() || bindings.len() > timeline.limits().members() {
        return Err(KeyboardContinuationError::InvalidCount);
    }
    let map = maps
        .get(seat)
        .ok_or(KeyboardContinuationError::BindingMismatch)?;
    if !staging.matches_current_map(seat, map) {
        return Err(KeyboardContinuationError::ConfigurationPending);
    }
    for (index, expected) in bindings.iter().enumerate() {
        if bindings[..index].iter().any(|other| {
            other.key == expected.key
                || (other.action == expected.action && other.context == expected.context)
        }) {
            return Err(KeyboardContinuationError::DuplicateBinding);
        }
        let mut actual = map.binding_indices_for_input(InputBinding::Key(expected.key));
        let binding = actual.next().and_then(|index| map.binding_at(index));
        if actual.next().is_some()
            || !binding.is_some_and(|binding| {
                binding.action == expected.action
                    && binding.context == expected.context
                    && binding.transform == ActionTransform::Button
                    && map.is_context_enabled(&expected.context)
            })
        {
            return Err(KeyboardContinuationError::BindingMismatch);
        }
        if map
            .bindings()
            .iter()
            .filter(|binding| {
                binding.action == expected.action && binding.context == expected.context
            })
            .any(|binding| {
                binding.transform != ActionTransform::Button
                    || match binding.input {
                        InputBinding::Key(key) => key != expected.key,
                        InputBinding::GamepadButton(_) => false,
                        _ => true,
                    }
            })
        {
            return Err(KeyboardContinuationError::BindingMismatch);
        }
    }
    Ok(())
}

fn restore_keyboard_inventory(
    world: &mut World,
    claim: &InputEndpointClaim,
    holds: &[KeyboardHold],
) -> Result<InputAdmission, KeyboardContinuationError> {
    require_continuation_resources(world)?;
    if holds.is_empty() || holds.len() > world.resource::<InputTimeline>().limits().members() {
        return Err(KeyboardContinuationError::InvalidCount);
    }
    world.resource_scope(|world, staging: Mut<ActionStaging>| {
        let bindings = holds
            .iter()
            .map(|hold| hold.binding.clone())
            .collect::<Vec<_>>();
        let timeline = world.resource::<InputTimeline>();
        let maps = world.resource::<ActionMaps>();
        validate_bindings(timeline, maps, &staging, claim.seat(), &bindings)?;
        world
            .resource::<SemanticActions>()
            .validate_keyboard_restore(
                claim.seat(),
                holds.iter().filter(|hold| hold.held).count(),
            )?;
        let (admission, members) = world
            .resource_mut::<InputTimeline>()
            .restore_keyboard_members(claim, holds)?;
        world.resource_scope(|world, mut semantic: Mut<SemanticActions>| {
            semantic.restore_keyboard_members(
                world.resource::<ActionMaps>(),
                &staging,
                claim.seat(),
                holds,
                &members,
            );
            world
                .resource_mut::<InputTimeline>()
                .observe_semantic_holds(&semantic);
        });
        Ok(admission)
    })
}

#[cfg(test)]
mod tests {
    use nara_app::{App, RuntimeInstance, RuntimeTimeSettings};

    use super::*;
    use crate::InputPlugin;

    #[test]
    fn continuation_reports_missing_resources_before_admission() {
        let mut app = App::new();
        app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .unwrap();
        app.add_plugin(InputPlugin).unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let claim = runtime
            .world()
            .resource::<InputTimeline>()
            .claim_endpoint(InputWindowId::UNSPECIFIED, InputSeatId::PRIMARY)
            .unwrap();
        let outcome = runtime
            .with_driver_scope(|scope| {
                scope.apply_command(|world: &mut World| {
                    world.remove_resource::<ActionMaps>();
                });
                for result in [
                    restore_keyboard_holds(scope, claim.clone(), Vec::new()),
                    reconcile_keyboard_holds(scope, claim.clone(), Vec::new()),
                ] {
                    assert_eq!(result, Err(KeyboardContinuationError::MissingResources));
                }
                scope.apply_command(|world: &mut World| {
                    world.remove_resource::<ActionStaging>();
                });
                for result in [
                    restore_keyboard_holds(scope, claim.clone(), Vec::new()),
                    reconcile_keyboard_holds(scope, claim, Vec::new()),
                ] {
                    assert_eq!(
                        result,
                        Err(RuntimeWorldAccessError::MissingResource {
                            type_name: type_name::<ActionStaging>(),
                        }
                        .into())
                    );
                }
            })
            .unwrap();
        assert!(outcome.post_operation_fault.is_none());
        assert_eq!(
            runtime.world().resource::<InputTimeline>().retained_len(),
            0
        );
        runtime.finish().unwrap();
    }
}
