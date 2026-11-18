use std::sync::Arc;

use gilrs::{EventType, Gamepad};
use nara_input::{
    AxisValue, ButtonTransitionPhase, GamepadConnection, InputIngressError, InputTimeline,
};

use crate::devices::{GilrsDevice, GilrsDeviceToken};
use crate::{BridgeError, GamepadAxis, GamepadButton, GamepadRoute, GilrsDevices};

#[derive(Clone, Default)]
pub(crate) struct PhysicalState {
    pub buttons: [bool; 19],
    pub axes: [AxisValue; 8],
}

impl PhysicalState {
    pub fn snapshot(gamepad: &Gamepad<'_>) -> Result<Self, BridgeError> {
        let mut state = Self::default();
        for &button in GamepadButton::ALL {
            state.buttons[button.code() as usize] = gamepad.is_pressed(button.native());
        }
        for &axis in GamepadAxis::ALL {
            state.axes[axis.code() as usize] =
                AxisValue::new(gamepad.value(axis.native())).map_err(BridgeError::Axis)?;
        }
        Ok(state)
    }
}

struct ActiveConnection {
    connection: GamepadConnection,
    route: GamepadRoute,
    revision: u64,
    incarnation: u64,
    invalidated_through: u64,
}

struct DeviceState {
    id: usize,
    label: u16,
    serial: u64,
    physical: PhysicalState,
    blocked_buttons: [bool; 19],
    blocked_axes: [bool; 8],
    active: Option<ActiveConnection>,
}

impl DeviceState {
    fn suppress_held(&mut self) {
        self.blocked_buttons = self.physical.buttons;
        self.blocked_axes = self.physical.axes.map(|v| v != AxisValue::ZERO);
    }

    fn disconnect(&mut self, timeline: &mut InputTimeline) -> Result<(), BridgeError> {
        if let Some(active) = &self.active {
            if timeline
                .endpoint_observation(active.route.window, active.route.seat)
                .is_some_and(|o| o.incarnation() == active.incarnation)
            {
                timeline.disconnect_gamepad(&active.connection)?;
            }
        }
        self.active = None;
        self.suppress_held();
        Ok(())
    }
}

pub(crate) struct Bridge {
    pub owner: Arc<()>,
    states: Vec<DeviceState>,
    serial: u64,
    max_devices: usize,
    default_route: Option<GamepadRoute>,
}

impl Bridge {
    pub fn new(max_devices: usize, default_route: Option<GamepadRoute>) -> (Self, GilrsDevices) {
        let owner = Arc::new(());
        let devices = GilrsDevices {
            owner: Arc::clone(&owner),
            devices: Vec::with_capacity(max_devices),
            closed: false,
        };
        (
            Self {
                owner,
                states: Vec::with_capacity(max_devices),
                serial: 0,
                max_devices,
                default_route,
            },
            devices,
        )
    }

    pub fn check_inventory(&self, devices: &GilrsDevices) -> Result<(), BridgeError> {
        if !Arc::ptr_eq(&self.owner, &devices.owner) || devices.closed {
            return Err(BridgeError::InventoryReplaced);
        }
        Ok(())
    }

    pub fn connected(
        &mut self,
        id: usize,
        name: &str,
        physical: PhysicalState,
        devices: &mut GilrsDevices,
    ) -> Result<(), BridgeError> {
        if self.states.iter().any(|state| state.id == id) {
            return Ok(());
        }
        if self.states.len() == self.max_devices {
            return Err(BridgeError::DeviceCapacity);
        }
        let label = (0..self.max_devices)
            .find(|label| {
                self.states
                    .iter()
                    .all(|state| usize::from(state.label) != *label)
            })
            .expect("a bounded device slot is available") as u16;
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or(BridgeError::IdentityExhausted)?;
        devices.devices.push(GilrsDevice {
            token: GilrsDeviceToken {
                owner: Arc::clone(&self.owner),
                serial: self.serial,
            },
            name: name.chars().take(128).collect(),
            route: self.default_route,
            revision: 0,
        });
        self.states.push(DeviceState {
            id,
            label,
            serial: self.serial,
            physical,
            blocked_buttons: [false; 19],
            blocked_axes: [false; 8],
            active: None,
        });
        Ok(())
    }

    pub fn disconnected(
        &mut self,
        id: usize,
        devices: &mut GilrsDevices,
        timeline: &mut InputTimeline,
    ) -> Result<(), BridgeError> {
        if let Some(index) = self.states.iter().position(|s| s.id == id) {
            self.states[index].disconnect(timeline)?;
            let serial = self.states.remove(index).serial;
            devices.devices.retain(|d| d.token.serial != serial);
        }
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        devices: &GilrsDevices,
        timeline: &mut InputTimeline,
        drained: bool,
    ) -> Result<(), BridgeError> {
        self.check_inventory(devices)?;
        for state in &mut self.states {
            let device = devices
                .devices
                .iter()
                .find(|d| d.token.serial == state.serial)
                .ok_or(BridgeError::InventoryReplaced)?;
            let observation = device
                .route
                .and_then(|r| timeline.endpoint_observation(r.window, r.seat));
            if let Some(active) = &state.active {
                if device.revision != active.revision
                    || !observation.is_some_and(|o| {
                        o.focused()
                            && o.incarnation() == active.incarnation
                            && o.invalidated_through() == active.invalidated_through
                    })
                {
                    state.disconnect(timeline)?;
                }
            }
            if drained && state.active.is_none() {
                if let Some((route, observation)) =
                    device.route.zip(observation).filter(|(_, o)| o.focused())
                {
                    state.suppress_held();
                    let connection =
                        match timeline.connect_gamepad(route.window, route.seat, state.label) {
                            Ok(connection) => connection,
                            Err(InputIngressError::GamepadRetirementPending { .. }) => continue,
                            Err(error) => return Err(error.into()),
                        };
                    state.active = Some(ActiveConnection {
                        connection,
                        route,
                        revision: device.revision,
                        incarnation: observation.incarnation(),
                        invalidated_through: observation.invalidated_through(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn event(
        &mut self,
        id: usize,
        event: EventType,
        timeline: &mut InputTimeline,
    ) -> Result<(), BridgeError> {
        let Some(state) = self.states.iter_mut().find(|s| s.id == id) else {
            return Ok(());
        };
        match event {
            EventType::ButtonPressed(button, _) | EventType::ButtonReleased(button, _) => {
                let Some(button) = GamepadButton::from_native(button) else {
                    return Ok(());
                };
                let pressed = matches!(event, EventType::ButtonPressed(..));
                let index = button.code() as usize;
                state.physical.buttons[index] = pressed;
                if !pressed && state.blocked_buttons[index] {
                    state.blocked_buttons[index] = false;
                    return Ok(());
                }
                if !state.blocked_buttons[index] {
                    if let Some(active) = &state.active {
                        timeline.admit_gamepad_button(
                            &active.connection,
                            button.code(),
                            if pressed {
                                ButtonTransitionPhase::Pressed
                            } else {
                                ButtonTransitionPhase::Released
                            },
                        )?;
                    }
                }
            }
            EventType::AxisChanged(axis, raw, _) => {
                let Some(axis) = GamepadAxis::from_native(axis) else {
                    return Ok(());
                };
                let value = AxisValue::new(raw).map_err(BridgeError::Axis)?;
                let index = axis.code() as usize;
                state.physical.axes[index] = value;
                if value == AxisValue::ZERO && state.blocked_axes[index] {
                    state.blocked_axes[index] = false;
                    return Ok(());
                }
                if !state.blocked_axes[index] {
                    if let Some(active) = &state.active {
                        timeline.admit_gamepad_axis(&active.connection, axis.code(), value)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn close(&mut self, timeline: &mut InputTimeline) -> Result<(), BridgeError> {
        let mut first = None;
        for state in &mut self.states {
            if let Err(error) = state.disconnect(timeline) {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
}
