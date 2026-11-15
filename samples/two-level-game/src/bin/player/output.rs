use std::{
    io::{self, Write},
    process::ExitCode,
};

use serde_json::{Value, json};
use two_level_game::Scenario;

pub(super) fn scenario_name(scenario: Scenario) -> &'static str {
    match scenario {
        Scenario::Interactive => "interactive",
        Scenario::Travel => "travel",
        Scenario::Save => "save",
        Scenario::Restore => "restore",
    }
}

pub(super) fn bounded(message: &str) -> String {
    message.chars().take(2048).collect()
}

pub(super) fn failure(scenario: Option<Scenario>, message: &str) -> Value {
    json!({
        "success": false,
        "scenario": scenario.map(scenario_name),
        "message": bounded(message),
        "error": bounded(message),
        "level": null,
        "score": null,
        "collected": null,
        "markers": [],
        "next_marker_id": null,
        "save": null,
        "state": null,
        "visited_levels": [],
        "observed_image_products": [],
        "resident_images": [],
        "image_proofs": [],
        "startup_receipt": null,
        "current_receipt": null,
        "scene_receipts": [],
        "preserved_startup_instance": null,
        "source_free": true,
        "processed_runtime": false,
        "cleanup_complete": true,
    })
}

pub(super) fn emit(result: Value) -> ExitCode {
    let success = result["success"] == true;
    let written = emit_to(&mut io::stdout().lock(), &mut io::stderr().lock(), &result);
    if success && written.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub(super) fn emit_to(
    output: &mut impl Write,
    errors: &mut impl Write,
    result: &Value,
) -> io::Result<()> {
    let diagnostic = if let Some(message) = result["message"].as_str() {
        diagnostic_to(errors, message)
    } else {
        Ok(())
    };
    let record = writeln!(output, "TWO_LEVEL_PLAYER_RESULT {result}").and_then(|()| output.flush());
    diagnostic.and(record)
}

pub(super) fn diagnostic_to(errors: &mut impl Write, message: &str) -> io::Result<()> {
    writeln!(errors, "two_level_player: {message}").and_then(|()| errors.flush())
}
