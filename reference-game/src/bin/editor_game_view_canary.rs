use std::{path::Path, process::ExitCode};

use nara::input::KeyCode;
use nara_editor::{
    EditorGameViewCanaryRequest, EditorLaunchOptions, run_game_view_canary_with_intent,
};
use nara_reference_game::wave_game_view_intent;

const PLAYER_SPRITE_SORT_KEY: i32 = 20;

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
        EditorGameViewCanaryRequest::new(PLAYER_SPRITE_SORT_KEY, KeyCode::Character('d')),
    )?;
    let initial = evidence.initial();
    let input_response = evidence.input_response();
    let paused = evidence.paused();
    let stepped = evidence.stepped();
    let resized = evidence.resized();

    if ![
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
            "{{\"schema\":\"nara-reference-game.editor-game-view-v1\",",
            "\"runtime_generation\":{},\"initial_frame\":{},",
            "\"input_frame\":{},\"paused_frame\":{},\"stepped_frame\":{},",
            "\"resized_geometry\":{},\"resized_extent\":[{},{}],",
            "\"sprite_instances\":{},\"runtime_ui\":true,",
            "\"input_delta_x\":{},\"step_delta_x\":{}}}"
        ),
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
