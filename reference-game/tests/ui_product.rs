#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;
#[path = "support/ui_product_fixture.rs"]
mod ui_product_fixture;

use nara::ui::UiActivationSource;
use nara_reference_game::WaveRetryPhase;
use ui_product_fixture::{
    CANONICAL_UI_TARGET, FRACTIONAL_INPUT_TARGET, LETTERBOX_RESIZE_TARGET, ProductUiCapture,
    ProductUiState, REFERENCE_RETRY_ACTION, RetryActivation, RetryActivationOutcome,
    SemanticCapture, SingleRuntimeRetryTrajectory, UI_PHYSICAL_TARGETS, capture_product_states,
    capture_readability_case, capture_retry_interaction_variants,
    capture_single_runtime_retry_trajectory, exercise_fractional_pointer_retry_activation,
    exercise_paused_retry_activation, exercise_pending_retry_reactivation,
    exercise_resize_cancels_retry_capture, exercise_retry_activation,
    exercise_winit_accessibility_mapping_canary,
};

#[test]
fn one_runtime_retry_trace_binds_exact_commands_to_presented_state() {
    let trace: SingleRuntimeRetryTrajectory =
        capture_single_runtime_retry_trajectory(CANONICAL_UI_TARGET);
    let captures = [
        &trace.running,
        &trace.completed,
        &trace.first_pending,
        &trace.pending_after_reactivation,
        &trace.rejected,
        &trace.second_pending,
        &trace.applied,
    ];

    assert!(captures.windows(2).all(|pair| {
        pair[0].runtime_generation == pair[1].runtime_generation
            && pair[0].presentation_epoch < pair[1].presentation_epoch
    }));
    assert_eq!(
        captures
            .iter()
            .map(|capture| capture.run_generation)
            .collect::<Vec<_>>(),
        vec![1, 1, 1, 1, 1, 1, 2]
    );

    assert_eq!(
        trace.first_activation_receipt.presentation_epoch(),
        trace.completed.presentation_epoch
    );
    assert_eq!(
        trace.first_activation_receipt.frame_revision().get(),
        trace.completed.frame_revision
    );
    assert_eq!(
        trace.first_activation_receipt.route_revision().get(),
        trace.completed.route_revision
    );
    assert_eq!(
        trace.second_activation_receipt.presentation_epoch(),
        trace.rejected.presentation_epoch
    );
    assert_eq!(
        trace.second_activation_receipt.frame_revision().get(),
        trace.rejected.frame_revision
    );
    assert_eq!(
        trace.second_activation_receipt.route_revision().get(),
        trace.rejected.route_revision
    );

    assert_eq!(trace.retry_batches_after_first_activation, vec![1]);
    assert_eq!(trace.retry_batches_after_pending_reactivation, vec![1]);
    assert_eq!(trace.retry_command_batches, vec![1, 1]);
    assert!(!trace.retry_trace_overflowed);

    let retry_key = &trace.completed.retry().expect("completed Retry").key;
    for capture in [
        &trace.completed,
        &trace.first_pending,
        &trace.pending_after_reactivation,
        &trace.rejected,
        &trace.second_pending,
    ] {
        let retry = capture.retry().expect("the terminal trace retains Retry");
        assert_eq!(&retry.key, retry_key, "{capture:#?}");
        assert!(retry.focused, "{capture:#?}");
    }
    assert_eq!(trace.first_pending.state, ProductUiState::RetryPending);
    assert_eq!(
        trace.pending_after_reactivation.state,
        ProductUiState::RetryPending
    );
    assert_eq!(trace.rejected.state, ProductUiState::RetryRejected);
    assert_eq!(trace.second_pending.state, ProductUiState::RetryPending);
    assert_eq!(trace.applied.state, ProductUiState::RetryApplied);
    assert!(trace.applied.retry().is_none(), "{:#?}", trace.applied);
    assert!(!trace.applied.paint_keys.contains(retry_key));
    assert!(!contains_key(&trace.applied.hit_facts, retry_key));
    assert!(!contains_key(&trace.applied.focus_facts, retry_key));
    assert!(
        trace
            .applied
            .semantic_facts
            .iter()
            .all(|fact| &fact.key != retry_key)
    );
}

#[test]
fn canonical_target_covers_the_six_product_states() {
    let capture_set = capture_product_states(CANONICAL_UI_TARGET);
    let running = &capture_set.successful.running;
    let completed = &capture_set.successful.completed;
    let pending = &capture_set.successful.pending;
    let applied = &capture_set.successful.applied;
    let defeated = &capture_set.defeated;
    let rejected = &capture_set.rejected;
    let captures = [running, completed, pending, applied, defeated, rejected];
    assert_eq!(captures.len(), 6);
    for capture in captures {
        assert_eq!(capture.render_scale, CANONICAL_UI_TARGET.render_scale);
        assert_eq!(
            capture.input_scale_factor,
            CANONICAL_UI_TARGET.input_scale_factor
        );
        assert!(capture.frame_revision > 0, "{capture:#?}");
        assert!(capture.source_revision > 0, "{capture:#?}");
        assert!(capture.route_revision > 0, "{capture:#?}");
        assert!(capture.presentation_revision > 0, "{capture:#?}");
        assert!(capture.presentation_epoch > 0, "{capture:#?}");
        assert_capture_bounds(capture, CANONICAL_UI_TARGET);
    }

    assert_eq!(running.state, ProductUiState::Running);
    assert_eq!(completed.state, ProductUiState::Completed);
    assert_eq!(pending.state, ProductUiState::RetryPending);
    assert_eq!(applied.state, ProductUiState::RetryApplied);
    assert_eq!(defeated.state, ProductUiState::Defeated);
    assert_eq!(rejected.state, ProductUiState::RetryRejected);
    assert_eq!(running.runtime_generation, completed.runtime_generation);
    assert_eq!(completed.runtime_generation, pending.runtime_generation);
    assert_eq!(pending.runtime_generation, applied.runtime_generation);
    assert!(running.presentation_epoch < completed.presentation_epoch);
    assert!(completed.presentation_epoch < pending.presentation_epoch);
    assert!(pending.presentation_epoch < applied.presentation_epoch);
    assert_eq!(running.run_generation, 1);
    assert_eq!(completed.run_generation, 1);
    assert_eq!(pending.run_generation, 1);
    assert_eq!(applied.run_generation, 2);

    let rejected_pending = &capture_set.rejected_pending;
    assert_eq!(rejected_pending.state, ProductUiState::RetryPending);
    assert_eq!(
        rejected_pending.runtime_generation,
        rejected.runtime_generation
    );
    assert!(rejected_pending.presentation_epoch < rejected.presentation_epoch);
    assert_eq!(rejected_pending.run_generation, 1);
    assert_eq!(rejected.run_generation, 1);

    assert_eq!(running.run_generation, 1);
    assert!(running.retry().is_none(), "{running:#?}");
    assert_semantic_prefix(running, "HEALTH ");
    assert_semantic_prefix(running, "WAVE ");
    assert_semantic_prefix(running, "SCORE ");
    assert_semantic_prefix(running, "MOVE ");
    assert_semantic_prefix(running, "AUTO-FIRE ");

    assert_semantic_name(completed, "VICTORY");
    assert_semantic_prefix(completed, "FINAL SCORE ");
    assert_semantic_contains(completed, " ENEMIES ");
    assert_semantic_name(completed, "Press Enter or activate Retry");
    assert_enabled_retry(completed, true);
    assert_semantic_name(defeated, "DEFEAT");
    assert_semantic_prefix(defeated, "FINAL SCORE ");
    assert_semantic_contains(defeated, " ENEMIES ");
    assert_semantic_name(defeated, "Press Enter or activate Retry");
    assert_enabled_retry(defeated, true);

    let pending_retry = pending.retry().expect("pending Retry semantics");
    assert_eq!(pending_retry.name, "Retry");
    assert!(!pending_retry.enabled);
    assert!(pending_retry.focused);
    assert!(!pending_retry.focusable);
    assert_eq!(pending_retry.key, completed.retry().unwrap().key);
    assert!(!pending.retry_region_primitives.is_empty());
    assert_ne!(
        pending.retry_region_primitives,
        completed.retry_region_primitives
    );
    assert_semantic_name(pending, "Retry is pending");
    assert_key_state(&pending.hit_facts, &pending_retry.key, false);
    assert_key_state(&pending.focus_facts, &pending_retry.key, false);

    assert_enabled_retry(rejected, true);
    assert_semantic_name(rejected, "Retry replacement was rejected");
    assert_eq!(rejected.retry().unwrap().key, pending_retry.key);
    assert_eq!(rejected_pending.retry().unwrap().key, pending_retry.key);
    assert_ne!(
        rejected.retry_region_primitives,
        pending.retry_region_primitives
    );

    assert_eq!(applied.run_generation, 2);
    assert!(applied.retry().is_none(), "{applied:#?}");
    assert!(applied.retry_region_primitives.is_empty());
    assert!(!has_semantic_name(applied, "VICTORY"));
    assert!(!has_semantic_name(applied, "DEFEAT"));
    let retired_retry_key = &pending_retry.key;
    assert!(!applied.paint_keys.contains(retired_retry_key));
    assert!(!contains_key(&applied.hit_facts, retired_retry_key));
    assert!(!contains_key(&applied.focus_facts, retired_retry_key));
    assert!(applied.semantic_facts.iter().all(|fact| !fact.focused));
    assert!(running.structural_keys().eq(applied.structural_keys()));
    assert_eq!(running.logical_layouts, applied.logical_layouts);

    for capture in captures {
        assert_product_geometry(capture);
    }
}

#[test]
fn three_physical_targets_preserve_one_readable_longest_content_layout() {
    let captures = UI_PHYSICAL_TARGETS.map(capture_readability_case);
    for (target, capture) in UI_PHYSICAL_TARGETS.into_iter().zip(&captures) {
        assert_eq!(capture.state, ProductUiState::RetryRejected);
        assert_eq!(capture.render_scale, target.render_scale);
        assert_eq!(capture.input_scale_factor, target.input_scale_factor);
        assert_semantic_prefix(capture, "AUTO-FIRE ACTIVE");
        assert_semantic_name(capture, "Retry replacement was rejected");
        assert_enabled_retry(capture, true);
        assert_capture_bounds(capture, target);
        assert_product_geometry(capture);
    }

    let baseline = &captures[0];
    for capture in &captures[1..] {
        assert!(capture.structural_keys().eq(baseline.structural_keys()));
        assert_eq!(capture.logical_layouts, baseline.logical_layouts);
        assert_eq!(capture.semantic_facts, baseline.semantic_facts);
    }
}

#[test]
fn one_fractional_input_scale_maps_pointer_to_the_same_retry_intent() {
    assert_eq!(FRACTIONAL_INPUT_TARGET.render_scale, 2.0);
    assert_eq!(FRACTIONAL_INPUT_TARGET.input_scale_factor, 1.25);
    let outcome = exercise_fractional_pointer_retry_activation();
    assert_retry_activation(&outcome, &[UiActivationSource::Pointer]);
}

#[test]
fn component_input_modalities_admit_one_retry_command_per_presented_activation() {
    for activation in [
        RetryActivation::Pointer,
        RetryActivation::Keyboard,
        RetryActivation::Gamepad,
        RetryActivation::ComponentAccessibility,
        RetryActivation::ConcurrentKeyboardAndPointer,
    ] {
        let outcome = exercise_retry_activation(activation);
        let expected_sources = match activation {
            RetryActivation::Pointer => vec![UiActivationSource::Pointer],
            RetryActivation::Keyboard => vec![UiActivationSource::Keyboard],
            RetryActivation::Gamepad => vec![UiActivationSource::Gamepad],
            RetryActivation::ComponentAccessibility => vec![UiActivationSource::Accessibility],
            RetryActivation::ConcurrentKeyboardAndPointer => {
                vec![UiActivationSource::Keyboard, UiActivationSource::Pointer]
            }
        };
        assert_retry_activation(&outcome, &expected_sources);
    }
}

#[test]
fn winit_accessibility_mapper_preserves_the_exact_retry_route_and_command() {
    let outcome = exercise_winit_accessibility_mapping_canary();
    assert_eq!(outcome.source_route, outcome.published_route);
    assert_eq!(outcome.source_route, outcome.mapped_route);
    assert!(outcome.retry_was_published_focused);
    assert_eq!(outcome.retry_batches, vec![1]);
    assert_eq!(outcome.status.phase(), WaveRetryPhase::Pending);
    assert_eq!(outcome.status.last_rejection(), None);
}

#[test]
fn paused_retry_input_is_retained_until_the_runtime_resumes() {
    let (while_paused, after_resume) = exercise_paused_retry_activation();
    assert_eq!(while_paused.phase(), WaveRetryPhase::Idle);
    assert_eq!(while_paused.last_rejection(), None);
    assert_eq!(after_resume.phase(), WaveRetryPhase::Pending);
    assert_eq!(after_resume.last_rejection(), None);
}

#[test]
fn pending_retry_rejects_reactivation_and_applies_exactly_one_generation() {
    let outcome = exercise_pending_retry_reactivation();
    assert_eq!(outcome.pending.phase(), WaveRetryPhase::Pending);
    assert_eq!(outcome.pending.last_rejection(), None);
    assert_eq!(outcome.after_reactivation, outcome.pending);
    assert_eq!(outcome.applied.phase(), WaveRetryPhase::Applied);
    assert_eq!(outcome.applied.last_rejection(), None);
    assert_eq!(outcome.run_generation, 2);
}

#[test]
fn resize_cancels_an_inflight_retry_capture_without_activation() {
    let outcome = exercise_resize_cancels_retry_capture();
    assert_eq!(outcome.retry_status.phase(), WaveRetryPhase::Idle);
    assert!(!outcome.capture.retry().unwrap().pressed);
    assert!(outcome.black_bar_input_blocked);
    assert!((outcome.content_origin.0 - 0.0).abs() < 1.0e-6);
    assert!((outcome.content_origin.1 - 96.0).abs() < 1.0e-6);
    assert!((outcome.content_size.0 - 1_024.0).abs() < 1.0e-6);
    assert!((outcome.content_size.1 - 576.0).abs() < 1.0e-6);
    assert!((outcome.content_scale_factor - 1_024.0 / 960.0).abs() < 1.0e-6);
    assert_eq!(outcome.capture.render_scale, 1_024.0 / 960.0);
    assert_capture_bounds(&outcome.capture, LETTERBOX_RESIZE_TARGET);
    assert_product_geometry(&outcome.capture);
    let packet_bounds = outcome.capture.packet_bounds.unwrap();
    assert!(packet_bounds.min_y() >= 96, "{:#?}", outcome.capture);
    assert!(packet_bounds.max_y() <= 672, "{:#?}", outcome.capture);
}

#[test]
fn retry_exposes_exact_and_distinct_default_hover_pressed_and_focus_facts() {
    let variants = capture_retry_interaction_variants();
    let default = variants.default.retry().expect("default Retry");
    let hover = variants.hovered.retry().expect("hover Retry");
    let pressed = variants.pressed.retry().expect("pressed Retry");
    let focused = variants.focused.retry().expect("focused Retry");

    assert!(default.enabled && !default.pressed);
    assert!(hover.enabled && !hover.pressed);
    assert!(pressed.enabled && pressed.pressed);
    assert!(focused.enabled && focused.focused);
    for variant in [
        &variants.default,
        &variants.hovered,
        &variants.pressed,
        &variants.focused,
    ] {
        assert!(!variant.retry_region_primitives.is_empty());
        assert!(
            variant
                .structural_keys()
                .eq(variants.default.structural_keys())
        );
    }
    assert_ne!(
        variants.default.retry_region_primitives,
        variants.hovered.retry_region_primitives
    );
    assert_ne!(
        variants.hovered.retry_region_primitives,
        variants.pressed.retry_region_primitives
    );
    assert_ne!(
        variants.pressed.retry_region_primitives,
        variants.focused.retry_region_primitives
    );
}

fn assert_retry_activation(
    outcome: &RetryActivationOutcome,
    expected_sources: &[UiActivationSource],
) {
    let receipt = outcome.activation_receipt;
    assert!(!outcome.ui_action_trace_overflowed, "{outcome:#?}");
    assert!(!outcome.retry_trace_overflowed, "{outcome:#?}");
    assert_eq!(
        outcome
            .ui_actions
            .iter()
            .map(|event| event.source())
            .collect::<Vec<_>>(),
        expected_sources,
        "{outcome:#?}"
    );
    for event in &outcome.ui_actions {
        assert_eq!(event.surface_id(), receipt.surface_id(), "{outcome:#?}");
        assert_eq!(
            event.presentation_epoch(),
            receipt.presentation_epoch(),
            "{outcome:#?}"
        );
        assert_eq!(
            event.frame_revision(),
            receipt.frame_revision().get(),
            "{outcome:#?}"
        );
        assert_eq!(event.target(), &outcome.retry_key, "{outcome:#?}");
        assert_eq!(
            event.action().as_str(),
            REFERENCE_RETRY_ACTION,
            "{outcome:#?}"
        );
    }
    assert_eq!(outcome.retry_batches, vec![1], "{outcome:#?}");
    assert_eq!(outcome.status.phase(), WaveRetryPhase::Pending, "{outcome:#?}");
    assert_eq!(outcome.status.last_rejection(), None, "{outcome:#?}");
}

fn has_semantic_name(capture: &ProductUiCapture, name: &str) -> bool {
    capture.semantic_facts.iter().any(|fact| fact.name == name)
}

fn assert_semantic_name(capture: &ProductUiCapture, name: &str) {
    assert!(
        has_semantic_name(capture, name),
        "{name:?} missing from {capture:#?}"
    );
}

fn assert_semantic_prefix(capture: &ProductUiCapture, prefix: &str) {
    assert!(
        capture
            .semantic_facts
            .iter()
            .any(|fact| fact.name.starts_with(prefix)),
        "semantic prefix {prefix:?} missing from {capture:#?}"
    );
}

fn assert_semantic_contains(capture: &ProductUiCapture, fragment: &str) {
    assert!(
        capture
            .semantic_facts
            .iter()
            .any(|fact| fact.name.contains(fragment)),
        "semantic fragment {fragment:?} missing from {capture:#?}"
    );
}

fn assert_enabled_retry(capture: &ProductUiCapture, expected_focus: bool) {
    let retry = capture.retry().expect("enabled Retry semantics");
    assert_eq!(retry.name, "Retry");
    assert!(retry.enabled);
    assert!(retry.focusable);
    assert_eq!(retry.focused, expected_focus);
    assert!(retry.width >= 44.0, "{retry:#?}");
    assert!(retry.height >= 36.0, "{retry:#?}");
    assert!(retry.x >= 0.0 && retry.y >= 0.0, "{retry:#?}");
    assert!(retry.x + retry.width <= 960.0, "{retry:#?}");
    assert!(retry.y + retry.height <= 540.0, "{retry:#?}");
    assert!(capture.paint_keys.contains(&retry.key));
    assert_key_state(&capture.hit_facts, &retry.key, true);
    assert_key_state(&capture.focus_facts, &retry.key, true);
}

fn assert_capture_bounds(capture: &ProductUiCapture, target: ui_product_fixture::UiPhysicalTarget) {
    assert!(capture.packet_primitives > 0, "{capture:#?}");
    assert!(capture.glyph_pages > 0, "{capture:#?}");
    assert!(capture.has_nonblank_glyphs, "{capture:#?}");
    let packet_bounds = capture
        .packet_bounds
        .expect("a nonempty product packet should have bounds");
    assert!(packet_bounds.min_x() >= 0, "{capture:#?}");
    assert!(packet_bounds.min_y() >= 0, "{capture:#?}");
    assert!(packet_bounds.max_x() <= target.width as i32, "{capture:#?}");
    assert!(
        packet_bounds.max_y() <= target.height as i32,
        "{capture:#?}"
    );
    for layout in &capture.logical_layouts {
        if !layout.visible {
            continue;
        }
        assert!(layout.x >= 0.0 && layout.y >= 0.0, "{layout:#?}");
        assert!(layout.width > 0.0 && layout.height > 0.0, "{layout:#?}");
        assert!(layout.x + layout.width <= 960.0, "{layout:#?}");
        assert!(layout.y + layout.height <= 540.0, "{layout:#?}");
    }
    for text in &capture.text_layouts {
        assert!(
            text.layout_width <= text.allocated_width + 1.0e-4,
            "{text:#?}"
        );
        assert!(
            text.layout_height <= text.allocated_height + 1.0e-4,
            "{text:#?}"
        );
    }
}

fn assert_product_geometry(capture: &ProductUiCapture) {
    let hud = capture
        .semantic_facts
        .iter()
        .filter(|fact| is_hud_fact(fact))
        .collect::<Vec<_>>();
    assert_eq!(hud.len(), 5, "{capture:#?}");
    for fact in &hud {
        assert!(fact.x >= 24.0 && fact.y >= 24.0, "{fact:#?}");
        assert!(fact.x + fact.width <= 936.0, "{fact:#?}");
        assert!(fact.y + fact.height <= 516.0, "{fact:#?}");
        assert!(capture.paint_keys.contains(&fact.key), "{fact:#?}");
    }
    assert_non_overlapping(&hud);

    let terminal = capture
        .semantic_facts
        .iter()
        .filter(|fact| is_terminal_fact(fact))
        .collect::<Vec<_>>();
    if capture.retry().is_some() {
        assert!(hud.iter().all(|fact| fact.inert), "{capture:#?}");
        assert_eq!(terminal.len(), 4, "{capture:#?}");
        assert!(has_visible_layout(capture, 300.0, 140.0, 360.0, 260.0));
        assert!(
            capture
                .logical_layouts
                .iter()
                .filter(|layout| {
                    layout.visible
                        && approximately(layout.x, 0.0)
                        && approximately(layout.y, 0.0)
                        && approximately(layout.width, 960.0)
                        && approximately(layout.height, 540.0)
                })
                .count()
                >= 3,
            "the HUD, modal root, and dim underlay must cover the viewport: {capture:#?}"
        );
        for fact in &terminal {
            assert!(fact.x >= 300.0 && fact.y >= 140.0, "{fact:#?}");
            assert!(fact.x + fact.width <= 660.0, "{fact:#?}");
            assert!(fact.y + fact.height <= 400.0, "{fact:#?}");
            assert!(capture.paint_keys.contains(&fact.key), "{fact:#?}");
        }
        assert_non_overlapping(&terminal);
    } else {
        assert!(hud.iter().all(|fact| !fact.inert), "{capture:#?}");
        assert!(terminal.is_empty(), "{capture:#?}");
        assert!(!has_visible_layout(capture, 300.0, 140.0, 360.0, 260.0));
    }
}

fn is_hud_fact(fact: &SemanticCapture) -> bool {
    ["HEALTH ", "WAVE ", "SCORE ", "MOVE ", "AUTO-FIRE "]
        .iter()
        .any(|prefix| fact.name.starts_with(prefix))
}

fn is_terminal_fact(fact: &SemanticCapture) -> bool {
    fact.name == "VICTORY"
        || fact.name == "DEFEAT"
        || fact.name.starts_with("FINAL SCORE ")
        || fact
            .actions
            .iter()
            .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
        || fact.name == "Press Enter or activate Retry"
        || fact.name == "Retry is pending"
        || fact.name == "Retry replacement was rejected"
}

fn assert_non_overlapping(facts: &[&SemanticCapture]) {
    for (index, first) in facts.iter().enumerate() {
        for second in &facts[index + 1..] {
            let overlaps = first.x < second.x + second.width
                && first.x + first.width > second.x
                && first.y < second.y + second.height
                && first.y + first.height > second.y;
            assert!(
                !overlaps,
                "semantic facts overlap: {first:#?} and {second:#?}"
            );
        }
    }
}

fn has_visible_layout(capture: &ProductUiCapture, x: f32, y: f32, width: f32, height: f32) -> bool {
    capture.logical_layouts.iter().any(|layout| {
        layout.visible
            && approximately(layout.x, x)
            && approximately(layout.y, y)
            && approximately(layout.width, width)
            && approximately(layout.height, height)
    })
}

fn approximately(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1.0e-4
}

fn assert_key_state(
    facts: &[ui_product_fixture::KeyStateCapture],
    key: &nara::ui::advanced::UiKey,
    enabled: bool,
) {
    let fact = facts
        .iter()
        .find(|fact| &fact.key == key)
        .expect("the Retry key should remain in the immutable frame index");
    assert_eq!(fact.enabled, enabled, "{fact:#?}");
}

fn contains_key(
    facts: &[ui_product_fixture::KeyStateCapture],
    key: &nara::ui::advanced::UiKey,
) -> bool {
    facts.iter().any(|fact| &fact.key == key)
}
