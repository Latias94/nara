use std::{error::Error, sync::Arc};

use nara_ui::{
    advanced::{UiActivationSource, UiRouteRejection},
    prelude::{UiActionId, UiActionJournal, UiButton, UiLayer},
};

#[path = "interaction_lab/fixture.rs"]
mod fixture;
#[path = "interaction_lab/presenter.rs"]
mod presenter;
#[path = "interaction_lab/probes.rs"]
mod probes;
#[path = "interaction_lab/runtime.rs"]
mod runtime;

use fixture::{build_lab, surface_id};
use probes::{
    acknowledge_latest, activate_accessibility, assert_latest_action, assert_structural_order,
    invalidate_geometry, latest_rejection, prove_disabled_rejection, prove_modal_rejection,
    prove_non_modal_pass_through, prove_pointer_activation,
    prove_pointer_capture_and_surface_isolation, prove_reenabled_focus, queue_secondary_pointer,
    register_consumers, required_frame, semantic_key,
};
use runtime::LabRuntime;

const DOMAIN: &str = "nara.example.interaction-lab";
const PRIMARY_ACTION: &str = "gallery.primary";
const UPDATED_ACTION: &str = "gallery.updated";
const GAMEPLAY_ACTION: &str = "gallery.pass-through";

fn main() -> Result<(), Box<dyn Error>> {
    let (mut app, entities) = build_lab()?;
    let (fast_cursor, paused_cursor) = register_consumers(app.world_mut()?)?;
    let mut app = LabRuntime::start(app, entities)?;
    app.tick()?;

    let primary_surface = surface_id(1);
    let secondary_surface = surface_id(2);
    let primary_frame = required_frame(&app, primary_surface)?;
    let secondary_frame = required_frame(&app, secondary_surface)?;
    assert_structural_order(&primary_frame);
    assert_eq!(primary_frame.environment().scale_factor(), 1.0);
    assert_eq!(secondary_frame.environment().scale_factor(), 1.25);

    let primary_receipt = app.present(Arc::clone(&primary_frame), 1)?;
    let secondary_receipt = app.present(Arc::clone(&secondary_frame), 1)?;

    let primary_key = semantic_key(&app, primary_receipt, "Primary action")?;
    let primary_action = UiActionId::new(PRIMARY_ACTION)?;

    prove_pointer_capture_and_surface_isolation(
        &mut app,
        primary_receipt,
        secondary_receipt,
        &primary_key,
        &paused_cursor,
    )?;
    prove_pointer_activation(&mut app, &primary_action, &fast_cursor, &paused_cursor)?;
    prove_non_modal_pass_through(&mut app)?;

    app.set_primary_button(UiButton::new(
        "Primary action",
        UiActionId::new(UPDATED_ACTION)?,
        16.0,
    )?)?;
    activate_accessibility(&mut app, primary_receipt, &primary_key, &primary_action)?;
    assert_latest_action(
        app.world(),
        &fast_cursor,
        &primary_action,
        UiActivationSource::Accessibility,
    )?;
    acknowledge_latest(&mut app, &fast_cursor)?;

    app.tick()?;
    let updated_frame = required_frame(&app, primary_surface)?;
    let updated_receipt = app.present(Arc::clone(&updated_frame), 2)?;
    let updated_action = UiActionId::new(UPDATED_ACTION)?;
    activate_accessibility(&mut app, updated_receipt, &primary_key, &updated_action)?;
    assert_latest_action(
        app.world(),
        &fast_cursor,
        &updated_action,
        UiActivationSource::Accessibility,
    )?;
    acknowledge_latest(&mut app, &fast_cursor)?;

    invalidate_geometry(&mut app)?;

    app.set_primary_button(
        UiButton::new("Primary action", updated_action, 16.0)?.with_enabled(false),
    )?;
    app.tick()?;
    let disabled_frame = required_frame(&app, primary_surface)?;
    let disabled_receipt = app.present(disabled_frame, 3)?;
    prove_disabled_rejection(&mut app, disabled_receipt, &primary_key)?;

    app.set_primary_button(UiButton::new(
        "Primary action",
        UiActionId::new(UPDATED_ACTION)?,
        16.0,
    )?)?;
    app.tick()?;
    let reenabled_frame = required_frame(&app, primary_surface)?;
    let reenabled_receipt = app.present(reenabled_frame, 4)?;
    prove_reenabled_focus(&app, reenabled_receipt, &primary_key)?;

    app.set_overlay_layer(UiLayer::Modal)?;
    app.tick()?;
    let modal_frame = required_frame(&app, primary_surface)?;
    app.present(modal_frame, 5)?;
    prove_modal_rejection(&mut app)?;

    let retained = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&paused_cursor)?;
    assert_eq!(retained.len(), 3);
    acknowledge_latest(&mut app, &paused_cursor)?;
    assert_eq!(app.world().resource::<UiActionJournal>().retained_len(), 0);

    queue_secondary_pointer(&mut app)?;
    app.retire_surface(secondary_surface)?;
    app.tick()?;
    assert!(matches!(
        latest_rejection(&app),
        Some(UiRouteRejection::RetiredSurface { surface: 2 })
    ));

    app.close()?;

    println!(
        "interaction lab: scene+ecs authoring, two surfaces, timeline arbitration, capture/focus/modal, exact behavior, accessibility, fail-closed routing, retained cursors"
    );
    Ok(())
}
