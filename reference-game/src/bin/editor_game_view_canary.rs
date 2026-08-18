use std::{path::Path, process::ExitCode};

use nara::{input::KeyCode, scene::SceneEntityId};
use nara_editor::{
    EditorGameViewCanaryRequest, EditorLaunchOptions, EditorUiAuthoringCanaryRequest,
    run_game_view_canary_with_intent,
};
use nara_reference_game::wave_game_view_intent;

const PLAYER_SPRITE_SORT_KEY: i32 = 20;
const UI_MOVE_DELTA: [f64; 2] = [12.0, 8.0];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("editor_game_view_canary: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let evidence = run_game_view_canary_with_intent(
        EditorLaunchOptions::new(Path::new(env!("CARGO_MANIFEST_DIR"))),
        wave_game_view_intent(),
        EditorGameViewCanaryRequest::new(PLAYER_SPRITE_SORT_KEY, KeyCode::Character('d'))
            .with_ui_authoring(EditorUiAuthoringCanaryRequest::new(
                SceneEntityId::new("reference-ui")?,
                SceneEntityId::new("autofire-status")?,
                SceneEntityId::new("reference-hud")?,
                UI_MOVE_DELTA,
            )),
    )?;
    let ui = evidence
        .ui_authoring()
        .ok_or("the native Editor canary omitted UI authoring evidence")?;
    let selected_ui = ui.selected();
    let previewed_ui = ui.previewed();
    let committed_ui = ui.committed();
    let structured_ui = ui.structured();
    let initial = evidence.initial();
    let input_response = evidence.input_response();
    let paused = evidence.paused();
    let stepped = evidence.stepped();
    let resized = evidence.resized();

    let preview_delta = [
        previewed_ui.node_origin()[0] - selected_ui.node_origin()[0],
        previewed_ui.node_origin()[1] - selected_ui.node_origin()[1],
    ];
    let committed_delta = [
        committed_ui.node_origin()[0] - previewed_ui.node_origin()[0],
        committed_ui.node_origin()[1] - previewed_ui.node_origin()[1],
    ];
    if selected_ui.fact_count() == 0
        || selected_ui.extent().contains(&0)
        || selected_ui.document() != previewed_ui.document()
        || selected_ui.document() != committed_ui.document()
        || selected_ui.document() != structured_ui.document()
        || selected_ui.revision() != previewed_ui.revision()
        || committed_ui.revision() == selected_ui.revision()
        || structured_ui.revision().source_id() != committed_ui.revision().source_id()
        || structured_ui.revision().generation()
            != committed_ui.revision().generation().checked_add(1).ok_or(
                "the native UI authoring revision generation exhausted",
            )?
        || selected_ui.gesture().is_some()
        || previewed_ui
            .gesture()
            .is_none_or(|gesture| gesture.preview_generation() == 0)
        || committed_ui.gesture().is_some()
        || structured_ui.gesture().is_some()
        || previewed_ui.editor_frame_index() <= selected_ui.editor_frame_index()
        || committed_ui.editor_frame_index() <= previewed_ui.editor_frame_index()
        || structured_ui.editor_frame_index() <= committed_ui.editor_frame_index()
        || previewed_ui.presentation_generation() <= selected_ui.presentation_generation()
        || committed_ui.presentation_generation() <= previewed_ui.presentation_generation()
        || structured_ui.presentation_generation() <= committed_ui.presentation_generation()
        || preview_delta
            .into_iter()
            .zip(UI_MOVE_DELTA)
            .any(|(observed, expected)| (f64::from(observed) - expected).abs() > 0.01)
        || committed_delta
            .into_iter()
            .any(|observed| observed.abs() > 0.01)
        || structured_ui.fact_count() != committed_ui.fact_count().saturating_add(1)
        || ui.created_entity() == &SceneEntityId::new("autofire-status")?
        || ui.undo_depth() != 2
        || !ui.stale_pick_rejected()
        || !ui.structural_stale_pick_rejected()
        || ![
            initial.runtime_ui_present(),
            input_response.runtime_ui_present(),
            paused.runtime_ui_present(),
            stepped.runtime_ui_present(),
            resized.runtime_ui_present(),
        ]
        .into_iter()
        .all(|present| present)
        || [
            initial.sprite_instances(),
            input_response.sprite_instances(),
            paused.sprite_instances(),
            stepped.sprite_instances(),
            resized.sprite_instances(),
        ]
        .into_iter()
        .any(|count| count == 0)
        || input_response.runtime_frame_index() <= initial.runtime_frame_index()
        || paused.runtime_frame_index() <= input_response.runtime_frame_index()
        || resized.runtime_frame_index() <= paused.runtime_frame_index()
        || stepped.runtime_frame_index() <= resized.runtime_frame_index()
        || resized.geometry_generation() <= paused.geometry_generation()
        || resized.target_generation() <= paused.target_generation()
        || resized.presentation_generation() <= paused.presentation_generation()
        || resized.extent() == paused.extent()
        || evidence.input_delta_x() <= f32::EPSILON
        || evidence.step_delta_x() <= f32::EPSILON
    {
        return Err("the native Game View canary evidence was incomplete".into());
    }

    println!(
        concat!(
            "{{\"schema\":\"nara-reference-game.editor-game-view-v2\",",
            "\"ui_document\":{},\"ui_selected_frame\":{},",
            "\"ui_preview_frame\":{},\"ui_committed_frame\":{},",
            "\"ui_structured_frame\":{},\"ui_created_entity\":\"{}\",",
            "\"ui_fact_count\":{},\"ui_move_delta\":[{},{}],\"ui_undo_depth\":{},",
            "\"runtime_generation\":{},\"initial_frame\":{},",
            "\"input_frame\":{},\"paused_frame\":{},\"stepped_frame\":{},",
            "\"resized_geometry\":{},\"resized_extent\":[{},{}],",
            "\"sprite_instances\":{},\"runtime_ui\":true,",
            "\"input_delta_x\":{},\"step_delta_x\":{}}}"
        ),
        selected_ui.document().raw(),
        selected_ui.editor_frame_index(),
        previewed_ui.editor_frame_index(),
        committed_ui.editor_frame_index(),
        structured_ui.editor_frame_index(),
        ui.created_entity().as_str(),
        structured_ui.fact_count(),
        UI_MOVE_DELTA[0],
        UI_MOVE_DELTA[1],
        ui.undo_depth(),
        initial.runtime_generation(),
        initial.runtime_frame_index(),
        input_response.runtime_frame_index(),
        paused.runtime_frame_index(),
        stepped.runtime_frame_index(),
        resized.geometry_generation(),
        resized.extent()[0],
        resized.extent()[1],
        resized.sprite_instances(),
        evidence.input_delta_x(),
        evidence.step_delta_x(),
    );
    println!("editor_game_view_canary: ok");
    Ok(())
}
