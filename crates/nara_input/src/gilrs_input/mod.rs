//! Native controller polling for standalone desktop games.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use gilrs::{
    EventType, Filter, GamepadId, Gilrs, GilrsBuilder,
    ev::filter::{axis_dpad_to_button, deadzone},
};
use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeFaultDetail,
};
use nara_ecs::{
    NonSendMut, ResMut,
    bevy_ecs::{error::BevyError, message::Messages},
    schedule::IntoScheduleConfigs,
};

use crate::{
    AxisValue, ButtonTransitionPhase, GamepadInputKind, INPUT_PLUGIN_ID, InputEdge, InputEdgeKind,
    InputSeatId, InputSet, InputWindowId,
};

mod controls;
mod devices;

use controls::{axis_from_native, button_from_native};
pub use devices::{
    GamepadAssignmentError, GamepadRoute, GilrsDevice, GilrsDeviceToken, GilrsDevices,
};

pub const GILRS_INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.gilrs");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(GILRS_INPUT_PLUGIN_ID, PluginCategory::Input)
        .requires_plugins(&[INPUT_PLUGIN_ID]);

/// Polls Gilrs before input projection. The default assigns controllers to the primary game seat.
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
        Self::new(Some(GamepadRoute::new(
            InputWindowId::PRIMARY,
            InputSeatId::PRIMARY,
        )))
    }
}

struct Native {
    gilrs: Option<Gilrs>,
    owner: Arc<()>,
    next_id: u64,
    started: bool,
    config: GilrsInputPlugin,
}

#[derive(Clone)]
struct NativeGilrs(Rc<RefCell<Native>>);

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
                "Gilrs requires 1..=64 devices and 1..=4096 events per frame".into(),
            ));
        }
        let gilrs = GilrsBuilder::new()
            .with_force_feedback(false)
            .with_default_filters(false)
            .set_update_state(false)
            .build()
            .map_err(|error| setup(error.to_string()))?;
        let owner = Arc::new(());
        let mut devices = GilrsDevices {
            owner: Arc::clone(&owner),
            devices: Vec::new(),
            closed: false,
        };
        let mut next_id = 1;
        for (id, gamepad) in gilrs.gamepads() {
            connect(&mut devices, self, &mut next_id, id, gamepad.name())
                .map_err(|error| setup(error.into()))?;
        }
        let native = NativeGilrs(Rc::new(RefCell::new(Native {
            gilrs: Some(gilrs),
            owner,
            next_id,
            started: false,
            config: *self,
        })));
        app.register_close_participant(
            RuntimeCloseParticipantId::new("nara.gilrs.native"),
            native.clone(),
        )?;
        app.insert_resource(devices)?;
        app.world_mut()?.insert_non_send(native);
        app.add_systems(CoreStage::First, poll_native.before(InputSet::ProjectInput))?;
        Ok(())
    }
}

fn connect(
    devices: &mut GilrsDevices,
    config: &GilrsInputPlugin,
    next_id: &mut u64,
    native: GamepadId,
    name: &str,
) -> Result<usize, &'static str> {
    if let Some(index) = devices
        .devices
        .iter()
        .position(|device| device.native == native)
    {
        return Ok(index);
    }
    if devices.devices.len() >= config.max_devices {
        return Err("Native gamepad device capacity was exceeded");
    }
    let serial = *next_id;
    *next_id = next_id
        .checked_add(1)
        .ok_or("Native gamepad connection ID was exhausted")?;
    devices.devices.push(GilrsDevice {
        token: GilrsDeviceToken {
            owner: Arc::clone(&devices.owner),
            serial,
        },
        name: name.chars().take(128).collect(),
        route: config.default_route,
        emitted_route: None,
        native,
    });
    Ok(devices.devices.len() - 1)
}

fn send(
    output: &mut Messages<InputEdge>,
    route: GamepadRoute,
    gamepad: u64,
    kind: GamepadInputKind,
) {
    output.write(InputEdge::new(
        route.window,
        route.seat,
        InputEdgeKind::Gamepad { gamepad, kind },
    ));
}

fn update_route(output: &mut Messages<InputEdge>, device: &mut GilrsDevice) {
    if device.route == device.emitted_route {
        return;
    }
    if let Some(route) = device.emitted_route {
        send(output, route, device.id(), GamepadInputKind::Disconnected);
    }
    if let Some(route) = device.route {
        send(output, route, device.id(), GamepadInputKind::Connected);
    }
    device.emitted_route = device.route;
}

fn gamepad_sample(event: EventType) -> Option<GamepadInputKind> {
    match event {
        EventType::ButtonPressed(button, _) => Some(GamepadInputKind::Button {
            button: button_from_native(button)?,
            phase: ButtonTransitionPhase::Pressed,
        }),
        EventType::ButtonReleased(button, _) => Some(GamepadInputKind::Button {
            button: button_from_native(button)?,
            phase: ButtonTransitionPhase::Released,
        }),
        EventType::AxisChanged(axis, value, _) => Some(GamepadInputKind::Axis {
            axis: axis_from_native(axis)?,
            value: AxisValue::new(value).ok()?,
        }),
        _ => None,
    }
}

fn poll_native(
    native: NonSendMut<NativeGilrs>,
    mut devices: ResMut<GilrsDevices>,
    mut output: ResMut<Messages<InputEdge>>,
) -> Result<(), BevyError> {
    let Native {
        gilrs,
        owner,
        next_id,
        started,
        config,
    } = &mut *native.0.borrow_mut();
    let fault = |summary| {
        RuntimeFaultDetail::new("gilrs.input", summary, "nara_input")
            .expect("static Gilrs fault metadata is valid")
            .into_bevy_error()
    };
    let gilrs = gilrs
        .as_mut()
        .ok_or_else(|| fault("Native gamepad polling has ended"))?;
    if !Arc::ptr_eq(owner, &devices.owner) {
        return Err(fault("Native gamepad inventory was replaced"));
    }
    if !*started {
        for device in &mut devices.devices {
            update_route(&mut output, device);
        }
        *started = true;
    }
    for _ in 0..config.max_events_per_frame {
        let Some(raw) = gilrs.next_event() else {
            for device in &mut devices.devices {
                update_route(&mut output, device);
            }
            break;
        };
        let Some(event) = Some(raw)
            .filter_ev(&axis_dpad_to_button, gilrs)
            .filter_ev(&deadzone, gilrs)
        else {
            continue;
        };
        gilrs.update(&event);
        match event.event {
            EventType::Connected => {
                let index = connect(
                    &mut devices,
                    config,
                    next_id,
                    event.id,
                    gilrs.gamepad(event.id).name(),
                )
                .map_err(fault)?;
                update_route(&mut output, &mut devices.devices[index]);
            }
            EventType::Disconnected => {
                if let Some(index) = devices
                    .devices
                    .iter()
                    .position(|device| device.native == event.id)
                {
                    let device = devices.devices.remove(index);
                    if let Some(route) = device.emitted_route {
                        send(
                            &mut output,
                            route,
                            device.id(),
                            GamepadInputKind::Disconnected,
                        );
                    }
                }
            }
            sample => {
                if let Some(kind) = gamepad_sample(sample)
                    && let Some(device) = devices
                        .devices
                        .iter()
                        .find(|device| device.native == event.id)
                    && let Some(route) = device.emitted_route
                {
                    send(&mut output, route, device.id(), kind);
                }
            }
        }
    }
    gilrs.inc();
    Ok(())
}

impl RuntimeCloseParticipant for NativeGilrs {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let mut native = self.0.borrow_mut();
        drop(native.gilrs.take());
        if let Ok(Some(mut devices)) = context.get_resource_mut::<GilrsDevices>()
            && Arc::ptr_eq(&devices.owner, &native.owner)
        {
            devices.devices.clear();
            devices.closed = true;
        }
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begin_close(context)
    }
}

#[cfg(all(test, windows))]
#[test]
#[ignore = "requires Windows Gaming Input on the host"]
fn native_gilrs_initializes_and_closes_on_its_owner() {
    use std::time::Duration;

    use nara_app::RuntimeInstance;

    let mut app = App::new();
    app.add_plugin(crate::InputPlugin).unwrap();
    app.add_plugin(GilrsInputPlugin::default()).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime.finish().unwrap();
}
