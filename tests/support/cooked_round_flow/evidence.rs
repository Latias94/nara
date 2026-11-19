use serde_json::{Value, json};
use two_level_game::levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey};

use super::Request;

pub fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing unsigned {field}: {value}"))
}

pub fn presented(record: &Value, request: &Request, previous: Option<&Value>) {
    for field in [
        "success",
        "source_free",
        "processed_runtime",
        "first_present_verified",
    ] {
        assert_eq!(record[field], true, "missing original {field}: {record}");
    }
    assert_eq!(record["observations_truncated"], false);
    assert_eq!(record["scenario"], "interactive");
    assert_eq!(record["content_target"], request.target);
    assert_eq!(record["content_profile"], request.content_profile);
    assert_eq!(record["catalog_hash"], request.catalog_digest["hash"]);
    assert_eq!(record["catalog_length"], request.catalog_digest["length"]);
    assert_eq!(
        record["controllers"],
        json!({"backend": "none", "connected": 0})
    );
    let source_input: Value = serde_json::from_slice(include_bytes!(
        "../../../samples/two-level-game/input/explorer.actions.json"
    ))
    .unwrap();
    assert_eq!(
        record["input_proof"],
        json!({
            "product": two_level_game::input::input_product().to_string(),
            "source_free": true, "definition": source_input["payload"],
        })
    );
    assert_eq!(record["flow"]["game_flow"], "round");
    assert!(record["flow"]["error"].is_null(), "{record}");
    assert!(record["state"]["error"].is_null(), "{record}");
    assert_eq!(record["adapter"]["backend"], "dx12");
    assert!(
        matches!(
            record["adapter"]["device_type"].as_str(),
            Some("discrete-gpu" | "integrated-gpu")
        ),
        "native round acceptance requires actual hardware: {record}"
    );
    assert!(!record["adapter"]["name"].as_str().unwrap().is_empty());
    let frame = &record["presentation"];
    for field in [
        "runtime_generation",
        "app_frame",
        "render_frame",
        "window_id",
        "surface_generation",
        "backend_instance",
        "device_epoch",
        "queue_submissions",
        "presents",
    ] {
        assert!(number(frame, field) > 0, "unpresented {field}: {record}");
    }
    if let Some(previous) = previous {
        for field in [
            "runtime_generation",
            "window_id",
            "surface_generation",
            "backend_instance",
            "device_epoch",
        ] {
            assert_eq!(
                frame[field], previous["presentation"][field],
                "original native owner changed"
            );
        }
        assert_eq!(record["adapter"], previous["adapter"]);
        for field in ["app_frame", "render_frame"] {
            assert!(number(frame, field) >= number(&previous["presentation"], field));
        }
        if previous["initial_title_presentation"].is_object() {
            assert_eq!(
                record["initial_title_presentation"],
                previous["initial_title_presentation"]
            );
        }
    }
    let initial = &record["initial_title_presentation"];
    if record["ready"] == true {
        assert!(
            initial.is_object(),
            "READY must retain its original presented Title: {record}"
        );
    }
    if initial.is_object() {
        assert_eq!(initial["phase"], "Title");
        assert_eq!(initial["round"], 0);
        assert_eq!(initial["presented_ui"]["matches_snapshot"], true);
        assert_eq!(
            initial["presented_ui"]["heading"],
            "Two-Level Exploration | Title | Round 0"
        );
        let receipt = &initial["presented_ui"]["receipt"];
        for field in [
            "runtime_generation",
            "window_id",
            "surface_generation",
            "backend_instance",
            "device_epoch",
        ] {
            assert_eq!(receipt[field], frame[field]);
        }
        for field in ["app_frame", "render_frame"] {
            assert!(number(receipt, field) <= number(frame, field));
        }
        assert!(number(receipt, "submitted_glyph_quads") > 0);
    }
    let ui = &record["flow"]["presented_ui"];
    if ui.is_object() {
        let receipt = &ui["receipt"];
        for field in [
            "runtime_generation",
            "app_frame",
            "render_frame",
            "window_id",
            "surface_generation",
            "backend_instance",
            "device_epoch",
        ] {
            assert_eq!(
                receipt[field], frame[field],
                "UI/native original submission disagrees: {record}"
            );
        }
        for field in [
            "surface_id",
            "presentation_epoch",
            "submitted_quads",
            "submitted_glyph_quads",
            "glyph_pages",
        ] {
            assert!(
                number(receipt, field) > 0,
                "unsubmitted original UI {field}: {record}"
            );
        }
    }
}

pub fn shown(record: &Value, phase: &str, round: u32) -> bool {
    let flow = &record["flow"];
    let ui = &flow["presented_ui"];
    let heading = format!("Two-Level Exploration | {phase} | Round {round}");
    flow["phase"] == phase
        && flow["round"] == round
        && ui["matches_snapshot"] == true
        && ui["heading"] == heading
        && record["state"]["initialized"] == true
        && record["state"]["phase"] == "Running"
        && record["state"]["restoring"] == false
}

pub fn assert_shown(record: &Value, phase: &str, round: u32, score: u32, goals: [bool; 2]) {
    assert!(
        shown(record, phase, round),
        "expected displayed {phase} round {round}: {record}"
    );
    assert_eq!(record["score"], score);
    assert_eq!(record["collected"], score / 10);
    assert_eq!(
        record["flow"]["goals"],
        json!({"meadow_collected": goals[0], "cavern_collected": goals[1]})
    );
    assert_eq!(record["state"]["scene_members"], AUTHORED_LEVEL_MEMBERS);
    for field in ["level", "score", "collected", "markers"] {
        assert_eq!(record[field], record["state"][field]);
    }
    let current = &record["current_receipt"];
    assert!(number(current, "instance_id") > 0 && number(current, "domain_id") > 0);
    assert_eq!(current["entity_count"], AUTHORED_LEVEL_MEMBERS);
    assert_eq!(current["truncated"], false);
    assert_eq!(current["instance_id"], record["state"]["current_instance"]);
    assert_eq!(current["domain_id"], record["startup_receipt"]["domain_id"]);
    let receipts = record["scene_receipts"].as_array().unwrap();
    assert!(!receipts.is_empty() && receipts.len() <= 16);
    assert_eq!(receipts.last().unwrap()["receipt"], *current);
    let level: LevelKey = serde_json::from_value(record["level"].clone()).unwrap();
    assert_eq!(
        receipts.last().unwrap()["scene_product"],
        level.level().scene_product().to_string()
    );
    assert_eq!(receipts.last().unwrap()["representation"], "runtime_scene");
    let collected = |value| if value { "collected" } else { "needed" };
    let goal_text = format!(
        "Goal: 1 pickup per main level | Meadow: {} | Cavern: {}",
        collected(goals[0]),
        collected(goals[1])
    );
    let controls = match phase {
        "Title" => "Enter: start | X: exit",
        "Playing" => "WASD: move / collect gems | E: travel | Escape: pause | X: exit",
        "Paused" => "Enter / Escape: continue | T: title | X: exit",
        "Complete" => "Enter: restart | T: title | X: exit",
        _ => panic!("unexpected acceptance phase: {phase}"),
    };
    let text = record["flow"]["presented_ui"]["text"].as_array().unwrap();
    assert!(text.contains(&json!(goal_text)) && text.contains(&json!(controls)));
    assert!(
        number(
            &record["flow"]["presented_ui"]["receipt"],
            "submitted_glyph_quads"
        ) > 0
    );
}

pub fn initial_title(record: &Value, pending_save: bool) {
    assert_shown(record, "Title", 0, 0, [false, false]);
    assert_eq!(record["level"], json!(LevelKey::main(LevelId::Meadow)));
    for field in [
        "tick",
        "logical_tick",
        "logical_elapsed_nanos",
        "travel_count",
        "saves",
        "restores",
    ] {
        assert_eq!(
            record["state"][field], 0,
            "Title must not run an extra fixed tick: {record}"
        );
    }
    assert_eq!(record["state"]["held"], json!([false, false, false, false]));
    assert_eq!(record["launch_admitted"], true);
    assert_eq!(record["resume_applied"], true);
    if pending_save {
        assert_eq!(record["launch_accepted"], false);
    }
}

pub fn frozen(before: &Value, after: &Value) {
    for field in [
        "tick",
        "logical_tick",
        "logical_elapsed_nanos",
        "player",
        "score",
        "collected",
        "travel_count",
        "saves",
        "restores",
        "current_instance",
        "held",
    ] {
        assert_eq!(
            after["state"][field], before["state"][field],
            "paused original {field} advanced"
        );
    }
    assert_eq!(after["state"]["held"], json!([false, false, false, false]));
    assert_eq!(before["current_receipt"], after["current_receipt"]);
    for field in ["app_frame", "render_frame"] {
        assert!(
            number(&after["presentation"], field) > number(&before["presentation"], field),
            "pause proof requires a newer actual presentation"
        );
    }
}

pub fn terminal(record: &Value, request: &Request, cues: bool) {
    for field in [
        "ready",
        "launch_admitted",
        "resume_applied",
        "cleanup_complete",
        "original_runtime_retired",
        "original_pump_returned",
    ] {
        assert_eq!(
            record[field], true,
            "missing original terminal {field}: {record}"
        );
    }
    if request.runtime != "audio-desktop" {
        return;
    }
    let audio = &record["audio"];
    assert_eq!(audio["format"], "two-level-cooked-audio/v1");
    assert_eq!(audio["source_free"], true);
    assert_eq!(audio["input_proof"], record["input_proof"]);
    assert_eq!(
        audio["bank_product"],
        two_level_game::sound_bank::bank_product().to_string()
    );
    assert_eq!(audio["verification_requested"], false);
    assert_eq!(audio["trajectory"], "interactive");
    assert!(audio["pcm"].is_null());
    for field in [
        "native_destroyed",
        "surface_retired",
        "provider_released",
        "target_fault_absent",
    ] {
        assert_eq!(audio["native"][field], true);
    }
    let snapshot = &audio["snapshot"];
    assert_eq!(
        snapshot["runtime"],
        record["presentation"]["runtime_generation"]
    );
    assert_eq!(snapshot["faults"], 0);
    assert!(snapshot["error"].is_null());
    assert_eq!(snapshot["admission_open"], false);
    for field in [
        "play_admitted",
        "stream_drop_returned",
        "data_callback_released",
        "error_callback_released",
        "physical_complete",
    ] {
        assert_eq!(
            snapshot[field], true,
            "original native audio did not retire: {record}"
        );
    }
    assert!(number(snapshot, "callbacks") > 0 && number(snapshot, "output_frames") > 0);
    assert_eq!(audio["callback_frames_drained"], snapshot["output_frames"]);
    assert_eq!(
        number(snapshot, "completed") + number(snapshot, "cancelled"),
        number(snapshot, "accepted")
    );
    if cues {
        assert!(number(snapshot, "accepted") >= 3);
    }
}

pub fn refused(record: &Value) {
    for field in [
        "success",
        "ready",
        "launch_admitted",
        "launch_accepted",
        "resume_applied",
        "processed_runtime",
        "first_present_verified",
        "original_runtime_retired",
        "original_pump_returned",
    ] {
        assert_eq!(
            record[field], false,
            "refusal crossed native admission: {record}"
        );
    }
    assert_eq!(record["presented_frames"], 0);
    assert!(record["presentation"].is_null() && record["adapter"].is_null());
    assert!(record["error"].is_string() || record["message"].is_string());
}
