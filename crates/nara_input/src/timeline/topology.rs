//! Fixed live storage for connected Input lifetimes and accepted button members.

use nara_core::Vec2;

use super::{
    HeldInput, InputConfigurationError, InputEventOrigin, InputIngressError, InputLimits,
    InputPointerId, InputSeatId, InputWindowId,
};
use crate::InputBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EndpointId(pub(super) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PointerId(pub(super) u64);

#[derive(Debug, Clone, Copy)]
pub(super) struct Endpoint {
    pub(super) id: EndpointId,
    pub(super) window: InputWindowId,
    pub(super) seat: InputSeatId,
    pub(super) retiring: bool,
    pub(super) focused: bool,
    pub(super) focus_loss: bool,
    pub(super) close: bool,
    pub(super) retirement: bool,
    pub(super) geometry: bool,
    pub(super) last_binding_generation: u64,
}

impl Endpoint {
    pub(super) fn obligations(self) -> usize {
        usize::from(self.focus_loss)
            + usize::from(self.close)
            + usize::from(self.retirement)
            + usize::from(self.geometry)
    }

    pub(super) fn rearmed_obligations(self) -> usize {
        3 + usize::from(self.geometry)
    }

    pub(super) fn rearm(&mut self) {
        self.focus_loss = true;
        self.close = true;
        self.retirement = true;
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Pointer {
    pub(super) id: PointerId,
    pub(super) endpoint: EndpointId,
    pub(super) label: InputPointerId,
    pub(super) retiring: bool,
    pub(super) position: Option<Vec2>,
    pub(super) leave: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Member {
    pub(super) endpoint: EndpointId,
    pub(super) pointer: Option<PointerId>,
    pub(super) input: InputBinding,
    pub(super) pressed: bool,
    pub(super) held: HeldInput,
    pub(super) origin: InputEventOrigin,
}

#[derive(Debug)]
pub(super) struct InputTopology {
    pub(super) endpoints: Box<[Option<Endpoint>]>,
    pub(super) pointers: Box<[Option<Pointer>]>,
    pub(super) members: Box<[Option<Member>]>,
    next_identity: u64,
}

impl InputTopology {
    pub(super) fn new(limits: InputLimits) -> Result<Self, InputConfigurationError> {
        Ok(Self {
            endpoints: slots(limits.endpoints())?,
            pointers: slots(limits.pointers())?,
            members: slots(limits.members())?,
            next_identity: 0,
        })
    }

    pub(super) fn endpoint_index(&self, window: InputWindowId, seat: InputSeatId) -> Option<usize> {
        self.endpoints.iter().position(|slot| {
            slot.is_some_and(|record| record.window == window && record.seat == seat)
        })
    }

    pub(super) fn connected_endpoint(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<usize, InputIngressError> {
        let index = self
            .endpoint_index(window, seat)
            .ok_or(InputIngressError::UnknownEndpoint { window, seat })?;
        if self.endpoints[index].unwrap().retiring {
            return Err(InputIngressError::EndpointRetiring { window, seat });
        }
        Ok(index)
    }

    pub(super) fn pointer_index(
        &self,
        endpoint: EndpointId,
        pointer: InputPointerId,
    ) -> Option<usize> {
        self.pointers.iter().position(|slot| {
            slot.is_some_and(|record| record.endpoint == endpoint && record.label == pointer)
        })
    }

    pub(super) fn connected_pointer(
        &self,
        endpoint_index: usize,
        pointer: InputPointerId,
    ) -> Result<usize, InputIngressError> {
        let endpoint = self.endpoints[endpoint_index].unwrap();
        let index =
            self.pointer_index(endpoint.id, pointer)
                .ok_or(InputIngressError::UnknownPointer {
                    window: endpoint.window,
                    seat: endpoint.seat,
                    pointer,
                })?;
        if self.pointers[index].unwrap().retiring {
            return Err(InputIngressError::PointerRetiring {
                window: endpoint.window,
                seat: endpoint.seat,
                pointer,
            });
        }
        Ok(index)
    }

    pub(super) fn free_endpoint(&self) -> Result<usize, InputIngressError> {
        self.endpoints.iter().position(Option::is_none).ok_or(
            InputIngressError::EndpointCapacityExceeded {
                limit: self.endpoints.len(),
            },
        )
    }

    pub(super) fn free_pointer(&self) -> Result<usize, InputIngressError> {
        self.pointers.iter().position(Option::is_none).ok_or(
            InputIngressError::PointerCapacityExceeded {
                limit: self.pointers.len(),
            },
        )
    }

    pub(super) fn free_member(&self) -> Result<usize, InputIngressError> {
        self.members.iter().position(Option::is_none).ok_or(
            InputIngressError::MemberCapacityExceeded {
                limit: self.members.len(),
            },
        )
    }

    pub(super) fn member_index(
        &self,
        endpoint: EndpointId,
        pointer: Option<PointerId>,
        input: InputBinding,
    ) -> Option<usize> {
        self.members.iter().position(|slot| {
            slot.is_some_and(|record| {
                record.pressed
                    && record.endpoint == endpoint
                    && record.pointer == pointer
                    && record.input == input
            })
        })
    }

    pub(super) fn next_identity(&self) -> Result<u64, InputIngressError> {
        self.next_identity
            .checked_add(1)
            .ok_or(InputIngressError::EndpointGenerationExhausted)
    }

    pub(super) fn install_endpoint(
        &mut self,
        slot: usize,
        identity: u64,
        window: InputWindowId,
        seat: InputSeatId,
    ) {
        debug_assert!(self.endpoints[slot].is_none());
        self.next_identity = identity;
        self.endpoints[slot] = Some(Endpoint {
            id: EndpointId(identity),
            window,
            seat,
            retiring: false,
            focused: false,
            focus_loss: true,
            close: true,
            retirement: true,
            geometry: false,
            last_binding_generation: 0,
        });
    }

    pub(super) fn install_pointer(
        &mut self,
        slot: usize,
        identity: u64,
        endpoint: EndpointId,
        label: InputPointerId,
    ) {
        debug_assert!(self.pointers[slot].is_none());
        self.next_identity = identity;
        self.pointers[slot] = Some(Pointer {
            id: PointerId(identity),
            endpoint,
            label,
            retiring: false,
            position: None,
            leave: false,
        });
    }

    pub(super) fn obligations(&self) -> usize {
        self.endpoints
            .iter()
            .flatten()
            .map(|record| record.obligations())
            .sum::<usize>()
            + self
                .pointers
                .iter()
                .flatten()
                .filter(|record| record.leave)
                .count()
            + self
                .members
                .iter()
                .flatten()
                .filter(|record| record.pressed)
                .count()
    }

    pub(super) fn has_pointer_positions(&self, endpoint: EndpointId) -> bool {
        self.pointers
            .iter()
            .flatten()
            .any(|pointer| pointer.endpoint == endpoint && pointer.position.is_some())
    }
}

fn slots<T>(count: usize) -> Result<Box<[Option<T>]>, InputConfigurationError> {
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(count)
        .map_err(|_| InputConfigurationError::AllocationRefused)?;
    slots.resize_with(count, || None);
    Ok(slots.into_boxed_slice())
}
