use std::{error::Error, sync::Arc};

use nara_app::App;
use nara_core::Vec2;
use nara_ecs::World;
use nara_input::{
    ActionPhase, ButtonDriverInput, InputPointerId, InputSeatId, InputTimeline, InputWindowEvent,
    InputWindowId, KeyCode, MouseButton, PointerDriverInput,
};
use nara_ui::{
    advanced::{
        UiActivationSource, UiFrame, UiKey, UiPresentationReceipt, UiRouteRejection, UiSeatId,
        UiSurfaceId, latest_ui_frame, ui_semantic_snapshot,
    },
    prelude::{UiActionConsumerId, UiActionCursor, UiActionId, UiActionJournal, UiSemanticRole},
};

use super::fixture::GameplayProbe;

const PRIMARY_WINDOW: InputWindowId = InputWindowId::new(11);
const SECONDARY_WINDOW: InputWindowId = InputWindowId::new(12);

pub(super) fn prove_pointer_capture_and_surface_isolation(
    app: &mut App,
    primary_surface: UiSurfaceId,
    secondary_surface: UiSurfaceId,
    primary_key: &UiKey,
    paused_cursor: &UiActionCursor,
) -> Result<(), Box<dyn Error>> {
    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(30.0, 30.0))?;
    admit_primary_button(
        app,
        PRIMARY_WINDOW,
        ButtonDriverInput::Press(MouseButton::Left),
    )?;
    app.update()?;

    let primary_semantics =
        ui_semantic_snapshot(app.world(), primary_surface, 1, UiSeatId::PRIMARY)?;
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
        ui_semantic_snapshot(app.world(), secondary_surface, 1, UiSeatId::PRIMARY)?;
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
    app.update()?;
    let released = ui_semantic_snapshot(app.world(), primary_surface, 1, UiSeatId::PRIMARY)?;
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
    app: &mut App,
    action: &UiActionId,
    fast_cursor: &UiActionCursor,
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
    app.update()?;
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
    acknowledge_latest(app.world_mut()?, fast_cursor)?;
    assert_eq!(app.world().resource::<UiActionJournal>().retained_len(), 1);
    Ok(())
}

pub(super) fn prove_non_modal_pass_through(app: &mut App) -> Result<(), Box<dyn Error>> {
    let rejection_count = app
        .world()
        .resource::<nara_ui::UiRouteRejectionLog>()
        .as_slice()
        .len();
    admit_keyboard(app, ButtonDriverInput::Press(KeyCode::Escape))?;
    app.update()?;
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
    app.update()?;
    assert_eq!(
        app.world().resource::<GameplayProbe>().phases(),
        [ActionPhase::Released]
    );
    Ok(())
}

pub(super) fn prove_modal_rejection(app: &mut App) -> Result<(), Box<dyn Error>> {
    admit_keyboard(app, ButtonDriverInput::Press(KeyCode::Escape))?;
    app.update()?;
    assert!(app.world().resource::<GameplayProbe>().phases().is_empty());
    assert!(matches!(
        latest_rejection(app),
        Some(UiRouteRejection::UnhandledInputBlocked)
    ));
    Ok(())
}

pub(super) fn prove_disabled_rejection(
    app: &mut App,
    receipt: UiPresentationReceipt,
    primary_key: &UiKey,
) -> Result<(), Box<dyn Error>> {
    let semantics = ui_semantic_snapshot(
        app.world(),
        receipt.surface_id(),
        receipt.presentation_epoch(),
        UiSeatId::PRIMARY,
    )?;
    let primary = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == primary_key)
        .expect("the disabled primary action should retain semantic identity");
    assert!(!primary.state().enabled());
    assert!(!primary.state().focusable());
    assert!(primary.state().focused());

    admit_pointer(app, PRIMARY_WINDOW, Vec2::new(30.0, 30.0))?;
    app.update()?;
    assert!(matches!(
        latest_rejection(app),
        Some(UiRouteRejection::NonActivation(
            nara_ui::advanced::UiNonActivation::TargetDisabled { .. }
        ))
    ));
    let reconciled = ui_semantic_snapshot(
        app.world(),
        receipt.surface_id(),
        receipt.presentation_epoch(),
        UiSeatId::PRIMARY,
    )?;
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
    app: &App,
    receipt: UiPresentationReceipt,
    primary_key: &UiKey,
) -> Result<(), Box<dyn Error>> {
    let semantics = ui_semantic_snapshot(
        app.world(),
        receipt.surface_id(),
        receipt.presentation_epoch(),
        UiSeatId::PRIMARY,
    )?;
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

pub(super) fn activate_accessibility(app: &mut App) -> Result<(), Box<dyn Error>> {
    let admission = app
        .world_mut()?
        .resource_mut::<InputTimeline>()
        .admit_accessibility_action(PRIMARY_WINDOW, InputSeatId::PRIMARY, 1)?;
    assert_eq!(admission.admitted(), 1);
    app.update()?;
    Ok(())
}

pub(super) fn invalidate_geometry(app: &mut App) -> Result<(), Box<dyn Error>> {
    let admission = app
        .world_mut()?
        .resource_mut::<InputTimeline>()
        .admit_window_event(
            PRIMARY_WINDOW,
            InputSeatId::PRIMARY,
            InputWindowEvent::Resized {
                logical_size: Vec2::new(640.0, 360.0),
            },
        )?;
    assert_eq!(admission.admitted(), 2);
    app.update()?;
    Ok(())
}

pub(super) fn queue_secondary_pointer(app: &mut App) -> Result<(), Box<dyn Error>> {
    admit_pointer(app, SECONDARY_WINDOW, Vec2::new(30.0, 30.0))
}

pub(super) fn latest_rejection(app: &App) -> Option<&UiRouteRejection> {
    app.world()
        .resource::<nara_ui::UiRouteRejectionLog>()
        .as_slice()
        .first()
        .map(|record| record.rejection())
}

fn admit_keyboard(app: &mut App, input: ButtonDriverInput<KeyCode>) -> Result<(), Box<dyn Error>> {
    let admission = app
        .world_mut()?
        .resource_mut::<InputTimeline>()
        .admit_keyboard(PRIMARY_WINDOW, InputSeatId::PRIMARY, input)?;
    assert_eq!(admission.admitted(), 1);
    Ok(())
}

fn admit_pointer(
    app: &mut App,
    window: InputWindowId,
    position: Vec2,
) -> Result<(), Box<dyn Error>> {
    let admission = app
        .world_mut()?
        .resource_mut::<InputTimeline>()
        .admit_pointer(
            window,
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(position),
        )?;
    assert_eq!(admission.admitted(), 1);
    Ok(())
}

fn admit_primary_button(
    app: &mut App,
    window: InputWindowId,
    input: ButtonDriverInput<MouseButton>,
) -> Result<(), Box<dyn Error>> {
    let admission = app
        .world_mut()?
        .resource_mut::<InputTimeline>()
        .admit_mouse_button(window, InputSeatId::PRIMARY, InputPointerId::PRIMARY, input)?;
    assert_eq!(admission.admitted(), 1);
    Ok(())
}

pub(super) fn register_consumers(
    world: &mut World,
) -> Result<(UiActionCursor, UiActionCursor), Box<dyn Error>> {
    let mut journal = world.resource_mut::<UiActionJournal>();
    let fast = journal.register_consumer(UiActionConsumerId::new("interaction-lab.fast")?)?;
    let paused = journal.register_consumer(UiActionConsumerId::new("interaction-lab.paused")?)?;
    Ok((fast, paused))
}

pub(super) fn acknowledge_latest(
    world: &mut World,
    cursor: &UiActionCursor,
) -> Result<(), Box<dyn Error>> {
    let latest = world
        .resource::<UiActionJournal>()
        .pending(cursor)?
        .last()
        .map(|event| event.sequence());
    if let Some(sequence) = latest {
        world
            .resource_mut::<UiActionJournal>()
            .acknowledge(cursor, sequence)?;
    }
    Ok(())
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
    app: &App,
    surface: UiSurfaceId,
    epoch: u64,
    name: &str,
) -> Result<UiKey, Box<dyn Error>> {
    Ok(
        ui_semantic_snapshot(app.world(), surface, epoch, UiSeatId::PRIMARY)?
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
    app: &App,
    surface: UiSurfaceId,
) -> Result<Arc<UiFrame>, Box<dyn Error>> {
    latest_ui_frame(app.world(), surface)?
        .ok_or_else(|| format!("surface {} did not compile a frame", surface.get()).into())
}
