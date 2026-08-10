use std::{error::Error, sync::Arc};

use nara_ui::{
    advanced::{UiActivationSource, UiRouteRejection, retire_ui_surface},
    prelude::{UiActionId, UiActionJournal, UiButton, UiLayer, UiRoot},
};

#[path = "interaction_lab/fixture.rs"]
mod fixture;
#[path = "interaction_lab/probes.rs"]
mod probes;

use fixture::{build_lab, surface_id};
use probes::{
    acknowledge_latest, activate_accessibility, assert_latest_action, assert_structural_order,
    invalidate_geometry, latest_rejection, prove_disabled_rejection, prove_modal_rejection,
    prove_non_modal_pass_through, prove_pointer_activation,
    prove_pointer_capture_and_surface_isolation, prove_reenabled_focus, queue_secondary_pointer,
    register_consumers, required_frame, semantic_key,
};

const DOMAIN: &str = "nara.example.interaction-lab";
const PRIMARY_ACTION: &str = "gallery.primary";
const UPDATED_ACTION: &str = "gallery.updated";
const GAMEPLAY_ACTION: &str = "gallery.pass-through";

fn main() -> Result<(), Box<dyn Error>> {
    let (mut app, entities) = build_lab()?;
    app.update()?;

    let primary_surface = surface_id(1);
    let secondary_surface = surface_id(2);
    let primary_frame = required_frame(&app, primary_surface)?;
    let secondary_frame = required_frame(&app, secondary_surface)?;
    assert_structural_order(&primary_frame);
    assert_eq!(primary_frame.environment().scale_factor(), 1.0);
    assert_eq!(secondary_frame.environment().scale_factor(), 1.25);

    let primary_receipt =
        present_for_interaction_lab(app.world_mut()?, Arc::clone(&primary_frame), 1)?;
    let secondary_receipt =
        present_for_interaction_lab(app.world_mut()?, Arc::clone(&secondary_frame), 1)?;

    let (fast_cursor, paused_cursor) = register_consumers(app.world_mut()?)?;
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

    app.world_mut()?
        .entity_mut(entities.primary_button)
        .insert(UiButton::new(
            "Primary action",
            UiActionId::new(UPDATED_ACTION)?,
            16.0,
        )?);
    activate_accessibility(&mut app, primary_receipt, &primary_key, &primary_action)?;
    assert_latest_action(
        app.world(),
        &fast_cursor,
        &primary_action,
        UiActivationSource::Accessibility,
    )?;
    acknowledge_latest(app.world_mut()?, &fast_cursor)?;

    app.update()?;
    let updated_frame = required_frame(&app, primary_surface)?;
    let updated_receipt =
        present_for_interaction_lab(app.world_mut()?, Arc::clone(&updated_frame), 2)?;
    let updated_action = UiActionId::new(UPDATED_ACTION)?;
    activate_accessibility(&mut app, updated_receipt, &primary_key, &updated_action)?;
    assert_latest_action(
        app.world(),
        &fast_cursor,
        &updated_action,
        UiActivationSource::Accessibility,
    )?;
    acknowledge_latest(app.world_mut()?, &fast_cursor)?;

    invalidate_geometry(&mut app)?;

    app.world_mut()?
        .entity_mut(entities.primary_button)
        .insert(UiButton::new("Primary action", updated_action, 16.0)?.with_enabled(false));
    app.update()?;
    let disabled_frame = required_frame(&app, primary_surface)?;
    let disabled_receipt = present_for_interaction_lab(app.world_mut()?, disabled_frame, 3)?;
    prove_disabled_rejection(&mut app, disabled_receipt, &primary_key)?;

    app.world_mut()?
        .entity_mut(entities.primary_button)
        .insert(UiButton::new(
            "Primary action",
            UiActionId::new(UPDATED_ACTION)?,
            16.0,
        )?);
    app.update()?;
    let reenabled_frame = required_frame(&app, primary_surface)?;
    let reenabled_receipt = present_for_interaction_lab(app.world_mut()?, reenabled_frame, 4)?;
    prove_reenabled_focus(&app, reenabled_receipt, &primary_key)?;

    app.world_mut()?
        .entity_mut(entities.scene_overlay)
        .insert(UiRoot::new(UiLayer::Modal));
    app.update()?;
    let modal_frame = required_frame(&app, primary_surface)?;
    present_for_interaction_lab(app.world_mut()?, modal_frame, 5)?;
    prove_modal_rejection(&mut app)?;

    let retained = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&paused_cursor)?;
    assert_eq!(retained.len(), 3);
    let paused_tail = retained
        .last()
        .expect("three actions are retained")
        .sequence();
    app.world_mut()?
        .resource_mut::<UiActionJournal>()
        .acknowledge(&paused_cursor, paused_tail)?;
    assert_eq!(app.world().resource::<UiActionJournal>().retained_len(), 0);

    queue_secondary_pointer(&mut app)?;
    assert!(retire_ui_surface(app.world_mut()?, secondary_surface)?.is_some());
    app.update()?;
    assert!(matches!(
        latest_rejection(&app),
        Some(UiRouteRejection::RetiredSurface { surface: 2 })
    ));

    println!(
        "interaction lab: scene+ecs authoring, two surfaces, timeline arbitration, capture/focus/modal, exact behavior, accessibility, fail-closed routing, retained cursors"
    );
    Ok(())
}

fn present_for_interaction_lab(
    world: &mut nara_ecs::World,
    frame: Arc<nara_ui_kernel::UiFrame>,
    presentation_epoch: u64,
) -> Result<nara_ui::UiPresentationReceipt, nara_ui::UiAdapterError> {
    let prepared = nara_ui::__private::prepare_ui_presentation(world, frame, presentation_epoch)?;
    Ok(nara_ui::__private::commit_prepared_ui_presentation(
        world, prepared,
    ))
}
