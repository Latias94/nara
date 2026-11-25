use std::{error::Error, fmt, sync::Arc};

use crate::{InputSeatId, InputWindowId};
use nara_ecs::Resource;

/// An explicit existing window and seat; this value creates neither endpoint nor seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamepadRoute {
    pub window: InputWindowId,
    pub seat: InputSeatId,
}

impl GamepadRoute {
    #[must_use]
    pub const fn new(window: InputWindowId, seat: InputSeatId) -> Self {
        Self { window, seat }
    }
}

/// Identifies one current native connection in its original inventory.
///
/// Tokens have no constructor, serialization or persistent hardware identity. A reconnect,
/// including one with the same native label, produces a different token.
#[derive(Debug, Clone)]
pub struct GilrsDeviceToken {
    pub(crate) owner: Arc<()>,
    pub(crate) serial: u64,
}

impl PartialEq for GilrsDeviceToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.serial == other.serial
    }
}

impl Eq for GilrsDeviceToken {}

/// Read-only metadata for a currently connected native device.
#[derive(Debug)]
pub struct GilrsDevice {
    pub(crate) token: GilrsDeviceToken,
    pub(crate) name: String,
    pub(crate) route: Option<GamepadRoute>,
    pub(crate) revision: u64,
}

impl GilrsDevice {
    #[must_use]
    pub fn token(&self) -> GilrsDeviceToken {
        self.token.clone()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn route(&self) -> Option<GamepadRoute> {
        self.route
    }
}

/// A refused assignment leaves the original device route unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadAssignmentError {
    ForeignToken,
    StaleToken,
    Closed,
    RevisionExhausted,
}

impl fmt::Display for GamepadAssignmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gamepad assignment refused: {self:?}")
    }
}

impl Error for GamepadAssignmentError {}

/// Bounded connected-device inventory and explicit per-device assignment owner.
///
/// Changes take effect at the next native poll after queued events drain. A route is intent;
/// only the original focused Input endpoint can admit a gamepad connection. Unassignment is
/// retained for this connection and does not fall back to the plugin's default route.
#[derive(Debug, Resource)]
pub struct GilrsDevices {
    pub(crate) owner: Arc<()>,
    pub(crate) devices: Vec<GilrsDevice>,
    pub(crate) closed: bool,
}

impl GilrsDevices {
    #[must_use]
    pub fn connected(&self) -> &[GilrsDevice] {
        &self.devices
    }

    pub fn assign(
        &mut self,
        token: &GilrsDeviceToken,
        route: GamepadRoute,
    ) -> Result<(), GamepadAssignmentError> {
        self.set_route(token, Some(route))
    }

    pub fn unassign(&mut self, token: &GilrsDeviceToken) -> Result<(), GamepadAssignmentError> {
        self.set_route(token, None)
    }

    fn set_route(
        &mut self,
        token: &GilrsDeviceToken,
        route: Option<GamepadRoute>,
    ) -> Result<(), GamepadAssignmentError> {
        if !Arc::ptr_eq(&self.owner, &token.owner) {
            return Err(GamepadAssignmentError::ForeignToken);
        }
        if self.closed {
            return Err(GamepadAssignmentError::Closed);
        }
        let device = self
            .devices
            .iter_mut()
            .find(|d| d.token == *token)
            .ok_or(GamepadAssignmentError::StaleToken)?;
        if device.route != route {
            let revision = device
                .revision
                .checked_add(1)
                .ok_or(GamepadAssignmentError::RevisionExhausted)?;
            device.route = route;
            device.revision = revision;
        }
        Ok(())
    }
}
