use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::materialize_startup_scene,
    app::{
        PluginDefinition, RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
        RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
    },
    gameplay::GameplayCommandSubmission,
    input::{ButtonDriverInput, KeyCode, MouseButton},
    prelude::World,
    project_host::ProjectContentLoader,
    ui::{
        UiActionId, UiRouteRejection, UiRouteRejectionLog, UiSemanticRole, UiSurfaceId,
        advanced::{
            UiKey, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, active_ui_surface_slots,
            update_ui_surface_slot,
        },
        latest_ui_frame, ui_semantic_snapshot,
    },
};
use nara_reference_game::{
    REFERENCE_UI_SURFACE_ID, WaveOutcome, WaveRetryPhase, WaveRetryRejection, WaveRetryStatus,
    WaveRunGeneration, WaveSnapshot, bundled_wave_commands,
};
use nara_ui_kernel::UiPaintPrimitive;
use nara_ui_render::{UiPacketPrimitive, UiPacketRect};

use super::project_content_fixture::headless_wave_candidate_plan_and_root_with;

#[path = "ui_product_fixture/interaction.rs"]
mod interaction;
#[path = "ui_product_fixture/presenter.rs"]
mod presenter;

use interaction::{
    activate_component_accessibility, activate_gamepad, clear_focus, drive_fixed,
    drive_to_terminal, focus_retry, inject_keyboard, inject_pointer_click, mouse, pointer_left,
    pointer_move, resize, retry_platform_center,
};
use presenter::{
    ComponentPresentationState, component_presenter_plugin,
    retry_replacement_rejection_fixture_plugin,
};

pub const REFERENCE_RETRY_ACTION: &str = "reference-game.retry";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPhysicalTarget {
    pub width: u32,
    pub height: u32,
    pub render_scale: f32,
    pub input_scale_factor: f32,
}

pub const UI_PHYSICAL_TARGETS: [UiPhysicalTarget; 3] = [
    UiPhysicalTarget {
        width: 1_280,
        height: 720,
        render_scale: 4.0 / 3.0,
        input_scale_factor: 1.0,
    },
    UiPhysicalTarget {
        width: 1_920,
        height: 1_080,
        render_scale: 2.0,
        input_scale_factor: 1.0,
    },
    UiPhysicalTarget {
        width: 2_560,
        height: 1_440,
        render_scale: 8.0 / 3.0,
        input_scale_factor: 1.0,
    },
];

pub const CANONICAL_UI_TARGET: UiPhysicalTarget = UI_PHYSICAL_TARGETS[1];

pub const FRACTIONAL_INPUT_TARGET: UiPhysicalTarget = UiPhysicalTarget {
    width: 1_920,
    height: 1_080,
    render_scale: 2.0,
    input_scale_factor: 1.25,
};

pub const LETTERBOX_RESIZE_TARGET: UiPhysicalTarget = UiPhysicalTarget {
    width: 1_024,
    height: 768,
    render_scale: 1_024.0 / 960.0,
    input_scale_factor: 1.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductUiState {
    Running,
    Completed,
    Defeated,
    RetryPending,
    RetryRejected,
    RetryApplied,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCapture {
    pub key: UiKey,
    pub parent: Option<UiKey>,
    pub role: UiSemanticRole,
    pub name: String,
    pub value: Option<String>,
    pub actions: Vec<UiActionId>,
    pub enabled: bool,
    pub inert: bool,
    pub focusable: bool,
    pub focused: bool,
    pub pressed: bool,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogicalLayoutCapture {
    pub key: UiKey,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyStateCapture {
    pub key: UiKey,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutCapture {
    pub allocated_width: f32,
    pub allocated_height: f32,
    pub layout_width: f32,
    pub layout_height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProductUiCapture {
    pub state: ProductUiState,
    pub runtime_generation: u64,
    pub frame_revision: u64,
    pub source_revision: u64,
    pub route_revision: u64,
    pub presentation_revision: u64,
    pub presentation_epoch: u64,
    pub run_generation: u64,
    pub render_scale: f32,
    pub input_scale_factor: f32,
    pub logical_layouts: Vec<LogicalLayoutCapture>,
    pub text_layouts: Vec<TextLayoutCapture>,
    pub semantic_facts: Vec<SemanticCapture>,
    pub paint_keys: Vec<UiKey>,
    pub hit_facts: Vec<KeyStateCapture>,
    pub focus_facts: Vec<KeyStateCapture>,
    pub retry_region_primitives: Vec<UiPacketPrimitive>,
    pub packet_primitives: usize,
    pub glyph_pages: usize,
    pub has_nonblank_glyphs: bool,
    pub packet_bounds: Option<UiPacketRect>,
}

#[derive(Debug, PartialEq)]
pub struct SuccessfulProductTrace {
    pub running: ProductUiCapture,
    pub completed: ProductUiCapture,
    pub pending: ProductUiCapture,
    pub applied: ProductUiCapture,
}

#[derive(Debug, PartialEq)]
pub struct ProductStateCaptureSet {
    pub successful: SuccessfulProductTrace,
    pub defeated: ProductUiCapture,
    pub rejected_pending: ProductUiCapture,
    pub rejected: ProductUiCapture,
}

impl ProductUiCapture {
    pub fn structural_keys(&self) -> impl Iterator<Item = &UiKey> {
        self.logical_layouts.iter().map(|layout| &layout.key)
    }

    pub fn retry(&self) -> Option<&SemanticCapture> {
        self.semantic_facts.iter().find(|fact| {
            fact.actions
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingRetryReactivation {
    pub pending: WaveRetryStatus,
    pub after_reactivation: WaveRetryStatus,
    pub applied: WaveRetryStatus,
    pub run_generation: u64,
}

#[derive(Debug, PartialEq)]
pub struct LetterboxResizeOutcome {
    pub capture: ProductUiCapture,
    pub retry_status: WaveRetryStatus,
    pub black_bar_input_blocked: bool,
    pub content_origin: (f32, f32),
    pub content_size: (f32, f32),
    pub content_scale_factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryActivation {
    Pointer,
    Keyboard,
    Gamepad,
    ComponentAccessibility,
    ConcurrentKeyboardAndPointer,
}

#[derive(Debug, PartialEq)]
pub struct RetryInteractionCaptures {
    pub default: ProductUiCapture,
    pub hovered: ProductUiCapture,
    pub pressed: ProductUiCapture,
    pub focused: ProductUiCapture,
}

pub fn capture_product_states(target: UiPhysicalTarget) -> ProductStateCaptureSet {
    let mut successful = product_runtime(
        target,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_fixed(&mut successful);
    assert_eq!(
        successful.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Running
    );
    let running = capture(&successful, target);
    drive_to_terminal(&mut successful);
    assert_eq!(
        successful.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Completed
    );
    let completed = capture(&successful, target);
    activate_retry_to_pending(&mut successful);
    assert_eq!(
        successful.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    let pending = capture(&successful, target);
    drive_fixed(&mut successful);
    assert_eq!(
        successful.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Applied
    );
    let applied = capture(&successful, target);
    stop_runtime(successful);

    let defeated_command =
        nara_reference_game::movement_command(1, 1, nara_reference_game::MovementDirection::Right)
            .expect("the product defeat command should be valid");
    let mut defeated =
        product_runtime(target, component_presenter_plugin(), vec![defeated_command]);
    drive_to_terminal(&mut defeated);
    assert_eq!(
        defeated.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Defeated
    );
    let defeated_capture = capture(&defeated, target);
    stop_runtime(defeated);

    let mut rejected = product_runtime(
        target,
        retry_replacement_rejection_fixture_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut rejected);
    activate_retry_to_pending(&mut rejected);
    assert_eq!(
        rejected.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    let rejected_pending = capture(&rejected, target);
    drive_fixed(&mut rejected);
    let retry = *rejected.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Idle);
    assert_eq!(
        retry.last_rejection(),
        Some(WaveRetryRejection::ReplacementRejected)
    );
    let rejected_capture = capture(&rejected, target);
    stop_runtime(rejected);

    ProductStateCaptureSet {
        successful: SuccessfulProductTrace {
            running,
            completed,
            pending,
            applied,
        },
        defeated: defeated_capture,
        rejected_pending,
        rejected: rejected_capture,
    }
}

pub fn capture_readability_case(target: UiPhysicalTarget) -> ProductUiCapture {
    let mut runtime = product_runtime(
        target,
        retry_replacement_rejection_fixture_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);
    activate_retry_to_pending(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    drive_fixed(&mut runtime);
    assert_eq!(
        runtime
            .world()
            .resource::<WaveRetryStatus>()
            .last_rejection(),
        Some(WaveRetryRejection::ReplacementRejected)
    );
    let capture = capture(&runtime, target);
    stop_runtime(runtime);
    capture
}

pub fn exercise_retry_activation(activation: RetryActivation) -> WaveRetryStatus {
    exercise_retry_activation_at(CANONICAL_UI_TARGET, activation)
}

fn activate_retry_to_pending(runtime: &mut RuntimeInstance) {
    focus_retry(runtime);
    inject_keyboard(runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the product Retry activation frame should drive");
    drive_fixed(runtime);
}

pub fn exercise_fractional_pointer_retry_activation() -> WaveRetryStatus {
    exercise_retry_activation_at(FRACTIONAL_INPUT_TARGET, RetryActivation::Pointer)
}

fn exercise_retry_activation_at(
    target: UiPhysicalTarget,
    activation: RetryActivation,
) -> WaveRetryStatus {
    let mut runtime = product_runtime(
        target,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);

    if !matches!(activation, RetryActivation::Pointer) {
        focus_retry(&mut runtime);
    }
    match activation {
        RetryActivation::Pointer => inject_pointer_click(&mut runtime),
        RetryActivation::Keyboard => inject_keyboard(&mut runtime, KeyCode::Enter),
        RetryActivation::Gamepad => activate_gamepad(&mut runtime),
        RetryActivation::ComponentAccessibility => activate_component_accessibility(&mut runtime),
        RetryActivation::ConcurrentKeyboardAndPointer => {
            inject_keyboard(&mut runtime, KeyCode::Enter);
            inject_pointer_click(&mut runtime);
        }
    }
    runtime
        .drive(Duration::ZERO)
        .expect("the product activation frame should drive");
    drive_fixed(&mut runtime);
    let status = *runtime.world().resource::<WaveRetryStatus>();
    stop_runtime(runtime);
    status
}

pub fn exercise_pending_retry_reactivation() -> PendingRetryReactivation {
    let mut runtime = product_runtime(
        CANONICAL_UI_TARGET,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);
    focus_retry(&mut runtime);
    inject_keyboard(&mut runtime, KeyCode::Enter);
    runtime
        .drive(Duration::ZERO)
        .expect("the initial Retry activation frame should drive");
    drive_fixed(&mut runtime);
    let pending = *runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(pending.phase(), WaveRetryPhase::Pending);

    inject_pointer_click(&mut runtime);
    runtime
        .drive(Duration::ZERO)
        .expect("the disabled Retry reactivation frame should drive");
    let after_reactivation = *runtime.world().resource::<WaveRetryStatus>();

    drive_fixed(&mut runtime);
    let applied = *runtime.world().resource::<WaveRetryStatus>();
    let run_generation = runtime.world().resource::<WaveRunGeneration>().get();
    stop_runtime(runtime);
    PendingRetryReactivation {
        pending,
        after_reactivation,
        applied,
        run_generation,
    }
}

pub fn exercise_paused_retry_activation() -> (WaveRetryStatus, WaveRetryStatus) {
    let mut runtime = product_runtime(
        CANONICAL_UI_TARGET,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);
    focus_retry(&mut runtime);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);

    inject_keyboard(&mut runtime, KeyCode::Enter);
    runtime.drive(Duration::ZERO).unwrap();
    let while_paused = *runtime.world().resource::<WaveRetryStatus>();

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Running);
    drive_fixed(&mut runtime);
    let after_resume = *runtime.world().resource::<WaveRetryStatus>();
    stop_runtime(runtime);
    (while_paused, after_resume)
}

pub fn exercise_resize_cancels_retry_capture() -> LetterboxResizeOutcome {
    let mut runtime = product_runtime(
        CANONICAL_UI_TARGET,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);
    let point = retry_platform_center(&runtime);
    pointer_move(&mut runtime, point);
    runtime.drive(Duration::ZERO).unwrap();
    mouse(&mut runtime, ButtonDriverInput::Press(MouseButton::Left));
    runtime.drive(Duration::ZERO).unwrap();
    resize(
        &mut runtime,
        LETTERBOX_RESIZE_TARGET.width,
        LETTERBOX_RESIZE_TARGET.height,
    );
    drive_until_surface_target(&mut runtime, LETTERBOX_RESIZE_TARGET);
    mouse(&mut runtime, ButtonDriverInput::Release(MouseButton::Left));
    runtime.drive(Duration::ZERO).unwrap();
    let retry_status = *runtime.world().resource::<WaveRetryStatus>();
    let capture = capture(&runtime, LETTERBOX_RESIZE_TARGET);
    let transform = active_ui_surface_slots(runtime.world())
        .expect("the component presenter should retain the reference surface slot")
        .into_iter()
        .find(|slot| slot.surface_id() == reference_surface_id())
        .expect("the reference UI surface slot should remain installed")
        .transform();
    let content_origin = (
        transform.physical_content_origin().x(),
        transform.physical_content_origin().y(),
    );
    let content_size = (
        transform.physical_content_size().width(),
        transform.physical_content_size().height(),
    );

    let previous_rejection_count = runtime
        .world()
        .resource::<UiRouteRejectionLog>()
        .as_slice()
        .len();
    pointer_move(
        &mut runtime,
        nara::prelude::Vec2::new(LETTERBOX_RESIZE_TARGET.width as f32 * 0.5, 48.0),
    );
    mouse(&mut runtime, ButtonDriverInput::Press(MouseButton::Left));
    mouse(&mut runtime, ButtonDriverInput::Release(MouseButton::Left));
    runtime.drive(Duration::ZERO).unwrap();
    let rejection_log = runtime.world().resource::<UiRouteRejectionLog>();
    let new_rejection_count = rejection_log
        .as_slice()
        .len()
        .saturating_sub(previous_rejection_count);
    // The bounded rejection log is newest-first, so this prefix contains only the black-bar input.
    let black_bar_input_blocked = rejection_log.as_slice()[..new_rejection_count]
        .iter()
        .any(|record| matches!(record.rejection(), UiRouteRejection::UnhandledInputBlocked));
    assert_eq!(
        runtime.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Idle
    );
    stop_runtime(runtime);
    LetterboxResizeOutcome {
        capture,
        retry_status,
        black_bar_input_blocked,
        content_origin,
        content_size,
        content_scale_factor: transform.content_scale_factor(),
    }
}

fn drive_until_surface_target(runtime: &mut RuntimeInstance, target: UiPhysicalTarget) {
    for _ in 0..8 {
        runtime.drive(Duration::ZERO).unwrap();
        let Some(slot) = active_ui_surface_slots(runtime.world())
            .ok()
            .and_then(|slots| {
                slots
                    .into_iter()
                    .find(|slot| slot.surface_id() == reference_surface_id())
            })
        else {
            continue;
        };
        let Some(frame) = latest_ui_frame(runtime.world(), reference_surface_id())
            .ok()
            .flatten()
        else {
            continue;
        };
        let Some((_, packet)) = runtime
            .world()
            .resource::<ComponentPresentationState>()
            .latest()
        else {
            continue;
        };
        let packet_frame = packet.frame();
        let transform = slot.transform();
        if (transform.physical_surface_size().width() - target.width as f32).abs() < 1.0e-6
            && (transform.physical_surface_size().height() - target.height as f32).abs() < 1.0e-6
            && (frame.environment().scale_factor() - target.render_scale).abs() < 1.0e-6
            && Arc::ptr_eq(&packet_frame, &frame)
        {
            return;
        }
    }
    panic!("the component fixture did not publish the resized target within eight frames");
}

pub fn capture_retry_interaction_variants() -> RetryInteractionCaptures {
    let target = CANONICAL_UI_TARGET;
    let mut runtime = product_runtime(
        target,
        component_presenter_plugin(),
        bundled_wave_commands(),
    );
    drive_to_terminal(&mut runtime);
    clear_focus(&mut runtime);
    let default = capture(&runtime, target);

    let point = retry_platform_center(&runtime);
    pointer_move(&mut runtime, point);
    runtime.drive(Duration::ZERO).unwrap();
    let hovered = capture(&runtime, target);
    mouse(&mut runtime, ButtonDriverInput::Press(MouseButton::Left));
    runtime.drive(Duration::ZERO).unwrap();
    let pressed = capture(&runtime, target);
    pointer_left(&mut runtime);
    runtime.drive(Duration::ZERO).unwrap();
    focus_retry(&mut runtime);
    let focused = capture(&runtime, target);
    stop_runtime(runtime);
    RetryInteractionCaptures {
        default,
        hovered,
        pressed,
        focused,
    }
}

fn product_runtime(
    target: UiPhysicalTarget,
    plugin: PluginDefinition,
    initial_commands: Vec<GameplayCommandSubmission>,
) -> RuntimeInstance {
    let (project, plan, root) = headless_wave_candidate_plan_and_root_with(plugin);
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&project, &plan).unwrap();
    let source = snapshot.startup_scene_source();
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let surface = active_ui_surface_slots(world)
                    .expect("the product UI adapter should be installed")
                    .into_iter()
                    .find(|slot| slot.surface_id() == reference_surface_id())
                    .expect("the reference UI surface should be installed");
                let authored =
                    UiSize::new(960.0, 540.0).expect("the reference UI authored extent is valid");
                let physical = UiSize::new(target.width as f32, target.height as f32)
                    .expect("the product UI physical target is valid");
                let transform = UiSurfaceContentTransform::new(
                    authored,
                    physical,
                    UiSafeAreaInsets::default(),
                    target.input_scale_factor,
                )
                .expect("the product UI target should produce one exact content transform");
                assert_eq!(transform.content_scale_factor(), target.render_scale);
                update_ui_surface_slot(world, surface.surface_id(), surface.endpoint(), transform)
                    .expect("the Host should update the reference UI mapping");
                let report = materialize_startup_scene(world, source)
                    .expect("the retained product scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
                let mut queue = world.resource_mut::<nara::gameplay::GameplayCommandQueue>();
                for command in initial_commands {
                    queue.submit(command).unwrap();
                }
            });
        })
        .unwrap();
    candidate.complete_startup().unwrap().promote()
}

fn stop_runtime(mut runtime: RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        runtime.drive(Duration::ZERO).unwrap();
        match runtime.state() {
            RuntimeState::Stopped => return,
            RuntimeState::CloseIncomplete => {
                panic!("component test runtime close became incomplete")
            }
            _ if Instant::now() < deadline => std::thread::yield_now(),
            state => panic!("component test runtime remained in {state:?} past its deadline"),
        }
    }
}

fn derive_product_ui_state(world: &World) -> ProductUiState {
    let retry = *world.resource::<WaveRetryStatus>();
    match retry.phase() {
        WaveRetryPhase::Pending => ProductUiState::RetryPending,
        WaveRetryPhase::Applied => ProductUiState::RetryApplied,
        WaveRetryPhase::Idle => {
            if retry.last_rejection() == Some(WaveRetryRejection::ReplacementRejected) {
                ProductUiState::RetryRejected
            } else {
                match world.resource::<WaveSnapshot>().outcome {
                    WaveOutcome::Running => ProductUiState::Running,
                    WaveOutcome::Completed => ProductUiState::Completed,
                    WaveOutcome::Defeated => ProductUiState::Defeated,
                }
            }
        }
    }
}

fn capture(runtime: &RuntimeInstance, target: UiPhysicalTarget) -> ProductUiCapture {
    let world = runtime.world();
    let frame = latest_ui_frame(world, reference_surface_id())
        .expect("the product UI adapter should remain installed")
        .expect("the product state should compile one immutable frame");
    let presenter = world.resource::<ComponentPresentationState>();
    let (receipt, packet) = presenter
        .latest()
        .expect("the bounded presenter should retain the accepted product packet");
    let acceptance = presenter
        .latest_acceptance_identity()
        .expect("the bounded presenter should retain backend acceptance identity");
    let packet_frame = packet.frame();
    assert!(Arc::ptr_eq(&packet_frame, &frame));
    let identity = packet.identity();
    assert_eq!(acceptance.packet(), identity);
    assert_eq!(acceptance.target(), packet.target());
    assert_eq!(
        acceptance.runtime_generation().get(),
        runtime.generation().get()
    );
    assert!(acceptance.matches_receipt(receipt));
    assert_eq!(receipt.surface_id(), identity.surface_id());
    assert_eq!(receipt.frame_revision(), identity.frame_revision());
    assert_eq!(receipt.route_revision(), identity.route_revision());
    assert_eq!(
        receipt.presentation_revision(),
        identity.presentation_revision()
    );
    let semantics = ui_semantic_snapshot(world, receipt, nara::ui::UiSeatId::PRIMARY)
        .expect("the bounded presenter should expose exact frame semantics");

    let logical_layouts = frame
        .layouts()
        .iter()
        .filter_map(|layout| {
            let key = layout.key()?;
            let rect = layout.rect();
            Some(LogicalLayoutCapture {
                key: key.clone(),
                x: rect.origin().x(),
                y: rect.origin().y(),
                width: rect.size().width(),
                height: rect.size().height(),
                visible: layout.visible(),
            })
        })
        .collect::<Vec<_>>();
    let text_layouts = frame
        .paint()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Text { rect, layout, .. } => Some(TextLayoutCapture {
                allocated_width: rect.size().width(),
                allocated_height: rect.size().height(),
                layout_width: layout.width(),
                layout_height: layout.height(),
            }),
            UiPaintPrimitive::Rect { .. } => None,
        })
        .collect::<Vec<_>>();
    let semantic_facts = semantics
        .nodes()
        .iter()
        .map(|node| {
            let rect = node.rect();
            let state = node.state();
            SemanticCapture {
                key: node.key().clone(),
                parent: node.parent().cloned(),
                role: node.role(),
                name: node.name().to_owned(),
                value: node.value().map(str::to_owned),
                actions: node.actions().to_vec(),
                enabled: state.enabled(),
                inert: state.inert(),
                focusable: state.focusable(),
                focused: state.focused(),
                pressed: state.pressed(),
                selected: state.selected(),
                expanded: state.expanded(),
                x: rect.origin().x(),
                y: rect.origin().y(),
                width: rect.size().width(),
                height: rect.size().height(),
            }
        })
        .collect::<Vec<_>>();
    let slot = active_ui_surface_slots(world)
        .expect("the product UI adapter should remain installed")
        .into_iter()
        .find(|slot| slot.surface_id() == reference_surface_id())
        .expect("the reference UI surface slot should remain installed");
    let retry = semantic_facts.iter().find(|fact| {
        fact.actions
            .iter()
            .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
    });
    let retry_packet_rect = retry.and_then(|fact| {
        let minimum = slot
            .transform()
            .authored_to_physical(nara_ui_kernel::UiPoint::new(fact.x, fact.y))?;
        let maximum = slot
            .transform()
            .authored_to_physical(nara_ui_kernel::UiPoint::new(
                fact.x + fact.width,
                fact.y + fact.height,
            ))?;
        UiPacketRect::new(
            minimum.x().floor() as i32,
            minimum.y().floor() as i32,
            maximum.x().ceil() as i32,
            maximum.y().ceil() as i32,
        )
    });
    let retry_region_primitives = packet
        .primitives()
        .iter()
        .filter(|primitive| {
            retry_packet_rect.is_some_and(|rect| {
                let primitive = primitive.rect();
                let center_x = primitive.min_x() + (primitive.max_x() - primitive.min_x()) / 2;
                let center_y = primitive.min_y() + (primitive.max_y() - primitive.min_y()) / 2;
                center_x >= rect.min_x()
                    && center_x < rect.max_x()
                    && center_y >= rect.min_y()
                    && center_y < rect.max_y()
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let paint_keys = frame
        .paint()
        .iter()
        .filter_map(|primitive| primitive.key().cloned())
        .collect::<Vec<_>>();
    let hit_facts = frame
        .hit_regions()
        .iter()
        .map(|fact| KeyStateCapture {
            key: fact.key().clone(),
            enabled: fact.enabled(),
        })
        .collect::<Vec<_>>();
    let focus_facts = frame
        .focus_order()
        .iter()
        .map(|fact| KeyStateCapture {
            key: fact.key().clone(),
            enabled: fact.enabled(),
        })
        .collect::<Vec<_>>();
    let has_nonblank_glyphs = packet
        .glyph_pages()
        .iter()
        .any(|page| page.pixels().iter().any(|alpha| *alpha != 0));
    let packet_bounds = packet
        .primitives()
        .iter()
        .map(UiPacketPrimitive::rect)
        .reduce(|bounds, rect| {
            UiPacketRect::new(
                bounds.min_x().min(rect.min_x()),
                bounds.min_y().min(rect.min_y()),
                bounds.max_x().max(rect.max_x()),
                bounds.max_y().max(rect.max_y()),
            )
            .expect("the union of valid packet rectangles should remain valid")
        });
    let environment = frame.environment();
    assert_eq!(environment.logical_size().width(), 960.0);
    assert_eq!(environment.logical_size().height(), 540.0);
    assert!((environment.scale_factor() - target.render_scale).abs() < 1.0e-6);
    assert!((slot.transform().platform_scale_factor() - target.input_scale_factor).abs() < 1.0e-6);

    ProductUiCapture {
        state: derive_product_ui_state(world),
        runtime_generation: runtime.generation().get(),
        frame_revision: identity.frame_revision().get(),
        source_revision: identity.source_revision().get(),
        route_revision: identity.route_revision().get(),
        presentation_revision: identity.presentation_revision().get(),
        presentation_epoch: receipt.presentation_epoch(),
        run_generation: world.resource::<WaveRunGeneration>().get(),
        render_scale: environment.scale_factor(),
        input_scale_factor: slot.transform().platform_scale_factor(),
        logical_layouts,
        text_layouts,
        semantic_facts,
        paint_keys,
        hit_facts,
        focus_facts,
        retry_region_primitives,
        packet_primitives: packet.primitives().len(),
        glyph_pages: packet.glyph_pages().len(),
        has_nonblank_glyphs,
        packet_bounds,
    }
}

fn reference_surface_id() -> UiSurfaceId {
    UiSurfaceId::new(REFERENCE_UI_SURFACE_ID).expect("the reference UI surface ID is non-zero")
}
