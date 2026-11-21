use std::{fs, path::Path};

use nara::app::FixedTime;
use serde_json::{Value, json};
use two_level_game::{
    command::{SemanticCommand, ServerCommand},
    levels::{LevelId, LevelKey},
    resume::{
        RuntimeCatchUpPolicy, SessionCounters,
        processed::{
            MAX_PROCESSED_RESUME_BYTES, PendingServerCommand, ProcessedResumeState,
            ProcessedRoomState, ProcessedRooms, decode_processed_resume, encode_processed_resume,
        },
    },
};

use super::{
    Custody, read_bounded,
    service::{
        ANCHOR_POSITION, Launch, RELEASED, Service, TIMESTEP_NANOS, action, assert_progress,
        movement, number, player, request, tick,
    },
};

const MILLISECOND: u64 = 1_000_000;
const INITIAL_REMAINDER: u64 = 7 * MILLISECOND;
const RELEASE_OFFSET: u64 = 2;
const MARKER_OFFSET: u64 = 4;
const HANDOFF_OFFSET: u64 = 8;
const HELD_RIGHT: [bool; 4] = [false, true, false, false];
const MAX_WALK_TICKS: usize = 128;
const CATALOG_REFUSAL: &str = "resume Runtime catalog differs from the admitted Server catalog";
const UP: usize = 2;
const DOWN: usize = 3;
const HELD_UP: [bool; 4] = [false, false, true, false];

pub(super) fn accept(
    custody: &mut Custody,
    executable: &Path,
    candidate_custody: &mut Custody,
    candidate_executable: &Path,
) {
    let levels = [
        LevelKey::main(LevelId::Meadow),
        LevelKey::main(LevelId::Cavern),
        LevelKey::new(LevelId::Cavern, 1).unwrap(),
    ];
    let instance = levels[2];
    let mut incumbent = Service::spawn(custody, executable, Launch::Paused(None), "server-paused");
    let ready = incumbent.ready(true, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");
    assert_progress(&ready["state"], levels[0], 0, &[]);
    assert_eq!(player(&ready["state"]), ANCHOR_POSITION);
    let placements = play_prefix(&mut incumbent, levels);

    incumbent.admit(&action("retry"));
    let retried = advance(&mut incumbent, TIMESTEP_NANOS);
    assert_progress(&retried["state"], instance, 30, &[3]);
    assert_eq!(retried["state"]["restarts"], 1, "{retried}");
    assert_eq!(player(&retried["state"]), ANCHOR_POSITION);

    incumbent.admit(&movement(true));
    let cap = u64::from(FixedTime::DEFAULT_MAX_STEPS_PER_FRAME);
    let burst = advance(
        &mut incumbent,
        (cap + 1) * TIMESTEP_NANOS + INITIAL_REMAINDER,
    );
    let original_tick = tick(&burst["state"]);
    assert_eq!(original_tick, tick(&retried["state"]) + cap, "{burst}");
    assert_eq!(burst["time"]["fixed_debt_nanos"], TIMESTEP_NANOS, "{burst}");
    assert_eq!(
        burst["time"]["fixed_remainder_nanos"], INITIAL_REMAINDER,
        "{burst}"
    );
    assert_eq!(burst["state"]["held"], json!(HELD_RIGHT), "{burst}");
    assert!(player(&burst["state"])[0] > ANCHOR_POSITION[0], "{burst}");

    assert_eq!(
        incumbent.admit(&after_ticks(movement(false), RELEASE_OFFSET)),
        original_tick + RELEASE_OFFSET
    );
    assert_eq!(
        incumbent.admit(&after_ticks(action("marker"), MARKER_OFFSET)),
        original_tick + MARKER_OFFSET
    );
    assert_eq!(
        incumbent.admit(&after_ticks(action("marker"), HANDOFF_OFFSET)),
        original_tick + HANDOFF_OFFSET
    );
    let boundary = incumbent.observe();
    assert_eq!(boundary["state"], burst["state"], "{boundary}");
    assert_eq!(boundary["time"], burst["time"], "{boundary}");
    let (captured, bytes) = capture(&mut incumbent);
    assert_capture(&captured, &boundary["state"], levels, placements);
    let manifest = executable.with_file_name("package.json");
    let package: Value = serde_json::from_slice(&read_bounded(&manifest, 1024 * 1024)).unwrap();
    assert_eq!(
        package["payload"]["catalog_digest"],
        json!(captured.catalog)
    );

    let storage = candidate_custody.fixture().directory("server-resume-state");
    let incompatible_path = storage.join("incompatible.json");
    let mut incompatible = captured.clone();
    incompatible.catalog.hash[0] ^= 1;
    let incompatible_bytes = encode_processed_resume(&incompatible).unwrap();
    fs::write(&incompatible_path, &incompatible_bytes).unwrap();
    let refusal = Service::spawn(
        candidate_custody,
        candidate_executable,
        Launch::Paused(Some(&incompatible_path)),
        "server-incompatible-resume",
    )
    .startup_refusal();
    assert!(
        refusal["message"]
            .as_str()
            .is_some_and(|message| message.contains(CATALOG_REFUSAL)),
        "{refusal}"
    );
    assert_eq!(incumbent.observe(), boundary);

    let resume_path = storage.join("resume.json");
    fs::write(&resume_path, &bytes).unwrap();
    let mut candidate = Service::spawn(
        candidate_custody,
        candidate_executable,
        Launch::Paused(Some(&resume_path)),
        "server-restored-resume",
    );
    let ready = candidate.ready(true, false);
    let restored_tick = tick(&ready["state"]);
    assert_eq!(restored_tick, 1, "{ready}");
    assert!(original_tick > restored_tick, "{boundary}\n{ready}");
    candidate.expect_restored_pending(&[RELEASE_OFFSET, MARKER_OFFSET, HANDOFF_OFFSET]);
    let restored = candidate.observe();
    for field in ["state", "commands", "time"] {
        assert_eq!(restored[field], ready[field], "{ready}\n{restored}");
    }
    assert_same_game(&boundary["state"], &restored["state"]);
    assert_eq!(restored["time"]["elapsed_nanos"], 0, "{restored}");
    assert_eq!(
        restored["time"]["fixed_debt_nanos"], TIMESTEP_NANOS,
        "{restored}"
    );
    assert_eq!(
        restored["time"]["fixed_remainder_nanos"], INITIAL_REMAINDER,
        "{restored}"
    );
    assert_eq!(
        capture(&mut candidate).1,
        bytes,
        "canonical resume bytes changed"
    );
    assert_eq!(incumbent.observe(), boundary);

    drive_suffix(&mut incumbent, &mut candidate, &boundary, &restored);
    let before_retire = incumbent.observe();
    assert_progress(&before_retire["state"], instance, 30, &[3, 4]);
    assert_eq!(before_retire["commands"]["retained"], 1, "{before_retire}");
    incumbent.send(&request("retire"));
    let retired = incumbent.finish("retired", None);
    assert_eq!(retired["state"], before_retire["state"], "{retired}");
    assert_eq!(retired["server_evidence"]["paused"], true, "{retired}");
    for field in ["commands", "time"] {
        assert_eq!(
            retired["server_evidence"][field], before_retire[field],
            "{retired}"
        );
    }

    let before_handoff = advance(&mut candidate, 2 * TIMESTEP_NANOS);
    assert_eq!(
        tick(&before_handoff["state"]),
        restored_tick + HANDOFF_OFFSET - 1
    );
    assert_progress(&before_handoff["state"], instance, 30, &[3, 4]);
    assert_eq!(before_handoff["commands"]["retained"], 1);
    let handed_off = advance(&mut candidate, TIMESTEP_NANOS);
    assert_eq!(tick(&handed_off["state"]), restored_tick + HANDOFF_OFFSET);
    assert_progress(&handed_off["state"], instance, 30, &[3, 4, 5]);
    assert_eq!(handed_off["commands"]["retained"], 0);

    let resumed = candidate.exchange_observation(&request("resume"), "resumed");
    assert_eq!(resumed["paused"], false, "{resumed}");
    candidate.admit(&action("marker"));
    candidate.until("continued input to place the next marker", |state| {
        state["markers"] == json!([3, 4, 5, 6])
    });
    let paused = candidate.exchange_observation(&request("pause"), "paused");
    assert_eq!(paused["paused"], true, "{paused}");
    candidate.send(&request("shutdown"));
    let shutdown = candidate.finish("shutdown", None);
    assert_eq!(shutdown["state"], paused["state"], "{shutdown}");
    assert_progress(&shutdown["state"], instance, 30, &[3, 4, 5, 6]);
    assert_eq!(
        read_bounded(&resume_path, MAX_PROCESSED_RESUME_BYTES),
        bytes
    );
    assert_eq!(
        read_bounded(&incompatible_path, MAX_PROCESSED_RESUME_BYTES),
        incompatible_bytes
    );

    println!(
        "TWO_LEVEL_SERVER_CONTINUATION original_tick={original_tick} restored_tick={restored_tick} \
         canonical_bytes={} debt_nanos={TIMESTEP_NANOS} remainder_nanos={INITIAL_REMAINDER} \
         release_offset={RELEASE_OFFSET} marker_offset={MARKER_OFFSET} \
         handoff_offset={HANDOFF_OFFSET} \
         incompatible=refused_before_ready incumbent=retired candidate=shutdown",
        bytes.len(),
    );
}

pub(super) fn accept_open_rooms(
    custody: &mut Custody,
    executable: &Path,
    candidate_custody: &mut Custody,
    candidate_executable: &Path,
) {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let mut incumbent = Service::spawn(custody, executable, Launch::Paused(None), "server-rooms");
    let ready = incumbent.ready(true, false);
    assert_eq!(tick(&ready["state"]), 0, "{ready}");

    incumbent.admit(&request("lookout"));
    advance(&mut incumbent, TIMESTEP_NANOS);
    let lookout = incumbent.until("Lookout to open", |state| state["lookout"] == "open");
    assert_eq!(lookout["side_room"], "closed", "{lookout}");
    incumbent.admit(&request("side_room"));
    advance(&mut incumbent, TIMESTEP_NANOS);
    let opened = incumbent.until("Side Room to open", |state| state["side_room"] == "open");
    assert_eq!(opened["lookout"], "open", "{opened}");

    let found = walk_to_relic(&mut incumbent, "side_room", UP);
    assert_progress(&found, meadow, 0, &[]);
    assert_eq!(found["lookout"], "open", "{found}");
    assert!(player(&found)[1] > ANCHOR_POSITION[1], "{found}");
    let original_tick = tick(&found);
    advance(&mut incumbent, INITIAL_REMAINDER);
    assert_eq!(
        incumbent.admit(&after_ticks(vertical_movement(UP, false), RELEASE_OFFSET)),
        original_tick + RELEASE_OFFSET
    );
    let boundary = incumbent.observe();
    assert_eq!(boundary["paused"], true, "{boundary}");
    assert_eq!(boundary["state"], found, "{boundary}");
    assert_eq!(boundary["state"]["held"], json!(HELD_UP), "{boundary}");
    assert_eq!(boundary["time"]["fixed_debt_nanos"], 0, "{boundary}");
    assert_eq!(
        boundary["time"]["fixed_remainder_nanos"], INITIAL_REMAINDER,
        "{boundary}"
    );
    let (captured, bytes) = capture(&mut incumbent);
    assert_eq!(
        captured.rooms,
        ProcessedRooms {
            side_room: ProcessedRoomState::RelicFound,
            lookout: ProcessedRoomState::Open,
        }
    );
    assert_eq!(captured.game.clock.ticks, number(&found, "logical_tick"));
    assert_eq!(captured.game.runtime_time.pending_nanos, INITIAL_REMAINDER);
    assert_eq!(
        captured.pending,
        [PendingServerCommand {
            tick_offset: RELEASE_OFFSET,
            command: ServerCommand::Gameplay(SemanticCommand::Move {
                direction: UP,
                pressed: false,
            }),
        }]
    );

    let storage = candidate_custody
        .fixture()
        .directory("server-room-resume-state");
    let incompatible_path = storage.join("cavern-with-rooms.json");
    let mut incompatible = captured;
    incompatible.game.progress.location.level = cavern;
    incompatible.game.player.translation = ANCHOR_POSITION;
    fs::write(
        &incompatible_path,
        encode_processed_resume(&incompatible).unwrap(),
    )
    .unwrap();
    let refusal = Service::spawn(
        candidate_custody,
        candidate_executable,
        Launch::Paused(Some(&incompatible_path)),
        "server-rooms-incompatible-resume",
    )
    .startup_refusal();
    assert!(
        refusal["message"]
            .as_str()
            .is_some_and(|message| message.contains("the current level has no side room portal")),
        "{refusal}"
    );
    assert_eq!(incumbent.observe(), boundary);
    assert_eq!(capture(&mut incumbent).1, bytes);

    let resume_path = storage.join("meadow-with-rooms.json");
    fs::write(&resume_path, &bytes).unwrap();
    let mut candidate = Service::spawn(
        candidate_custody,
        candidate_executable,
        Launch::Paused(Some(&resume_path)),
        "server-rooms-restored-resume",
    );
    let ready = candidate.ready(true, false);
    let restored_tick = tick(&ready["state"]);
    assert_eq!(restored_tick, 1, "{ready}");
    assert!(original_tick > restored_tick, "{boundary}\n{ready}");
    candidate.expect_restored_pending(&[RELEASE_OFFSET]);
    let restored = candidate.observe();
    for field in ["paused", "state", "commands", "time"] {
        assert_eq!(restored[field], ready[field], "{ready}\n{restored}");
    }
    assert_same_game(&boundary["state"], &restored["state"]);
    for field in ["fixed_debt_nanos", "fixed_remainder_nanos"] {
        assert_eq!(
            restored["time"][field], boundary["time"][field],
            "{restored}"
        );
    }
    assert_eq!(restored["time"]["elapsed_nanos"], 0, "{restored}");
    assert_eq!(
        capture(&mut candidate).1,
        bytes,
        "room resume bytes changed"
    );

    incumbent.send(&request("retire"));
    let retired = incumbent.finish("retired", None);
    assert_eq!(retired["state"], boundary["state"], "{retired}");
    for field in ["paused", "commands", "time"] {
        assert_eq!(
            retired["server_evidence"][field], boundary[field],
            "{retired}"
        );
    }

    let travelled = continue_open_rooms(&mut candidate, &restored["state"], meadow, cavern);
    candidate.send(&request("shutdown"));
    let shutdown = candidate.finish("shutdown", None);
    assert_eq!(shutdown["state"], travelled, "{shutdown}");
    assert_eq!(
        shutdown["server_evidence"]["gameplay_rejections"]["count"], 0,
        "{shutdown}"
    );

    println!(
        "TWO_LEVEL_SERVER_OPEN_ROOMS_CONTINUATION original_tick={original_tick} \
         restored_tick={restored_tick} canonical_bytes={} release_offset={RELEASE_OFFSET} \
         incompatible=refused_before_ready incumbent=retired candidate=shutdown",
        bytes.len(),
    );
}

fn continue_open_rooms(
    candidate: &mut Service<'_>,
    restored: &Value,
    meadow: LevelKey,
    cavern: LevelKey,
) -> Value {
    let moving = advance(candidate, TIMESTEP_NANOS - INITIAL_REMAINDER);
    assert_eq!(tick(&moving["state"]), tick(restored) + 1, "{moving}");
    assert_eq!(moving["commands"]["retained"], 1, "{moving}");
    assert_eq!(moving["time"]["fixed_remainder_nanos"], 0, "{moving}");
    assert_eq!(moving["state"]["held"], json!(HELD_UP), "{moving}");
    assert!(
        player(&moving["state"])[1] > player(restored)[1],
        "{moving}"
    );
    let released = advance(candidate, TIMESTEP_NANOS);
    assert_eq!(tick(&released["state"]), tick(restored) + RELEASE_OFFSET);
    assert_eq!(
        number(&released["state"], "logical_tick"),
        number(restored, "logical_tick") + RELEASE_OFFSET
    );
    assert_eq!(released["commands"]["retained"], 0, "{released}");
    assert_eq!(released["state"]["held"], json!(RELEASED), "{released}");
    assert_eq!(player(&released["state"]), player(&moving["state"]));
    assert_eq!(released["state"]["side_room"], "relic_found", "{released}");
    assert_eq!(released["state"]["lookout"], "open", "{released}");

    let found = walk_to_relic(candidate, "lookout", DOWN);
    assert_progress(&found, meadow, 0, &[]);
    assert_eq!(found["side_room"], "relic_found", "{found}");
    assert!(player(&found)[1] < ANCHOR_POSITION[1], "{found}");
    candidate.admit(&vertical_movement(DOWN, false));
    candidate.admit(&request("lookout"));
    let closed = advance(candidate, TIMESTEP_NANOS);
    assert_progress(&closed["state"], meadow, 0, &[]);
    assert_eq!(closed["state"]["held"], json!(RELEASED), "{closed}");
    assert_eq!(closed["state"]["travel_count"], 0, "{closed}");
    assert_eq!(closed["state"]["side_room"], "relic_found", "{closed}");
    assert_eq!(closed["state"]["lookout"], "closed", "{closed}");
    assert_eq!(
        capture(candidate).0.rooms,
        ProcessedRooms {
            side_room: ProcessedRoomState::RelicFound,
            lookout: ProcessedRoomState::Closed,
        }
    );

    candidate.admit(&action("travel"));
    advance(candidate, TIMESTEP_NANOS);
    let travelled = candidate.until("Cavern Travel to commit", |state| {
        state["level"] == json!(cavern) && state["phase"] == "Running"
    });
    assert_progress(&travelled, cavern, 0, &[]);
    assert_eq!(travelled["travel_count"], 1, "{travelled}");
    assert_eq!(travelled["held"], json!(RELEASED), "{travelled}");
    assert_eq!(travelled["side_room"], "closed", "{travelled}");
    assert_eq!(travelled["lookout"], "closed", "{travelled}");
    assert_eq!(player(&travelled), ANCHOR_POSITION);
    assert_eq!(
        capture(candidate).0.rooms,
        ProcessedRooms {
            side_room: ProcessedRoomState::Closed,
            lookout: ProcessedRoomState::Closed,
        }
    );
    travelled
}

fn vertical_movement(direction: usize, pressed: bool) -> Value {
    json!({"request": "command", "command": SemanticCommand::Move { direction, pressed }})
}

fn walk_to_relic(server: &mut Service<'_>, room: &str, direction: usize) -> Value {
    server.admit(&vertical_movement(direction, true));
    for _ in 0..MAX_WALK_TICKS {
        let observation = advance(server, TIMESTEP_NANOS);
        let state = &observation["state"];
        if state[room] == "relic_found" {
            assert_eq!(state["held"][direction], true, "{state}");
            return state.clone();
        }
    }
    panic!(
        "expected the {room} relic after {MAX_WALK_TICKS} ticks: {}",
        server.observe()
    );
}

fn play_prefix(server: &mut Service<'_>, levels: [LevelKey; 3]) -> [[f32; 2]; 3] {
    let mut placements = [[0.0; 2]; 3];
    for (index, (level, transition)) in levels
        .into_iter()
        .zip([None, Some("travel"), Some("instance")])
        .enumerate()
    {
        let marker = index as u64 + 1;
        let score = marker * 10;
        if let Some(operation) = transition {
            server.admit(&action(operation));
            advance(server, TIMESTEP_NANOS);
            let arrived = server.until("level travel to commit", |state| {
                state["level"] == json!(level) && state["phase"] == "Running"
            });
            assert_progress(&arrived, level, score - 10, &[]);
        }
        server.admit(&movement(true));
        let moved = walk_to_pickup(server, score);
        assert_progress(&moved, level, score, &[]);
        assert_eq!(moved["held"], json!(HELD_RIGHT), "{moved}");
        assert_eq!(moved["visible_pickups"], 1, "{moved}");
        server.admit(&movement(false));
        server.admit(&action("marker"));
        let marked = advance(server, TIMESTEP_NANOS);
        assert_progress(&marked["state"], level, score, &[marker]);
        assert_eq!(marked["state"]["held"], json!(RELEASED), "{marked}");
        placements[index] = player(&marked["state"]);
        assert_eq!(placements[index], player(&moved));
    }
    placements
}

fn assert_capture(
    captured: &ProcessedResumeState,
    observed: &Value,
    levels: [LevelKey; 3],
    placements: [[f32; 2]; 3],
) {
    let game = &captured.game;
    assert_eq!(game.progress.location.level, levels[2]);
    assert_eq!(game.progress.location.anchor.as_str(), "checkpoint");
    assert_eq!(game.progress.score, 30);
    assert_eq!(game.progress.collected.len(), levels.len());
    assert_eq!(game.progress.markers.len(), levels.len());
    for (index, level) in levels.into_iter().enumerate() {
        let pickup = &game.progress.collected[index];
        let marker = &game.progress.markers[index];
        assert_eq!((pickup.level, pickup.entity.as_str()), (level, "gem-near"));
        assert_eq!(marker.level, level);
        assert_eq!(marker.id.get(), index as u64 + 1);
        assert_eq!(marker.position, placements[index]);
    }
    assert_eq!(game.progress.next_marker_id.get(), 4);
    assert_eq!(game.player.translation, player(observed));
    assert_eq!(game.held, HELD_RIGHT);
    assert_eq!(game.clock.ticks, number(observed, "logical_tick"));
    assert_eq!(game.clock.timestep_nanos, TIMESTEP_NANOS);
    assert_eq!(
        game.runtime_time.catch_up_policy,
        RuntimeCatchUpPolicy::PreserveDebt
    );
    assert_eq!(
        game.runtime_time.max_steps_per_frame,
        FixedTime::DEFAULT_MAX_STEPS_PER_FRAME
    );
    assert_eq!(
        game.runtime_time.pending_nanos,
        TIMESTEP_NANOS + INITIAL_REMAINDER
    );
    assert_eq!(
        game.counters,
        SessionCounters {
            travel_count: 2,
            restarts: 1,
            saves: 0,
            restores: 0,
        }
    );
    assert_eq!(
        captured.pending,
        [
            PendingServerCommand {
                tick_offset: RELEASE_OFFSET,
                command: ServerCommand::Gameplay(SemanticCommand::Move {
                    direction: 1,
                    pressed: false,
                }),
            },
            PendingServerCommand {
                tick_offset: MARKER_OFFSET,
                command: ServerCommand::Gameplay(SemanticCommand::Marker),
            },
            PendingServerCommand {
                tick_offset: HANDOFF_OFFSET,
                command: ServerCommand::Gameplay(SemanticCommand::Marker),
            },
        ]
    );
}

fn drive_suffix(
    incumbent: &mut Service<'_>,
    candidate: &mut Service<'_>,
    before: &Value,
    restored: &Value,
) {
    let mut elapsed = 0;
    let mut held_position = player(&before["state"]);
    for (index, (delta, offset, remainder)) in [
        (MILLISECOND, 1, 8 * MILLISECOND),
        (11 * MILLISECOND, 1, 19 * MILLISECOND),
        (MILLISECOND, RELEASE_OFFSET, 0),
        (TIMESTEP_NANOS, 3, 0),
        (TIMESTEP_NANOS, MARKER_OFFSET, 0),
        (TIMESTEP_NANOS, 5, 0),
    ]
    .into_iter()
    .enumerate()
    {
        elapsed += delta;
        let original = advance(incumbent, delta);
        let replacement = advance(candidate, delta);
        assert_same_game(&original["state"], &replacement["state"]);
        assert_eq!(original["gameplay_rejections"]["count"], 0, "{original}");
        assert_eq!(
            replacement["gameplay_rejections"]["count"], 0,
            "{replacement}"
        );

        let held = offset < RELEASE_OFFSET;
        let marked = offset >= MARKER_OFFSET;
        let retained = u64::from(held) + u64::from(!marked) + 1;
        for (current, start) in [(&original, before), (&replacement, restored)] {
            let state = &current["state"];
            assert_eq!(tick(state), tick(&start["state"]) + offset, "{current}");
            assert_eq!(
                number(state, "logical_tick"),
                number(&start["state"], "logical_tick") + offset,
                "{current}"
            );
            assert_eq!(current["commands"]["retained"], retained, "{current}");
            assert_eq!(current["time"]["fixed_debt_nanos"], 0, "{current}");
            assert_eq!(
                current["time"]["fixed_remainder_nanos"], remainder,
                "{current}"
            );
            assert_eq!(
                number(&current["time"], "elapsed_nanos"),
                number(&start["time"], "elapsed_nanos") + elapsed,
                "{current}"
            );
        }
        let state = &original["state"];
        assert_eq!(
            state["held"],
            json!(if held { HELD_RIGHT } else { RELEASED }),
            "{state}"
        );
        assert_eq!(
            state["markers"],
            if marked { json!([3, 4]) } else { json!([3]) },
            "{state}"
        );
        let position = player(state);
        if index == 0 {
            assert!(position[0] > held_position[0], "{state}");
            held_position = position;
        } else {
            assert_eq!(position, held_position, "{state}");
        }
    }
}

fn assert_same_game(original: &Value, candidate: &Value) {
    let mut candidate = candidate.clone();
    candidate["tick"] = original["tick"].clone();
    assert_eq!(*original, candidate, "continued game observations differ");
}

fn walk_to_pickup(server: &mut Service<'_>, score: u64) -> Value {
    for _ in 0..MAX_WALK_TICKS {
        let observation = advance(server, TIMESTEP_NANOS);
        if number(&observation["state"], "score") >= score {
            return observation["state"].clone();
        }
    }
    let observation = server.observe();
    panic!("expected a score of {score} after {MAX_WALK_TICKS} ticks: {observation}");
}

fn advance(server: &mut Service<'_>, elapsed_nanos: u64) -> Value {
    let response = server.exchange_observation(
        &json!({"request": "advance", "elapsed_nanos": elapsed_nanos}),
        "advanced",
    );
    assert_eq!(response["paused"], true, "{response}");
    response
}

fn after_ticks(mut command: Value, offset: u64) -> Value {
    command["after_ticks"] = json!(offset);
    command
}

fn capture(server: &mut Service<'_>) -> (ProcessedResumeState, Vec<u8>) {
    let reply = server.exchange(&request("capture"), "captured");
    let envelope = &reply["state"];
    let bytes = serde_json::to_vec(envelope).unwrap();
    let state = decode_processed_resume(&bytes).expect("decode the Server's processed capture");
    let canonical = encode_processed_resume(&state).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&canonical).unwrap(),
        *envelope
    );
    (state, canonical)
}
