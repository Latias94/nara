use std::time::Duration;

use nara_app::{
    App, CoreStage, RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::__private::InputSet;
use nara_input::{
    ActionId, ActionMap, ActionOutcomes, ButtonDriverInput, InputEventKind, InputFrameObservations,
    InputGeometryGeneration, InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration,
    InputSeatId, InputSlotGeneration, InputSurfaceId, InputTimeline, InputWindowEvent,
    InputWindowId, KeyCode, apply_keyboard_driver_input_at, apply_window_input_driver_event,
};

#[derive(Debug, Default, Resource)]
struct ObservedSequences(Vec<u64>);

#[derive(Debug, Default, Resource)]
struct ObservedActions(usize);

fn consume_enter_for_ui(mut timeline: ResMut<InputTimeline>) {
    let consumed = timeline
        .pending_ui_entries()
        .filter_map(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    ..
                }
            )
            .then_some(entry.sequence())
        })
        .collect::<Vec<_>>();
    for sequence in consumed {
        timeline
            .__finalize_ui_entry(sequence, nara_input::InputUiDisposition::Consumed)
            .unwrap();
    }
}

fn capture_observations(
    observations: Res<InputFrameObservations>,
    mut captured: ResMut<ObservedSequences>,
) {
    captured.0 = observations
        .as_slice()
        .iter()
        .map(|entry| entry.sequence())
        .collect();
}

fn capture_actions(outcomes: Res<ActionOutcomes>, mut captured: ResMut<ObservedActions>) {
    captured.0 += outcomes.as_slice().len();
}

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn stop_runtime(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    if runtime.state() == RuntimeState::Stopping {
        runtime.drive(Duration::ZERO).unwrap();
    }
}

#[test]
fn paused_ui_consumption_is_observed_once_and_skipped_by_gameplay_on_resume() {
    let window = InputWindowId::new(1);
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(ObservedSequences::default())
        .unwrap()
        .insert_resource(ObservedActions::default())
        .unwrap()
        .add_systems(
            CoreStage::First,
            consume_enter_for_ui.in_set(InputSet::RouteUi),
        )
        .unwrap()
        .add_systems(CoreStage::Extract, capture_observations)
        .unwrap()
        .add_systems(CoreStage::Update, capture_actions)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
    app.world_mut()
        .unwrap()
        .resource_mut::<InputTimeline>()
        .publish_presented_route(InputPresentedRoute::new(
            window,
            InputSurfaceId::new(9),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(17).unwrap(),
            InputSlotGeneration::new(3).unwrap(),
            InputRouteGeneration::new(4).unwrap(),
            InputGeometryGeneration::new(5).unwrap(),
        ))
        .unwrap();

    let mut runtime = start_runtime(app);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                window,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(true),
            )?;
            apply_keyboard_driver_input_at(
                scope,
                window,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Enter),
            )?;
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(runtime.world().resource::<ObservedSequences>().0, [1, 2]);
    assert_eq!(runtime.world().resource::<ObservedActions>().0, 0);
    let cursors = runtime.world().resource::<InputTimeline>().cursors();
    assert_eq!(cursors.observation(), 2);
    assert_eq!(cursors.ui(), 2);
    assert_eq!(cursors.action_scanned(), 2);
    assert_eq!(cursors.oldest_deferred_action(), Some(2));

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.world().resource::<ObservedActions>().0, 0);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    stop_runtime(&mut runtime);
}
