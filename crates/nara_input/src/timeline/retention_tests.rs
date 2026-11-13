use std::time::Duration;

use nara_app::{App, CoreStage, RuntimeInstance};
use nara_core::{ByteLimit, Vec2};
use nara_ecs::{Res, ResMut, Resource};

use super::*;
use crate::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionPhase, ButtonDriverInput, ButtonInput, ButtonInputDriverError, InputPlugin,
    KeyCode, MouseButton, PointerDriverInput, SemanticActions, apply_keyboard_driver_input_at,
    apply_mouse_driver_input_at, apply_window_input_driver_event, connect_input_endpoint,
    connect_input_pointer, retire_input_pointer, semantic::ActionStaging,
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const BUTTON: MouseButton = MouseButton::Left;

#[derive(Default, Resource)]
struct DeliveredEdges {
    starts: usize,
    releases: usize,
}

fn observe_edges(actions: Res<SemanticActions>, mut delivered: ResMut<DeliveredEdges>) {
    for edge in actions.as_slice() {
        match edge.phase {
            ActionPhase::Started => delivered.starts += 1,
            ActionPhase::Changed => {}
            ActionPhase::Released => delivered.releases += 1,
        }
    }
}

fn small_timeline() -> InputTimeline {
    InputTimeline::with_limits(InputLimits::new(64, 4, 12, 8).unwrap()).unwrap()
}

fn runtime_with_timeline(mut timeline: InputTimeline) -> RuntimeInstance {
    timeline
        .connect_endpoint(WINDOW, InputSeatId::PRIMARY)
        .unwrap();
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .bind(ActionBinding::mouse(ActionId::new("hold").unwrap(), BUTTON))
        .unwrap();
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut app = App::new();
    app.insert_resource(timeline).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.init_resource::<DeliveredEdges>().unwrap();
    app.add_systems(CoreStage::Update, observe_edges).unwrap();
    RuntimeInstance::start(app).unwrap()
}

fn connect_pointer(
    runtime: &mut RuntimeInstance,
    pointer: u32,
) -> Result<(), ButtonInputDriverError> {
    runtime
        .with_driver_scope(|scope| {
            connect_input_pointer(
                scope,
                WINDOW,
                InputSeatId::PRIMARY,
                InputPointerId::new(pointer),
            )
        })
        .unwrap()
        .output
}

fn mouse(
    runtime: &mut RuntimeInstance,
    pointer: u32,
    input: ButtonDriverInput<MouseButton>,
) -> Result<(), ButtonInputDriverError> {
    let outcome = runtime
        .with_driver_scope(|scope| {
            apply_mouse_driver_input_at(
                scope,
                WINDOW,
                InputSeatId::PRIMARY,
                InputPointerId::new(pointer),
                input,
            )
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.map(|_| ())
}

fn drive_and_assert_drained(runtime: &mut RuntimeInstance) {
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .transitions()
            .is_empty()
    );
}

#[test]
fn balanced_endpoint_churn_does_not_retain_empty_physical_rows() {
    let mut runtime = runtime_with_timeline(small_timeline());
    for raw in 10..4_110 {
        let window = InputWindowId::new(u64::from(raw));
        runtime
            .with_driver_scope(|scope| connect_input_endpoint(scope, window, InputSeatId::PRIMARY))
            .unwrap()
            .output
            .unwrap();
        connect_pointer(&mut runtime, raw).unwrap();
        for input in [
            ButtonDriverInput::Press(KeyCode::Space),
            ButtonDriverInput::Release(KeyCode::Space),
        ] {
            runtime
                .with_driver_scope(|scope| {
                    apply_keyboard_driver_input_at(scope, window, InputSeatId::PRIMARY, input)
                })
                .unwrap()
                .output
                .unwrap();
        }
        mouse(&mut runtime, raw, ButtonDriverInput::Press(BUTTON)).unwrap();
        mouse(&mut runtime, raw, ButtonDriverInput::Release(BUTTON)).unwrap();
        runtime
            .with_driver_scope(|scope| {
                retire_input_pointer(
                    scope,
                    WINDOW,
                    InputSeatId::PRIMARY,
                    InputPointerId::new(raw),
                )
            })
            .unwrap()
            .output
            .unwrap();
        runtime
            .with_driver_scope(|scope| {
                apply_window_input_driver_event(
                    scope,
                    window,
                    InputSeatId::PRIMARY,
                    InputWindowEvent::SurfaceRetired,
                )
            })
            .unwrap()
            .output
            .unwrap();
        drive_and_assert_drained(&mut runtime);
    }
    let timeline = runtime.world().resource::<InputTimeline>();
    let live = (
        timeline.topology.endpoints.iter().flatten().count(),
        timeline.topology.pointers.iter().flatten().count(),
        timeline.topology.members.iter().flatten().count(),
    );
    let storage = (
        timeline.topology.endpoints.len(),
        timeline.topology.pointers.len(),
        timeline.topology.members.len(),
    );
    let pressed = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(BUTTON);
    runtime.finish().unwrap();
    assert_eq!(
        live,
        (2, 1, 0),
        "only the intentionally connected default and test endpoint survive churn"
    );
    assert_eq!(
        storage,
        (4, 12, 8),
        "historical labels cannot grow allocated pools"
    );
    assert!(!pressed);
}

fn saturate(runtime: &mut RuntimeInstance) -> usize {
    let mut accepted = 0;
    for raw in 1..=9 {
        connect_pointer(runtime, raw).unwrap();
        if mouse(runtime, raw, ButtonDriverInput::Press(BUTTON)).is_err() {
            break;
        }
        accepted += 1;
        drive_and_assert_drained(runtime);
    }
    accepted
}

#[test]
fn held_endpoint_admission_is_bounded_even_after_each_event_log_is_drained() {
    let mut runtime = runtime_with_timeline(small_timeline());
    let accepted = saturate(&mut runtime);
    let live = runtime
        .world()
        .resource::<InputTimeline>()
        .topology
        .members
        .iter()
        .flatten()
        .count();
    let pressed = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(BUTTON);
    runtime.finish().unwrap();
    assert_eq!(accepted, 8);
    assert_eq!(live, 8);
    assert!(pressed, "refusing the next member preserves accepted holds");
}

#[test]
fn platform_focus_loss_finishes_all_accepted_holds_after_cross_frame_growth() {
    let mut runtime = runtime_with_timeline(small_timeline());
    let accepted = saturate(&mut runtime);
    let outcome = runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                WINDOW,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    drive_and_assert_drained(&mut runtime);
    let physical = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(BUTTON);
    let semantic = runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("hold").unwrap(),
        &ActionContext::gameplay(),
    );
    let delivered = runtime.world().resource::<DeliveredEdges>();
    let delivered = (delivered.starts, delivered.releases);
    runtime.finish().unwrap();
    assert_eq!(accepted, 8);
    assert!(
        outcome.output.is_ok(),
        "accepted members already reserve complete termination"
    );
    assert!(!physical && !semantic);
    assert_eq!(delivered, (1, 1));
}

#[test]
fn physical_loss_does_not_allocate_a_new_route_binding_generation() {
    let mut timeline = small_timeline();
    timeline.next_binding_generation = u64::MAX;
    let mut runtime = runtime_with_timeline(timeline);
    connect_pointer(&mut runtime, 1).unwrap();
    mouse(&mut runtime, 1, ButtonDriverInput::Press(BUTTON)).unwrap();
    drive_and_assert_drained(&mut runtime);
    let outcome = runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                WINDOW,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .unwrap();
    drive_and_assert_drained(&mut runtime);
    let physical = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(BUTTON);
    let semantic = runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("hold").unwrap(),
        &ActionContext::gameplay(),
    );
    runtime.finish().unwrap();
    assert!(
        !physical && !semantic,
        "revocation requires no new identity"
    );
    assert!(outcome.output.is_ok());
}

fn drain_unmapped_timeline(timeline: &mut InputTimeline) {
    timeline.finish_projection();
    timeline.publish_observations(&mut InputFrameObservations::default());
    timeline.finalize_ui_routing();
    timeline.stage_actions(&ActionStaging::default());
    timeline.take_deferred_action_inputs();
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
    assert_eq!(timeline.retained_len(), 0);
}

fn route(window: u64) -> InputPresentedRoute {
    InputPresentedRoute::new(
        InputWindowId::new(window),
        InputSurfaceId::new(window),
        InputSeatId::PRIMARY,
        InputPresentationEpoch::new(1).unwrap(),
        InputSlotGeneration::new(1).unwrap(),
        InputRouteGeneration::new(1).unwrap(),
        InputGeometryGeneration::new(1).unwrap(),
    )
}

#[test]
fn geometry_loss_revokes_only_its_route_when_another_prepared_route_pins_identity() {
    let mut timeline = InputTimeline::default();
    for window in [2, 3] {
        timeline
            .connect_endpoint(InputWindowId::new(window), InputSeatId::PRIMARY)
            .unwrap();
    }
    timeline
        .connect_pointer(
            InputWindowId::new(2),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
        )
        .unwrap();
    timeline.publish_presented_route(route(2)).unwrap();
    timeline
        .admit_pointer(
            InputWindowId::new(2),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    drain_unmapped_timeline(&mut timeline);
    let prepared = prepare_presented_route(&mut timeline, route(3)).unwrap();
    timeline.next_binding_generation = u64::MAX;
    timeline
        .admit_window_event(
            InputWindowId::new(2),
            InputSeatId::PRIMARY,
            InputWindowEvent::Resized {
                logical_size: Vec2::new(100.0, 100.0),
            },
        )
        .unwrap();
    timeline
        .admit_pointer(
            InputWindowId::new(2),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(3.0, 4.0)),
        )
        .unwrap();
    assert!(!timeline.entries().last().unwrap().route().is_ui_routable());
    assert!(
        matches!(
            commit_presented_route(&mut timeline, prepared),
            InputRouteCommit::Published(_)
        ),
        "another endpoint's prepared publication remains valid"
    );
}

#[test]
fn actual_mixed_focus_close_and_retirement_have_eight_distinct_terminal_records() {
    let mut timeline = InputTimeline::default();
    timeline
        .connect_endpoint(WINDOW, InputSeatId::PRIMARY)
        .unwrap();
    timeline
        .admit_window_event(
            WINDOW,
            InputSeatId::PRIMARY,
            InputWindowEvent::Focused(true),
        )
        .unwrap();
    timeline
        .admit_keyboard(
            WINDOW,
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Space),
        )
        .unwrap();
    for raw in 1..=2 {
        let pointer = InputPointerId::new(raw);
        timeline
            .connect_pointer(WINDOW, InputSeatId::PRIMARY, pointer)
            .unwrap();
        timeline
            .admit_mouse_button(
                WINDOW,
                InputSeatId::PRIMARY,
                pointer,
                ButtonDriverInput::Press(BUTTON),
            )
            .unwrap();
        timeline
            .admit_pointer(
                WINDOW,
                InputSeatId::PRIMARY,
                pointer,
                PointerDriverInput::Moved(Vec2::new(raw as f32, 1.0)),
            )
            .unwrap();
    }
    drain_unmapped_timeline(&mut timeline);
    let focus = timeline
        .admit_window_event(
            WINDOW,
            InputSeatId::PRIMARY,
            InputWindowEvent::Focused(false),
        )
        .unwrap();
    let close = timeline
        .admit_window_event(
            WINDOW,
            InputSeatId::PRIMARY,
            InputWindowEvent::CloseRequested,
        )
        .unwrap();
    let retire = timeline
        .admit_window_event(
            WINDOW,
            InputSeatId::PRIMARY,
            InputWindowEvent::SurfaceRetired,
        )
        .unwrap();
    assert_eq!(
        (focus.admitted(), close.admitted(), retire.admitted()),
        (6, 1, 1)
    );
    let entries = timeline.entries().collect::<Vec<_>>();
    assert_eq!(entries.len(), 8);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| matches!(entry.kind(), InputEventKind::PointerLeft))
            .count(),
        2
    );
    assert!(
        entries
            .windows(2)
            .all(|pair| pair[0].sequence() < pair[1].sequence())
    );
    assert!(matches!(
        entries[5].kind(),
        InputEventKind::FocusChanged { focused: false }
    ));
    assert!(matches!(entries[6].kind(), InputEventKind::CloseRequested));
    assert!(matches!(entries[7].kind(), InputEventKind::SurfaceRetired));
}
