use std::{error::Error, sync::Arc};

use nara_core::Vec2;
use nara_ecs::World;
use nara_input::{
    __InputDriverEvent, ActionPhase, ButtonDriverInput, InputPointerId, InputSeatId, InputTimeline,
    InputWindowEvent, InputWindowId, KeyCode, MouseButton, PointerDriverInput,
};
use nara_ui::{
    advanced::{
        UiAccessibilityInput, UiActivationSource, UiFrame, UiKey, UiPresentationReceipt,
        UiRouteRejection, UiSeatId, UiSurfaceId, latest_ui_frame, ui_semantic_snapshot,
    },
    prelude::{UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal, UiSemanticRole},
};

use super::{fixture::GameplayProbe, runtime::LabRuntime};

const PRIMARY_WINDOW: InputWindowId = InputWindowId::new(11);
const SECONDARY_WINDOW: InputWindowId = InputWindowId::new(12);

pub(super) fn prove_pointer_capture_and_surface_isolation(
    app: &mut LabRuntime,
    primary_receipt: UiPresentationReceipt,
    secondary_receipt: UiPresentationReceipt,
    primary_key: &UiKey,
    paused_cursor: &UiActionCursor,
) -> Result<(), Box<dyn Error>> {
    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(30.0, 30.0))?;
    admit_primary_button(
        app,
        PRIMARY_WINDOW,
        ButtonDriverInput::Press(MouseButton::Left),
    )?;
    app.tick()?;

    let primary_semantics = ui_semantic_snapshot(app.world(), primary_receipt, UiSeatId::PRIMARY)?;
    let primary = primary_semantics
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("primary semantic node should exist");
    assert_eq!(primary.role(), UiSemanticRole::Button);
    assert_eq!(primary.name(), "Primary action");
    assert!(primary.state().enabled());
    assert!(primary.parent().is_some());
    assert!(primary.rect().size().width() > 0.0);
    assert!(primary.state().focused());
    assert!(primary.state().pressed());
    assert_eq!(primary.actions(), [UiActionId::new(super::PRIMARY_ACTION)?]);

    let secondary_semantics =
        ui_semantic_snapshot(app.world(), secondary_receipt, UiSeatId::PRIMARY)?;
    let secondary = secondary_semantics
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("the same stable key should exist on the second surface");
    assert!(!secondary.state().pressed());

    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(300.0, 170.0))?;
    admit_primary_button(
        app,
        PRIMARY_WINDOW,
        ButtonDriverInput::Release(MouseButton::Left),
    )?;
    app.tick()?;
    let released = ui_semantic_snapshot(app.world(), primary_receipt, UiSeatId::PRIMARY)?;
    let primary = released
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("captured target should remain in the semantic tree");
    assert!(primary.state().focused());
    assert!(!primary.state().pressed());
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(paused_cursor)?
            .is_empty()
    );
    Ok(())
}

pub(super) fn prove_pointer_activation(
    app: &mut LabRuntime,
    action: &UiActionId,
    fast_cursor: &Arc<UiActionCursor>,
    paused_cursor: &UiActionCursor,
) -> Result<(), Box<dyn Error>> {
    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(30.0, 30.0))?;
    admit_primary_button(
        app,
        PRIMARY_WINDOW,
        ButtonDriverInput::Press(MouseButton::Left),
    )?;
    admit_primary_button(
        app,
        PRIMARY_WINDOW,
        ButtonDriverInput::Release(MouseButton::Left),
    )?;
    app.tick()?;
    assert_latest_action(
        app.world(),
        fast_cursor,
        action,
        UiActivationSource::Pointer,
    )?;
    assert_eq!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(paused_cursor)?
            .len(),
        1
    );
    acknowledge_latest(app, fast_cursor)?;
    assert_eq!(app.world().resource::<UiActionJournal>().retained_len(), 1);
    Ok(())
}

pub(super) fn prove_non_modal_pass_through(app: &mut LabRuntime) -> Result<(), Box<dyn Error>> {
    let rejection_count = app
        .world()
        .resource::<nara_ui::UiRouteRejectionLog>()
        .as_slice()
        .len();
    admit_keyboard(app, ButtonDriverInput::Press(KeyCode::Escape))?;
    app.tick()?;
    assert_eq!(
        app.world().resource::<GameplayProbe>().phases(),
        [ActionPhase::Started]
    );
    assert_eq!(
        app.world()
            .resource::<nara_ui::UiRouteRejectionLog>()
            .as_slice()
            .len(),
        rejection_count
    );

    admit_keyboard(app, ButtonDriverInput::Release(KeyCode::Escape))?;
    app.tick()?;
    assert_eq!(
        app.world().resource::<GameplayProbe>().phases(),
        [ActionPhase::Released]
    );
    Ok(())
}

pub(super) fn prove_modal_rejection(app: &mut LabRuntime) -> Result<(), Box<dyn Error>> {
    admit_keyboard(app, ButtonDriverInput::Press(KeyCode::Escape))?;
    app.tick()?;
    assert!(app.world().resource::<GameplayProbe>().phases().is_empty());
    assert!(matches!(
        latest_rejection(app),
        Some(UiRouteRejection::UnhandledInputBlocked)
    ));
    Ok(())
}

pub(super) fn prove_disabled_rejection(
    app: &mut LabRuntime,
    receipt: UiPresentationReceipt,
    primary_key: &UiKey,
) -> Result<(), Box<dyn Error>> {
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)?;
    let primary = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("the disabled primary action should retain semantic identity");
    assert!(!primary.state().enabled());
    assert!(!primary.state().focusable());
    assert!(primary.state().focused());

    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(30.0, 30.0))?;
    app.tick()?;
    assert!(matches!(
        latest_rejection(app),
        Some(UiRouteRejection::NonActivation(
            nara_ui::advanced::UiNonActivation::TargetDisabled { .. }
        ))
    ));
    let reconciled = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)?;
    let primary = reconciled
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("disabled reconciliation should preserve the primary semantic identity");
    assert!(!primary.state().enabled());
    assert!(primary.state().focused());
    Ok(())
}

pub(super) fn prove_reenabled_focus(
    app: &LabRuntime,
    receipt: UiPresentationReceipt,
    primary_key: &UiKey,
) -> Result<(), Box<dyn Error>> {
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)?;
    let primary = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("the re-enabled primary action should preserve semantic identity");
    assert!(primary.state().enabled());
    assert!(primary.state().focusable());
    assert!(primary.state().focused());
    Ok(())
}

pub(super) fn activate_accessibility(
    app: &mut LabRuntime,
    receipt: UiPresentationReceipt,
    target: &UiKey,
    action: &UiActionId,
) -> Result<(), Box<dyn Error>> {
    app.activate_accessibility(UiAccessibilityInput::semantic_action(
        receipt,
        UiSeatId::PRIMARY,
        target.clone(),
        action.clone(),
    ))
}

pub(super) fn invalidate_geometry(app: &mut LabRuntime) -> Result<(), Box<dyn Error>> {
    admit_input(
        app,
        __InputDriverEvent::Window {
            window: PRIMARY_WINDOW,
            seat: InputSeatId::PRIMARY,
            event: InputWindowEvent::Resized {
                logical_size: Vec2::new(640.0, 360.0),
            },
        },
    )?;
    app.tick()?;
    Ok(())
}

pub(super) fn queue_secondary_pointer(app: &mut LabRuntime) -> Result<(), Box<dyn Error>> {
    admit_pointer(app, SECONDARY_WINDOW, Vec2::new(30.0, 30.0))
}

pub(super) fn latest_rejection(app: &LabRuntime) -> Option<&UiRouteRejection> {
    app.world()
        .resource::<nara_ui::UiRouteRejectionLog>()
        .as_slice()
        .first()
        .map(|record| record.rejection())
}

fn admit_keyboard(
    app: &mut LabRuntime,
    input: ButtonDriverInput<KeyCode>,
) -> Result<(), Box<dyn Error>> {
    admit_input(
        app,
        __InputDriverEvent::Keyboard {
            window: PRIMARY_WINDOW,
            seat: InputSeatId::PRIMARY,
            input,
        },
    )
}

fn admit_pointer(
    app: &mut LabRuntime,
    window: InputWindowId,
    position: Vec2,
) -> Result<(), Box<dyn Error>> {
    admit_input(
        app,
        __InputDriverEvent::Pointer {
            window,
            seat: InputSeatId::PRIMARY,
            pointer: InputPointerId::PRIMARY,
            input: PointerDriverInput::Moved(position),
        },
    )
}

fn admit_primary_button(
    app: &mut LabRuntime,
    window: InputWindowId,
    input: ButtonDriverInput<MouseButton>,
) -> Result<(), Box<dyn Error>> {
    admit_input(
        app,
        __InputDriverEvent::Mouse {
            window,
            seat: InputSeatId::PRIMARY,
            pointer: InputPointerId::PRIMARY,
            input,
        },
    )
}

fn admit_input(app: &mut LabRuntime, input: __InputDriverEvent) -> Result<(), Box<dyn Error>> {
    let output = app
        .runtime_mut()
        .with_driver_scope(|scope| scope.__apply_port::<InputTimeline>(input))?
        .output??;
    assert_eq!(output.admission.admitted(), 1);
    Ok(())
}

pub(super) fn register_consumers(
    world: &mut World,
) -> Result<(Arc<UiActionCursor>, Arc<UiActionCursor>), Box<dyn Error>> {
    let mut journal = world.resource_mut::<UiActionJournal>();
    let fast = journal.register_consumer(UiActionConsumerId::new("interaction-lab.fast")?)?;
    let paused = journal.register_consumer(UiActionConsumerId::new("interaction-lab.paused")?)?;
    Ok((Arc::new(fast), Arc::new(paused)))
}

pub(super) fn acknowledge_latest(
    app: &mut LabRuntime,
    cursor: &Arc<UiActionCursor>,
) -> Result<(), Box<dyn Error>> {
    app.acknowledge_latest(cursor)
}

pub(super) fn assert_latest_action(
    world: &World,
    cursor: &UiActionCursor,
    action: &UiActionId,
    source: UiActivationSource,
) -> Result<(), Box<dyn Error>> {
    let event = world
        .resource::<UiActionJournal>()
        .pending(cursor)?
        .last()
        .expect("the expected semantic action should be retained");
    assert_eq!(event.action(), action);
    assert_eq!(event.source(), source);
    Ok(())
}

pub(super) fn semantic_key(
    app: &LabRuntime,
    receipt: UiPresentationReceipt,
    name: &str,
) -> Result<UiKey, Box<dyn Error>> {
    Ok(
        ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY)?
            .nodes()
            .iter()
            .find(|node| node.name() == name)
            .unwrap_or_else(|| panic!("missing semantic node {name}"))
            .key()
            .clone(),
    )
}

pub(super) fn assert_structural_order(frame: &UiFrame) {
    let primary = frame
        .semantics()
        .iter()
        .find(|node| node.name() == "Primary action")
        .expect("primary semantic node should compile")
        .key();
    let secondary = frame
        .semantics()
        .iter()
        .find(|node| node.name() == "Secondary action")
        .expect("secondary semantic node should compile")
        .key();
    let primary_index = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(primary))
        .expect("primary layout should compile")
        .structural_index();
    let secondary_index = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(secondary))
        .expect("secondary layout should compile")
        .structural_index();
    assert!(primary_index < secondary_index);
    assert!(
        frame
            .semantics()
            .iter()
            .any(|node| node.name() == "Scene-authored status")
    );
}

pub(super) fn required_frame(
    app: &LabRuntime,
    surface: UiSurfaceId,
) -> Result<Arc<UiFrame>, Box<dyn Error>> {
    latest_ui_frame(app.world(), surface)?.ok_or_else(|| {
        let diagnostics = app
            .world()
            .resource::<nara_ui::UiAdapterDiagnostics>()
            .as_slice();
        let frames = nara_ui::advanced::latest_ui_frames(app.world()).map(|frames| {
            frames
                .iter()
                .map(|frame| frame.environment().surface_id().get())
                .collect::<Vec<_>>()
        });
        format!(
            "surface {} did not compile a frame: diagnostics={diagnostics:?}, frames={frames:?}",
            surface.get()
        )
        .into()
    })
}
