//! Explicit keyboard Button continuation through the original Input owner.

use nara_app::{__RuntimeDriverPort, RuntimeDriverScope, RuntimeState, RuntimeWorldAccessError};
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

#[doc(hidden)]
pub struct KeyboardRestoreRequest {
    claim: InputEndpointClaim,
    holds: Vec<KeyboardHold>,
}

#[doc(hidden)]
pub enum KeyboardContinuationOperation {
    Restore(KeyboardRestoreRequest),
    Reconcile {
        claim: InputEndpointClaim,
        released: Vec<KeyCode>,
    },
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
/// The typed port accepts only Paused. The native owner must keep ordinary input isolated until
/// activation reconciliation. This creates fresh local members and semantic holds without
/// emitting physical Press or Started. Original projection/observation maintenance still runs.
pub fn restore_keyboard_holds(
    scope: &mut RuntimeDriverScope<'_>,
    claim: InputEndpointClaim,
    holds: Vec<KeyboardHold>,
) -> Result<InputAdmission, KeyboardContinuationError> {
    scope.__apply_port::<ActionStaging>(KeyboardContinuationOperation::Restore(
        KeyboardRestoreRequest { claim, holds },
    ))?
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
    scope.__apply_port::<ActionStaging>(KeyboardContinuationOperation::Reconcile {
        claim,
        released,
    })?
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

pub(crate) fn apply_registered(
    world: &mut World,
    input: KeyboardContinuationOperation,
) -> Result<
    Result<InputAdmission, KeyboardContinuationError>,
    (RuntimeWorldAccessError, KeyboardContinuationOperation),
> {
    if !world.contains_resource::<InputTimeline>()
        || !world.contains_resource::<ActionMaps>()
        || !world.contains_resource::<SemanticActions>()
        || !world.contains_resource::<ButtonInput<KeyCode>>()
    {
        return Ok(Err(KeyboardContinuationError::MissingResources));
    }
    let input = match input {
        KeyboardContinuationOperation::Reconcile { claim, released } => {
            return Ok(world
                .resource_mut::<InputTimeline>()
                .reconcile_keyboard_members(&claim, &released));
        }
        KeyboardContinuationOperation::Restore(input) => input,
    };
    if input.holds.is_empty()
        || input.holds.len() > world.resource::<InputTimeline>().limits().members()
    {
        return Ok(Err(KeyboardContinuationError::InvalidCount));
    }
    Ok(world.resource_scope(|world, staging: Mut<ActionStaging>| {
        let bindings = input
            .holds
            .iter()
            .map(|hold| hold.binding.clone())
            .collect::<Vec<_>>();
        let timeline = world.resource::<InputTimeline>();
        let maps = world.resource::<ActionMaps>();
        validate_bindings(timeline, maps, &staging, input.claim.seat(), &bindings)?;
        world
            .resource::<SemanticActions>()
            .validate_keyboard_restore(
                input.claim.seat(),
                input.holds.iter().filter(|hold| hold.held).count(),
            )?;
        let (admission, members) = world
            .resource_mut::<InputTimeline>()
            .restore_keyboard_members(&input.claim, &input.holds)?;
        world.resource_scope(|world, mut semantic: Mut<SemanticActions>| {
            semantic.restore_keyboard_members(
                world.resource::<ActionMaps>(),
                &staging,
                input.claim.seat(),
                &input.holds,
                &members,
            );
        });
        Ok(admission)
    }))
}

impl __RuntimeDriverPort for ActionStaging {
    type Input = KeyboardContinuationOperation;
    type Output = Result<InputAdmission, KeyboardContinuationError>;

    fn accepts_driver_state(state: RuntimeState) -> bool {
        state == RuntimeState::Paused
    }

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(KeyboardContinuationError::MissingResources)
    }
}
