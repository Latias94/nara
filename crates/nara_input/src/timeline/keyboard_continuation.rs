use super::*;
use crate::{KeyboardActionBinding, KeyboardContinuationError, KeyboardHold};

impl InputTimeline {
    pub(crate) fn reconcile_keyboard_members(
        &mut self,
        claim: &InputEndpointClaim,
        released: &[KeyCode],
    ) -> Result<InputAdmission, KeyboardContinuationError> {
        if !self.owns_claim(claim)? {
            return Err(KeyboardContinuationError::StaleEndpoint);
        }
        if released.len() > self.limits.members() {
            return Err(KeyboardContinuationError::InvalidCount);
        }
        if !self.retained.is_empty() {
            return Err(KeyboardContinuationError::PendingInput);
        }
        self.topology.connected_endpoint(claim.window, claim.seat)?;
        let mut slots = Vec::with_capacity(released.len());
        for (offset, key) in released.iter().copied().enumerate() {
            if released[..offset].contains(&key) {
                return Err(KeyboardContinuationError::DuplicateBinding);
            }
            let index = self
                .topology
                .member_index(claim.endpoint, None, None, InputBinding::Key(key))
                .ok_or(KeyboardContinuationError::ProjectionMismatch)?;
            let member = self.topology.members[index].unwrap();
            if member.held.disposition != HeldInputDisposition::Gameplay {
                return Err(KeyboardContinuationError::UnsupportedMembers);
            }
            slots.push(index);
        }
        let drafts = slots
            .iter()
            .map(|index| {
                let mut draft = self.release_draft(
                    self.topology.members[*index].unwrap(),
                    Some(InputInvalidationReason::ActivationMismatch),
                );
                draft.origin = InputEventOrigin::Platform;
                draft
            })
            .collect::<Vec<_>>();
        let plan = self.terminal_plan(drafts.len(), self.topology.obligations() - drafts.len());
        let admission = self.publish(plan, &drafts);
        for index in slots {
            self.topology.members[index].as_mut().unwrap().pressed = false;
        }
        self.assert_accounting();
        Ok(admission)
    }

    pub(crate) fn capture_keyboard_members(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        bindings: &[KeyboardActionBinding],
    ) -> Result<Vec<Option<u64>>, KeyboardContinuationError> {
        if !self.retained.is_empty() {
            return Err(KeyboardContinuationError::PendingInput);
        }
        let index = self.topology.connected_endpoint(window, seat)?;
        let endpoint = self.topology.endpoints[index].unwrap();
        if self.routes.contains_key(&(window, seat))
            || self.prepared_routes.contains_key(&(window, seat))
        {
            return Err(KeyboardContinuationError::UnsupportedMembers);
        }
        let mut result = vec![None; bindings.len()];
        for member in self
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
        {
            let owner = self
                .topology
                .endpoints
                .iter()
                .flatten()
                .find(|endpoint| endpoint.id == member.endpoint)
                .expect("member retains its endpoint");
            if owner.seat != seat {
                continue;
            }
            let InputBinding::Key(key) = member.input else {
                return Err(KeyboardContinuationError::UnsupportedMembers);
            };
            if member.endpoint != endpoint.id
                || member.pointer.is_some()
                || member.gamepad.is_some()
                || member.held.disposition != HeldInputDisposition::Gameplay
            {
                return Err(KeyboardContinuationError::UnsupportedMembers);
            }
            let slot = bindings
                .iter()
                .position(|binding| binding.key == key)
                .ok_or(KeyboardContinuationError::UnsupportedMembers)?;
            if result[slot].replace(member.held.press_sequence).is_some() {
                return Err(KeyboardContinuationError::UnsupportedMembers);
            }
        }
        Ok(result)
    }

    pub(crate) fn restore_keyboard_members(
        &mut self,
        claim: &InputEndpointClaim,
        holds: &[KeyboardHold],
    ) -> Result<(InputAdmission, Vec<Option<u64>>), KeyboardContinuationError> {
        if !self.owns_claim(claim)? {
            return Err(KeyboardContinuationError::StaleEndpoint);
        }
        let bindings = holds
            .iter()
            .map(|hold| hold.binding.clone())
            .collect::<Vec<_>>();
        let before = self.capture_keyboard_members(claim.window, claim.seat, &bindings)?;
        if before.iter().any(Option::is_some) {
            return Err(KeyboardContinuationError::MembersPresent);
        }
        let endpoint_index = self.topology.connected_endpoint(claim.window, claim.seat)?;
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let count = holds.iter().filter(|hold| hold.held).count();
        let slots = self
            .topology
            .members
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.is_none().then_some(index))
            .take(count)
            .collect::<Vec<_>>();
        if slots.len() != count {
            return Err(InputIngressError::MemberCapacityExceeded {
                limit: self.limits.members(),
            }
            .into());
        }
        let after = if count == 0 {
            self.topology.obligations()
        } else {
            self.topology.obligations() - endpoint.obligations()
                + endpoint.rearmed_obligations()
                + count
        };
        let plan = self.plan(count, after)?;
        let drafts = holds
            .iter()
            .filter(|hold| hold.held)
            .map(|hold| {
                self.ordinary_draft(
                    claim.window,
                    claim.seat,
                    None,
                    InputEventKind::KeyRestored {
                        key: hold.binding.key,
                    },
                    InputEventOrigin::Restored,
                )
            })
            .collect::<Vec<_>>();
        let admission = self.publish(plan, &drafts);
        if count > 0 {
            self.topology.endpoints[endpoint_index]
                .as_mut()
                .unwrap()
                .rearm();
        }
        let mut admitted = slots.into_iter().zip(drafts).enumerate();
        let mut members = Vec::with_capacity(holds.len());
        for hold in holds {
            if !hold.held {
                members.push(None);
                continue;
            }
            let (offset, (slot, draft)) = admitted
                .next()
                .expect("batch admission preflights every member");
            let sequence = admission
                .first_sequence()
                .expect("nonempty admission has an identity")
                + offset as u64;
            self.topology.members[slot] = Some(Member {
                endpoint: endpoint.id,
                pointer: None,
                gamepad: None,
                input: InputBinding::Key(hold.binding.key),
                pressed: true,
                axis_value: None,
                held: HeldInput::new(sequence, draft.route),
                origin: InputEventOrigin::Restored,
            });
            members.push(Some(sequence));
        }
        self.assert_accounting();
        Ok((admission, members))
    }
}
