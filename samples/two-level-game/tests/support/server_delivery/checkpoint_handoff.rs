use std::{
    fs,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::{DirectoryCapability, ExpectedTarget, FileIdentity, FsError, RelativeComponent},
    project_tooling::host_fs,
};
use serde_json::{Value, json};
use two_level_game::{
    command::ServerCommand,
    levels::{LevelId, LevelKey},
    resume::processed::{
        MAX_PROCESSED_RESUME_BYTES, PendingServerCommand, ProcessedResumeState,
        decode_processed_resume, encode_processed_resume,
    },
    save::{MAX_CHECKPOINT_BYTES, decode_checkpoint, io::SaveIoError},
};

use super::{
    Custody, read_bounded,
    service::{
        ANCHOR_POSITION, Launch, RELEASED, Service, TIMESTEP_NANOS, action, assert_progress,
        movement, player, request, tick,
    },
};

const MEADOW: LevelKey = LevelKey::main(LevelId::Meadow);
const MAX_WALK_TICKS: usize = 128;

pub(super) fn accept(
    custody: &mut Custody,
    executable: &Path,
    candidate_custody: &mut Custody,
    candidate_executable: &Path,
) {
    let storage = custody.fixture().directory("server-checkpoint-handoff");
    let directory = host_fs::writable_directory(&storage).unwrap();
    let contention = FsError::LockContended.to_string();
    let claim = request("claim_checkpoint");

    for (case, replace) in [("handoff", false), ("replacement", true)] {
        let name = RelativeComponent::new(format!("{case}.json")).unwrap();
        let path = storage.join(name.to_str().unwrap());
        let resume_path = storage.join(format!("{case}-resume.json"));
        let cold = || Launch::PausedCheckpoint {
            save: &path,
            resume: None,
        };
        let mut incumbent = Service::spawn(
            custody,
            executable,
            cold(),
            &format!("server-checkpoint-{case}-incumbent"),
        );
        let ready = incumbent.ready(true, true);
        assert_progress(&ready["state"], MEADOW, 0, &[]);
        assert_eq!(player(&ready["state"]), ANCHOR_POSITION);
        assert!(!path.exists());
        let at_pickup = save_first_checkpoint(&mut incumbent);
        let original = read_checkpoint(&directory, &name);
        assert_checkpoint(&original.1, &[]);

        incumbent.admit(&action("marker"));
        let marked = step(&mut incumbent);
        let original_tick = tick(&marked["state"]);
        assert_progress(&marked["state"], MEADOW, 10, &[1]);
        assert_eq!(player(&marked["state"]), at_pickup);
        assert_eq!(incumbent.admit(&request("save")), original_tick + 1);
        let before_capture = incumbent.observe();
        assert_eq!(before_capture["state"], marked["state"], "{before_capture}");
        assert_eq!(before_capture["state"]["saves"], 1, "{before_capture}");
        assert_eq!(
            before_capture["checkpoint_pending"], false,
            "{before_capture}"
        );
        assert_eq!(
            before_capture["commands"]["retained"], 1,
            "{before_capture}"
        );
        assert_eq!(
            before_capture["time"]["fixed_debt_nanos"], 0,
            "{before_capture}"
        );
        let (captured, bytes) = capture(&mut incumbent);
        let boundary = incumbent.observe_after_capture_maintenance(before_capture);
        assert_eq!(
            captured.pending,
            [PendingServerCommand {
                tick_offset: 1,
                command: ServerCommand::Save,
            }]
        );
        assert_eq!(captured.game.counters.restores, 0);
        fs::write(&resume_path, &bytes).unwrap();

        if !replace {
            let refused = Service::spawn(
                candidate_custody,
                candidate_executable,
                Launch::Paused(Some(&resume_path)),
                "server-checkpoint-resume-without-save",
            )
            .startup_refusal();
            assert_refusal(
                &refused,
                "the checkpoint option must match the captured Server state",
            );
        }

        let mut candidate = Service::spawn(
            candidate_custody,
            candidate_executable,
            Launch::PausedCheckpoint {
                save: &path,
                resume: Some(&resume_path),
            },
            &format!("server-checkpoint-{case}-candidate"),
        );
        let ready = candidate.ready(true, true);
        let restored_tick = tick(&ready["state"]);
        assert_eq!(restored_tick, 1, "{ready}");
        assert!(restored_tick < original_tick, "{boundary}\n{ready}");
        candidate.expect_restored_pending(&[1]);
        let staged = candidate.observe();
        for field in ["paused", "checkpoint_pending", "state", "commands", "time"] {
            assert_eq!(staged[field], ready[field], "{ready}\n{staged}");
        }
        let mut expected = boundary["state"].clone();
        expected["tick"] = json!(restored_tick);
        assert_eq!(staged["state"], expected, "{staged}");
        for field in ["fixed_debt_nanos", "fixed_remainder_nanos"] {
            assert_eq!(staged["time"][field], boundary["time"][field], "{staged}");
        }
        assert_refusal(&candidate.exchange(&claim, "rejected"), &contention);
        for denied in [
            action("marker"),
            request("side_room"),
            request("lookout"),
            request("save"),
            request("restore"),
            request("resume"),
            advance_request(),
        ] {
            assert_refusal(
                &candidate.exchange(&denied, "rejected"),
                "checkpoint is pending",
            );
        }
        assert_eq!(candidate.observe(), staged);
        assert_eq!(
            capture(&mut candidate).1,
            bytes,
            "checkpoint resume bytes changed"
        );
        assert_eq!(candidate.observe(), staged);
        assert_eq!(incumbent.observe(), boundary);
        assert_eq!(read_checkpoint(&directory, &name), original);

        let replacement = if replace {
            let parked = RelativeComponent::new("replacement-original.json").unwrap();
            let parked_path = storage.join(parked.to_str().unwrap());
            fs::rename(&path, &parked_path).unwrap();
            fs::copy(&parked_path, &path).unwrap();
            let replacement = read_checkpoint(&directory, &name);
            assert_ne!(replacement.0, original.0);
            assert_eq!(replacement.1, original.1);
            Some((parked, replacement))
        } else {
            None
        };
        retire(incumbent, &boundary);

        let (final_checkpoint, outcome) = if let Some((parked, replacement)) = replacement {
            assert_refusal(
                &candidate.exchange(&claim, "rejected"),
                &SaveIoError::Conflict.to_string(),
            );
            assert_eq!(candidate.observe(), staged);
            retire(candidate, &staged);
            assert_eq!(read_checkpoint(&directory, &parked), original);
            (replacement, "replacement_refused")
        } else {
            candidate.exchange(&claim, "checkpoint_claimed");
            let mut claimed = staged;
            claimed["checkpoint_pending"] = json!(false);
            assert_eq!(candidate.observe(), claimed);
            assert_eq!(read_checkpoint(&directory, &name), original);

            let refused = Service::spawn(custody, executable, cold(), "server-checkpoint-busy")
                .startup_refusal();
            assert_refusal(&refused, &contention);
            let completed_resume = storage.join("completed-restore-state.json");
            let published = continue_with_checkpoint(
                candidate,
                &claimed["state"],
                &directory,
                &name,
                &original,
                &completed_resume,
            );
            let mut fresh = Service::spawn(
                candidate_custody,
                candidate_executable,
                Launch::PausedCheckpoint {
                    save: &path,
                    resume: Some(&completed_resume),
                },
                "server-checkpoint-completed-restore",
            );
            let ready = fresh.ready(true, true);
            assert_eq!(ready["state"]["restores"], 1, "{ready}");
            assert_eq!(
                capture(&mut fresh).1,
                read_bounded(&completed_resume, MAX_PROCESSED_RESUME_BYTES)
            );
            fresh.exchange(&claim, "checkpoint_claimed");
            fresh.send(&request("shutdown"));
            let closed = fresh.finish("shutdown", None);
            assert_eq!(closed["state"]["restores"], 1, "{closed}");
            (published, "claimed")
        };
        assert_eq!(read_checkpoint(&directory, &name), final_checkpoint);

        let mut reopened = Service::spawn(
            custody,
            executable,
            cold(),
            &format!("server-checkpoint-{case}-reopened"),
        );
        reopened.ready(true, true);
        reopened.send(&request("shutdown"));
        reopened.finish("shutdown", None);
        assert_eq!(read_checkpoint(&directory, &name), final_checkpoint);
        assert_eq!(
            read_bounded(&resume_path, MAX_PROCESSED_RESUME_BYTES),
            bytes
        );

        println!(
            "TWO_LEVEL_SERVER_CHECKPOINT_HANDOFF case={case} outcome={outcome} \
             original_tick={original_tick} restored_tick={restored_tick} pending_save_offset=1 \
             resume_bytes={} original_checkpoint_bytes={} lock_reopened=true",
            bytes.len(),
            original.1.len(),
        );
    }
}

fn save_first_checkpoint(server: &mut Service<'_>) -> [f32; 2] {
    server.admit(&movement(true));
    let picked_up = (0..MAX_WALK_TICKS)
        .map(|_| step(server))
        .find(|observed| observed["state"]["score"] != 0)
        .expect("the explorer collects Meadow's near gem");
    let moved = &picked_up["state"];
    assert_progress(moved, MEADOW, 10, &[]);
    assert_eq!(moved["held"], json!([false, true, false, false]), "{moved}");
    assert_eq!(moved["visible_pickups"], 1, "{moved}");
    let position = player(moved);
    assert!(position[0] > ANCHOR_POSITION[0], "{moved}");

    server.admit(&movement(false));
    server.admit(&request("save"));
    let saved = step(server);
    let state = &saved["state"];
    assert_progress(state, MEADOW, 10, &[]);
    assert_eq!(state["held"], json!(RELEASED), "{saved}");
    assert_eq!(state["saves"], 1, "{saved}");
    assert_eq!(player(state), position, "{saved}");
    position
}

fn continue_with_checkpoint(
    mut candidate: Service<'_>,
    start: &Value,
    directory: &DirectoryCapability,
    name: &RelativeComponent,
    original: &(FileIdentity, Vec<u8>),
    completed_resume: &Path,
) -> (FileIdentity, Vec<u8>) {
    let handed_off = step(&mut candidate);
    let state = &handed_off["state"];
    assert_eq!(tick(state), tick(start) + 1, "{handed_off}");
    assert_progress(state, MEADOW, 10, &[1]);
    assert_eq!(state["saves"], 2, "{handed_off}");
    assert_eq!(handed_off["commands"]["retained"], 0, "{handed_off}");
    let position = player(state);
    assert_eq!(position, player(start));
    let published = read_checkpoint(directory, name);
    assert_ne!(published.0, original.0);
    assert_ne!(published.1, original.1);
    assert_checkpoint(&published.1, &[position]);

    let idle = step(&mut candidate);
    assert_eq!(tick(&idle["state"]), tick(state) + 1, "{idle}");
    assert_eq!(idle["state"]["saves"], 2, "{idle}");
    assert_eq!(read_checkpoint(directory, name), published);

    candidate.admit(&action("marker"));
    step(&mut candidate);
    candidate.admit(&request("save"));
    let saved = step(&mut candidate);
    assert_progress(&saved["state"], MEADOW, 10, &[1, 2]);
    assert_eq!(saved["state"]["saves"], 3, "{saved}");
    let saved_file = read_checkpoint(directory, name);
    assert_ne!(saved_file.0, published.0);
    assert_checkpoint(&saved_file.1, &[position; 2]);

    candidate.admit(&action("marker"));
    let unsaved = step(&mut candidate);
    assert_progress(&unsaved["state"], MEADOW, 10, &[1, 2, 3]);

    let temporary = RelativeComponent::new("restore-replacement.tmp").unwrap();
    let mut replacement = directory.create_temp(&temporary).unwrap();
    replacement.write_all(&saved_file.1).unwrap();
    let restore_tick = candidate.admit(&request("restore"));
    let preparing = step(&mut candidate);
    assert_eq!(preparing["state"]["phase"], "Preparing", "{preparing}");
    assert_eq!(tick(&preparing["state"]), restore_tick, "{preparing}");
    assert_eq!(preparing["state"]["restores"], 0, "{preparing}");
    directory
        .replace_temp(replacement, name, ExpectedTarget::Identity(saved_file.0))
        .unwrap();
    let replacement = read_checkpoint(directory, name);
    assert_ne!(replacement.0, saved_file.0);
    assert_eq!(replacement.1, saved_file.1);

    let degraded = wait_for_restore_settlement(&mut candidate);
    let conflict = SaveIoError::Conflict.to_string();
    assert_eq!(degraded["state"]["phase"], "Degraded", "{degraded}");
    assert_eq!(
        degraded["state"]["error"],
        format!("restored checkpoint baseline changed: {conflict}"),
        "{degraded}"
    );
    assert_eq!(degraded["state"]["restores"], 0, "{degraded}");

    candidate.admit(&action("retry"));
    let retried = step(&mut candidate);
    assert_progress(&retried["state"], MEADOW, 10, &[1, 2]);
    assert_eq!(retried["state"]["restores"], 0, "{retried}");

    let save_tick = candidate.admit(&request("save"));
    let refused = candidate.exchange(&advance_request(), "advanced");
    assert_eq!(refused["paused"], true, "{refused}");
    assert_eq!(tick(&refused["state"]), save_tick, "{refused}");
    assert_eq!(refused["commands"]["retained"], 0, "{refused}");
    let rejections = &refused["gameplay_rejections"];
    assert_eq!(rejections["count"], 1, "{refused}");
    assert_eq!(rejections["latest"]["message"], conflict, "{refused}");
    assert_eq!(refused["state"]["saves"], 3, "{refused}");
    assert_eq!(refused["state"]["restores"], 0, "{refused}");
    assert_eq!(read_checkpoint(directory, name), replacement);

    candidate.admit(&request("restore"));
    step(&mut candidate);
    let restored = candidate.until("the claimed checkpoint to restore", |state| {
        state["phase"] == "Running" && state["restoring"] == false
    });
    assert_progress(&restored, MEADOW, 10, &[1, 2]);
    assert_eq!(restored["restores"], 1, "{restored}");
    assert_eq!(restored["saves"], 3, "{restored}");
    assert_eq!(player(&restored), ANCHOR_POSITION);

    candidate.admit(&request("save"));
    let resaved = step(&mut candidate);
    assert_eq!(resaved["state"]["saves"], 4, "{resaved}");
    assert_eq!(resaved["state"]["restores"], 1, "{resaved}");
    let resaved_file = read_checkpoint(directory, name);
    assert_ne!(resaved_file.0, replacement.0);
    assert_checkpoint(&resaved_file.1, &[position; 2]);

    let (captured, bytes) = capture(&mut candidate);
    assert!(captured.pending.is_empty());
    assert_eq!(captured.game.counters.restores, 1);
    fs::write(completed_resume, bytes).unwrap();
    candidate.send(&request("shutdown"));
    let closed = candidate.finish("shutdown", None);
    assert_eq!(closed["state"], resaved["state"], "{closed}");
    assert_eq!(closed["server_evidence"]["gameplay_rejections"]["count"], 1);
    resaved_file
}

fn wait_for_restore_settlement(server: &mut Service<'_>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    let observe = request("observe");
    loop {
        let observed = server.exchange(&observe, "observed");
        assert_eq!(observed["paused"], true, "{observed}");
        if !matches!(
            observed["state"]["phase"].as_str(),
            Some("Preparing" | "Reconstructing")
        ) {
            return observed;
        }
        assert!(
            Instant::now() < deadline,
            "checkpoint Restore remained pending: {observed}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn retire(mut server: Service<'_>, boundary: &Value) {
    server.send(&request("retire"));
    let retired = server.finish("retired", None);
    assert_eq!(retired["state"], boundary["state"], "{retired}");
    for field in ["paused", "commands", "time"] {
        assert_eq!(
            retired["server_evidence"][field], boundary[field],
            "{retired}"
        );
    }
}

fn assert_refusal(reply: &Value, message: &str) {
    assert!(
        reply["message"]
            .as_str()
            .is_some_and(|text| text.contains(message)),
        "expected {message:?}: {reply}"
    );
}

fn advance_request() -> Value {
    json!({"request": "advance", "elapsed_nanos": TIMESTEP_NANOS})
}

fn step(server: &mut Service<'_>) -> Value {
    let advanced = server.exchange_observation(&advance_request(), "advanced");
    assert_eq!(advanced["paused"], true, "{advanced}");
    advanced
}

fn capture(server: &mut Service<'_>) -> (ProcessedResumeState, Vec<u8>) {
    let reply = server.exchange(&request("capture"), "captured");
    let wire = &reply["state"];
    assert_eq!(wire["format_version"], 4, "{reply}");
    assert!(wire["payload"]["checkpoint"].is_array(), "{reply}");
    let state = decode_processed_resume(&serde_json::to_vec(wire).unwrap()).unwrap();
    let bytes = encode_processed_resume(&state).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *wire);
    (state, bytes)
}

fn read_checkpoint(
    directory: &DirectoryCapability,
    name: &RelativeComponent,
) -> (FileIdentity, Vec<u8>) {
    let file = directory.open_child_file(name).unwrap();
    let bytes = file
        .read_to_end_bounded(MAX_CHECKPOINT_BYTES as u64)
        .unwrap();
    (file.identity(), bytes)
}

fn assert_checkpoint(bytes: &[u8], marker_positions: &[[f32; 2]]) {
    assert!(!bytes.is_empty());
    let decoded = decode_checkpoint(bytes).unwrap();
    assert_eq!(decoded.migrated_from, None);
    let checkpoint = decoded.checkpoint;
    assert_eq!(checkpoint.location.level, MEADOW);
    assert_eq!(checkpoint.location.anchor.as_str(), "checkpoint");
    assert_eq!(checkpoint.score, 10);
    assert_eq!(checkpoint.collected.len(), 1);
    let pickup = &checkpoint.collected[0];
    assert_eq!((pickup.level, pickup.entity.as_str()), (MEADOW, "gem-near"));
    assert_eq!(checkpoint.markers.len(), marker_positions.len());
    for (index, (marker, position)) in checkpoint.markers.iter().zip(marker_positions).enumerate() {
        assert_eq!(marker.id.get(), index as u64 + 1);
        assert_eq!(marker.level, MEADOW);
        assert_eq!(marker.position, *position);
    }
    assert_eq!(
        checkpoint.next_marker_id.get(),
        marker_positions.len() as u64 + 1
    );
}
