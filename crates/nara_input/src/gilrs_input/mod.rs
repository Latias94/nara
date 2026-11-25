//! Optional native gamepad ingress through the original Nara Input timeline.

use std::{cell::RefCell, error::Error, fmt, rc::Rc, sync::Arc};

use crate::{AxisValueError, INPUT_PLUGIN_ID, InputIngressError, InputSet, InputTimeline};
use gilrs::{
    EventType, Filter, Gilrs, GilrsBuilder,
    ev::filter::{axis_dpad_to_button, deadzone},
};
use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    RuntimeFaultDetail,
};
use nara_ecs::bevy_ecs::error::BevyError;
use nara_ecs::{NonSendMut, ResMut, schedule::IntoScheduleConfigs};

mod bridge;
mod controls;
mod devices;

use bridge::{Bridge, PhysicalState};
pub use devices::{
    GamepadAssignmentError, GamepadRoute, GilrsDevice, GilrsDeviceToken, GilrsDevices,
};

pub const GILRS_INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.gilrs");
const CLOSE: PluginShutdownObligationId = PluginShutdownObligationId::new("nara.gilrs.native");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(GILRS_INPUT_PLUGIN_ID, PluginCategory::Input)
        .requires_plugins(&[INPUT_PLUGIN_ID])
        .shutdown_obligations(&[CLOSE]);

/// Concrete optional native backend. Construction stores configuration without acquiring Gilrs.
///
/// Polling consumes at most `max_events_per_frame` mapped events, including filter-generated
/// and dropped events. Defaults admit 16 connected devices and 256 events per frame.
#[derive(Debug, Clone, Copy)]
pub struct GilrsInputPlugin {
    pub default_route: Option<GamepadRoute>,
    pub max_devices: usize,
    pub max_events_per_frame: usize,
}

impl GilrsInputPlugin {
    #[must_use]
    pub const fn new(default_route: Option<GamepadRoute>) -> Self {
        Self {
            default_route,
            max_devices: 16,
            max_events_per_frame: 256,
        }
    }
}

impl Default for GilrsInputPlugin {
    fn default() -> Self {
        Self::new(None)
    }
}

#[derive(Debug)]
enum BridgeError {
    Ingress(InputIngressError),
    Axis(AxisValueError),
    DeviceCapacity,
    IdentityExhausted,
    InventoryReplaced,
}

impl From<InputIngressError> for BridgeError {
    fn from(value: InputIngressError) -> Self {
        Self::Ingress(value)
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingress(error) => error.fmt(f),
            Self::Axis(error) => error.fmt(f),
            _ => write!(f, "native gamepad bridge failed: {self:?}"),
        }
    }
}

impl Error for BridgeError {}

struct NativeState {
    gilrs: Option<Gilrs>,
    bridge: Bridge,
    budget: usize,
    #[cfg(test)]
    drop_probe: Option<tests::DropProbe>,
}

struct NativeGilrs(Rc<RefCell<NativeState>>);
struct NativeClose(Rc<RefCell<NativeState>>);

impl Plugin for GilrsInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let setup = |message: String| PluginError::SetupFailed {
            plugin: GILRS_INPUT_PLUGIN_ID,
            message,
        };
        if !(1..=64).contains(&self.max_devices) || !(1..=4096).contains(&self.max_events_per_frame)
        {
            return Err(setup(
                "Gilrs limits require 1..=64 devices and 1..=4096 events per frame".into(),
            ));
        }
        let (bridge, mut devices) = Bridge::new(self.max_devices, self.default_route);
        let state = Rc::new(RefCell::new(NativeState {
            gilrs: None,
            bridge,
            budget: self.max_events_per_frame,
            #[cfg(test)]
            drop_probe: None,
        }));
        app.register_plugin_runtime_close_participant(
            CLOSE,
            RuntimeCloseParticipantId::new("nara.gilrs.native"),
            NativeClose(Rc::clone(&state)),
        )?;
        {
            let mut native = state.borrow_mut();
            match GilrsBuilder::new()
                .with_force_feedback(false)
                .with_default_filters(false)
                .set_update_state(false)
                .build()
            {
                Ok(gilrs) => native.gilrs = Some(gilrs),
                Err(gilrs::Error::NotImplemented(gilrs)) => {
                    native.gilrs = Some(gilrs);
                    return Err(setup("Gilrs does not support this native platform".into()));
                }
                Err(error) => return Err(setup(error.to_string())),
            }
            let NativeState { gilrs, bridge, .. } = &mut *native;
            for (id, gamepad) in gilrs
                .as_ref()
                .unwrap()
                .gamepads()
                .take(self.max_devices + 1)
            {
                bridge
                    .connected(
                        id.into(),
                        gamepad.name(),
                        PhysicalState::snapshot(&gamepad).map_err(|e| setup(e.to_string()))?,
                        &mut devices,
                    )
                    .map_err(|e| setup(e.to_string()))?;
            }
        }
        app.insert_resource(devices)?;
        app.world_mut()?.insert_non_send(NativeGilrs(state));
        app.add_systems(
            CoreStage::First,
            poll_native.before(InputSet::ProjectTimeline),
        )?;
        Ok(())
    }
}

impl NativeState {
    fn poll(
        &mut self,
        devices: &mut GilrsDevices,
        timeline: &mut InputTimeline,
    ) -> Result<bool, BridgeError> {
        self.bridge.reconcile(devices, timeline, false)?;
        let Some(gilrs) = &mut self.gilrs else {
            return Err(BridgeError::InventoryReplaced);
        };
        let mut drained = false;
        let mut refused_action = false;
        for _ in 0..self.budget {
            let Some(raw) = gilrs.next_event() else {
                drained = true;
                break;
            };
            let dpad = Some(raw).filter_ev(&axis_dpad_to_button, gilrs).unwrap();
            let filtered = Some(dpad).filter_ev(&deadzone, gilrs).unwrap();
            gilrs.update(&filtered);
            let id = usize::from(filtered.id);
            let result = match filtered.event {
                EventType::Connected => {
                    let gamepad = gilrs.gamepad(filtered.id);
                    self.bridge.connected(
                        id,
                        gamepad.name(),
                        PhysicalState::snapshot(&gamepad)?,
                        devices,
                    )
                }
                EventType::Disconnected => self.bridge.disconnected(id, devices, timeline),
                EventType::Dropped => match dpad.event {
                    EventType::AxisChanged(axis, _, code) => self.bridge.event(
                        id,
                        EventType::AxisChanged(
                            axis,
                            gilrs.gamepad(filtered.id).state().value(code),
                            code,
                        ),
                        timeline,
                    ),
                    _ => Ok(()),
                },
                event => self.bridge.event(id, event, timeline),
            };
            match result {
                Err(BridgeError::Ingress(InputIngressError::ActionOutputAllocationRefused)) => {
                    refused_action = true;
                }
                other => other?,
            }
        }
        gilrs.inc();
        self.bridge.reconcile(devices, timeline, drained)?;
        Ok(refused_action)
    }
}

fn poll_native(
    native: NonSendMut<NativeGilrs>,
    mut devices: ResMut<GilrsDevices>,
    mut timeline: ResMut<InputTimeline>,
) -> Result<(), BevyError> {
    let refused_action = native
        .0
        .borrow_mut()
        .poll(&mut devices, &mut timeline)
        .map_err(|error| {
            let (code, summary) = match error {
                BridgeError::Ingress(_) => (
                    "gilrs.input-ingress",
                    "Native gamepad input could not be admitted",
                ),
                BridgeError::Axis(_) => (
                    "gilrs.invalid-axis",
                    "Native gamepad axis was not normalized",
                ),
                BridgeError::DeviceCapacity => (
                    "gilrs.device-capacity",
                    "Native gamepad device capacity was exceeded",
                ),
                BridgeError::IdentityExhausted => (
                    "gilrs.identity-exhausted",
                    "Native gamepad connection identity was exhausted",
                ),
                BridgeError::InventoryReplaced => (
                    "gilrs.inventory-replaced",
                    "Native gamepad inventory was replaced or closed",
                ),
            };
            RuntimeFaultDetail::new(code, summary, "nara_input")
                .expect("static Gilrs fault metadata is valid")
                .into_bevy_error()
        })?;
    if refused_action {
        return Err(BevyError::warning(
            InputIngressError::ActionOutputAllocationRefused,
        ));
    }
    Ok(())
}

impl NativeClose {
    fn close(&mut self, context: &mut RuntimeCloseContext<'_>) -> RuntimeCloseProgress {
        let mut native = self.0.borrow_mut();
        let result = context
            .resource_mut::<InputTimeline>()
            .map(|mut timeline| native.bridge.close(&mut timeline));
        drop(native.gilrs.take());
        #[cfg(test)]
        drop(native.drop_probe.take());
        if let Ok(Some(mut devices)) = context.get_resource_mut::<GilrsDevices>() {
            if Arc::ptr_eq(&devices.owner, &native.bridge.owner) {
                devices.devices.clear();
                devices.closed = true;
            }
        }
        match result {
            Ok(Ok(())) => RuntimeCloseProgress::Complete,
            _ => RuntimeCloseProgress::CompleteWithError {
                code: "gilrs.input-cleanup",
            },
        }
    }
}

impl RuntimeCloseParticipant for NativeClose {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.close(context))
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.close(context))
    }
}

#[cfg(test)]
mod tests;
