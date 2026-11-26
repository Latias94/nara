use std::{error::Error, fmt, sync::Arc};

use crate::{InputSeatId, InputWindowId};
use nara_ecs::Resource;

/// The window and seat that receive a controller's input.
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
    pub(crate) emitted_route: Option<GamepadRoute>,
    pub(crate) native: gilrs::GamepadId,
}

impl GilrsDevice {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.token.serial
    }

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
}

impl fmt::Display for GamepadAssignmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gamepad assignment refused: {self:?}")
    }
}

impl Error for GamepadAssignmentError {}

/// Bounded connected-device inventory and explicit per-device assignment owner.
///
/// Changes take effect when the native event queue next drains. Unassignment stays in effect
/// for that connection until explicitly changed.
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
        device.route = route;
        Ok(())
    }
}
