use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use two_level_game::{
    animation::atlas_product,
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    sound_bank::bank_product,
};

use super::{Request, package, walls};

pub fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing unsigned {field}: {value}"))
}

pub fn presented(record: &Value, request: &Request) {
    assert_eq!(record["success"], true, "{record}");
    assert_eq!(record["source_free"], true);
    assert_eq!(record["processed_runtime"], true);
    assert_eq!(
        record["content_profile"],
        "two-level-audio-desktop-shipping"
    );
    assert_eq!(record["content_target"], request.target);
    assert_eq!(record["catalog_hash"], request.catalog_digest["hash"]);
    assert_eq!(record["catalog_length"], request.catalog_digest["length"]);
    assert_eq!(record["first_present_verified"], true);
    assert_eq!(record["observations_truncated"], false);
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
        assert!(
            number(&record["presentation"], field) > 0,
            "unpresented {field}: {record}"
        );
    }
    assert_eq!(record["adapter"]["backend"], "dx12");
    assert!(matches!(
        record["adapter"]["device_type"].as_str(),
        Some("discrete-gpu" | "integrated-gpu")
    ));
    assert_eq!(record["state"]["initialized"], true);
    assert_eq!(record["state"]["phase"], "Running");
    assert_eq!(record["state"]["restoring"], false);
    assert!(record["state"]["error"].is_null(), "{record}");
    assert_eq!(record["state"]["scene_members"], AUTHORED_LEVEL_MEMBERS);
    let receipts = record["scene_receipts"].as_array().unwrap();
    assert!(!receipts.is_empty() && receipts.len() <= 16);
    for observed in receipts {
        walls::scene(observed);
    }
    let current: LevelKey = serde_json::from_value(record["level"].clone()).unwrap();
    let resident: BTreeSet<_> = [current.level().image_product(), atlas_product()]
        .into_iter()
        .collect();
    assert_eq!(
        record["resident_images"],
        json!(resident.iter().map(ToString::to_string).collect::<Vec<_>>())
    );
    let visited: BTreeSet<LevelId> =
        serde_json::from_value(record["visited_levels"].clone()).unwrap();
    assert_eq!(
        visited.len(),
        record["visited_levels"].as_array().unwrap().len()
    );
    assert!(visited.contains(&LevelId::STARTUP) && visited.contains(&current.level()));
    let products: BTreeSet<_> = visited
        .iter()
        .map(|level| level.image_product())
        .chain([atlas_product()])
        .collect();
    assert_eq!(
        record["observed_image_products"],
        json!(products.iter().map(ToString::to_string).collect::<Vec<_>>())
    );
    let proofs = record["image_proofs"].as_array().unwrap();
    assert_eq!(proofs.len(), products.len());
    assert_eq!(
        json!(
            proofs
                .iter()
                .map(|proof| &proof["product"])
                .collect::<Vec<_>>()
        ),
        record["observed_image_products"]
    );
    for product in products {
        assert_eq!(
            proofs
                .iter()
                .filter(|proof| **proof == request.images[&product.to_string()])
                .count(),
            1,
            "missing complete Runtime Image proof for {product}"
        );
    }
}

pub fn retired(record: &Value, request: &Request) {
    presented(record, request);
    for field in [
        "ready",
        "launch_admitted",
        "launch_accepted",
        "resume_applied",
        "cleanup_complete",
        "original_runtime_retired",
        "original_pump_returned",
    ] {
        assert_eq!(record[field], true, "missing original {field}: {record}");
    }
    let audio = &record["audio"];
    assert_eq!(audio["format"], "two-level-cooked-audio/v1");
    assert_eq!(audio["bank_product"], bank_product().to_string());
    assert_eq!(audio["source_free"], true);
    for field in [
        "native_destroyed",
        "surface_retired",
        "provider_released",
        "target_fault_absent",
    ] {
        assert_eq!(
            audio["native"][field], true,
            "missing physical {field}: {record}"
        );
    }
    let snapshot = &audio["snapshot"];
    assert_eq!(
        snapshot["runtime"],
        record["presentation"]["runtime_generation"]
    );
    assert!(number(snapshot, "callbacks") > 0 && number(snapshot, "output_frames") > 0);
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
            "original audio owner did not retire: {record}"
        );
    }
    assert_eq!(
        number(snapshot, "completed") + number(snapshot, "cancelled"),
        number(snapshot, "accepted")
    );
    assert_eq!(audio["callback_frames_drained"], snapshot["output_frames"]);
}

pub fn refused(record: &Value, reason: &str) {
    for field in [
        "success",
        "ready",
        "launch_admitted",
        "launch_accepted",
        "resume_applied",
        "processed_runtime",
        "first_present_verified",
    ] {
        assert_eq!(
            record[field], false,
            "publication preceded cooked-content refusal: {record}"
        );
    }
    assert_eq!(record["source_free"], true);
    assert_eq!(record["presented_frames"], 0);
    for field in [
        "state",
        "level",
        "startup_receipt",
        "current_receipt",
        "presentation",
        "adapter",
    ] {
        assert!(
            record[field].is_null(),
            "{field} was published before content admission: {record}"
        );
    }
    for field in [
        "scene_receipts",
        "visited_levels",
        "resident_images",
        "observed_image_products",
        "image_proofs",
    ] {
        assert!(record[field].as_array().unwrap().is_empty(), "{record}");
    }
    assert!(
        record["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    assert!(record["audio"]["snapshot"].is_null());
    assert!(record["audio"]["pcm"].is_null());
    assert_eq!(record["audio"]["callback_frames_drained"], 0);
    match reason {
        "compiled_catalog_expectation_mismatch" => {
            assert!(
                record["message"]
                    .as_str()
                    .unwrap()
                    .contains("Runtime catalog differs from the Host expectation"),
                "{record}"
            );
        }
        "corrupt_sound_bank_member" => {
            let diagnostics = record["runtime_diagnostics"].as_str().unwrap();
            assert!(
                diagnostics.contains("project.run.runtime-content-rejected")
                    && diagnostics.contains("member-digest-mismatch"),
                "{record}"
            );
        }
        _ => panic!("unexpected negative acceptance case: {reason}"),
    }
}

fn expected_causes() -> Vec<Value> {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let other = LevelKey::new(LevelId::Cavern, 1).unwrap();
    vec![
        json!({"kind": "pickup", "level": meadow, "entity": "gem-near"}),
        json!({"kind": "travel", "level": cavern}),
        json!({"kind": "pickup", "level": cavern, "entity": "gem-near"}),
        json!({"kind": "travel", "level": other}),
        json!({"kind": "pickup", "level": other, "entity": "gem-near"}),
        json!({"kind": "travel", "level": cavern}),
        json!({"kind": "travel", "level": meadow}),
    ]
}

pub fn accepted_causes(snapshot: &Value) -> Vec<Value> {
    snapshot["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["kind"] == "cue" && record["phase"]["kind"] == "accepted")
        .map(|record| record["cause"].clone())
        .collect()
}

struct Cue {
    record: Value,
    source: usize,
    start: Option<u64>,
    end: Option<(u64, bool)>,
}

pub fn verified(record: &Value, request: &Request) {
    retired(record, request);
    assert_eq!(record["scenario"], "travel");
    assert_eq!(record["animation_frames"], json!([0, 1, 2, 3]));
    assert_eq!(record["state"]["verified"], true);
    assert_eq!(record["level"], json!(LevelKey::main(LevelId::Meadow)));
    assert_eq!(record["score"], 30);
    assert_eq!(record["collected"], 3);
    assert_eq!(record["markers"], json!([1]));
    assert_eq!(record["state"]["visible_pickups"], 1);
    assert_eq!(record["state"]["travel_count"], 4);
    assert_eq!(record["state"]["restarts"], 1);
    assert_eq!(record["state"]["saves"], 0);
    assert_eq!(record["state"]["restores"], 0);
    let audio = &record["audio"];
    assert_eq!(audio["verification_requested"], true);
    assert_eq!(audio["trajectory"], "ordinary_travel_script");
    assert_eq!(audio["trajectory_completed"], true);
    let snapshot = &audio["snapshot"];
    assert_eq!(number(snapshot, "accepted"), 7);
    assert!(number(snapshot, "started") > 0);
    assert_eq!(accepted_causes(snapshot), expected_causes());
    let frames = number(snapshot, "output_frames");
    assert!(frames <= 48_000 * 60);
    let runtime = number(snapshot, "runtime");
    let mut cues = BTreeMap::<u64, Cue>::new();
    let mut scopes = Vec::<(u64, u64)>::new();
    let mut last_frame = 0;
    let mut last_sequence = 0;
    let mut starts = 0;
    let mut completions = 0;
    let mut cancellations = 0;
    for entry in snapshot["records"].as_array().unwrap() {
        assert_eq!(number(entry, "runtime"), runtime);
        let frame = number(entry, "frame");
        assert!(frame >= last_frame && frame <= frames);
        last_frame = frame;
        let instance = number(entry, "instance");
        assert!(instance > 0);
        match entry["kind"].as_str().unwrap() {
            "scope_applied" => {
                assert!(
                    scopes
                        .last()
                        .is_none_or(|(previous, _)| *previous < instance)
                );
                assert!(
                    record["scene_receipts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|scene| scene["receipt"]["instance_id"] == instance)
                );
                scopes.push((instance, frame));
            }
            "cue" => {
                let sequence = number(entry, "sequence");
                let source = match entry["clip"].as_str().unwrap() {
                    "pickup" => 0,
                    "travel" => 1,
                    clip => panic!("unexpected clip: {clip}"),
                };
                assert_eq!(entry["clip"], entry["cause"]["kind"]);
                if entry["phase"]["kind"] == "accepted" {
                    assert!(sequence > last_sequence);
                    last_sequence = sequence;
                    assert_eq!(scopes.last().map(|(current, _)| *current), Some(instance));
                    let scene = record["scene_receipts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|scene| scene["receipt"]["instance_id"] == instance)
                        .unwrap();
                    assert_eq!(scene["level"], entry["cause"]["level"]["level"]);
                    assert!(
                        cues.insert(
                            sequence,
                            Cue {
                                record: entry.clone(),
                                source,
                                start: None,
                                end: None
                            }
                        )
                        .is_none()
                    );
                    continue;
                }
                let cue = cues
                    .get_mut(&sequence)
                    .expect("actual callback lifecycle follows acceptance");
                assert_eq!(entry["instance"], cue.record["instance"]);
                assert_eq!(entry["cause"], cue.record["cause"]);
                assert_eq!(source, cue.source);
                assert!(frame >= number(&cue.record, "frame"));
                match entry["phase"]["kind"].as_str().unwrap() {
                    "started" => {
                        assert_eq!(scopes.last().map(|(current, _)| *current), Some(instance));
                        assert!(cue.start.replace(frame).is_none() && cue.end.is_none());
                        starts += 1;
                    }
                    "completed" => {
                        assert_eq!(
                            frame,
                            cue.start.unwrap() + request.source_samples[source].len() as u64
                        );
                        assert!(cue.end.replace((frame, false)).is_none());
                        completions += 1;
                    }
                    "cancelled" => {
                        assert_eq!(entry["phase"]["started"], cue.start.is_some());
                        assert!(cue.end.replace((frame, true)).is_none());
                        cancellations += 1;
                    }
                    phase => panic!("ordinary gameplay published an invalid phase: {phase}"),
                }
            }
            kind => panic!("unexpected original audio record: {kind}"),
        }
    }
    assert_eq!(number(snapshot, "started"), starts);
    assert_eq!(number(snapshot, "completed"), completions);
    assert_eq!(number(snapshot, "cancelled"), cancellations);
    assert_eq!(snapshot["desired_instance"], snapshot["applied_instance"]);
    assert_eq!(
        scopes.last().map(|(instance, _)| *instance),
        snapshot["applied_instance"].as_u64()
    );
    let mut mixed = vec![0.0_f32; frames as usize];
    for cue in cues.values() {
        let (end, cancelled) = cue
            .end
            .expect("every accepted effect has its terminal callback fact");
        let instance = number(&cue.record, "instance");
        let cut = scopes
            .iter()
            .find(|(next, _)| *next > instance)
            .map(|(_, frame)| *frame);
        assert!(cut.is_none_or(|cut| end <= cut));
        if cancelled {
            assert_eq!(cut, Some(end));
        }
        if let Some(start) = cue.start {
            let samples = &request.source_samples[cue.source];
            assert!(end >= start && end - start <= samples.len() as u64);
            assert!(!cancelled || end - start < samples.len() as u64);
            for (frame, sample) in (start..end).zip(samples) {
                mixed[frame as usize] += *sample;
            }
        }
    }
    let stereo = mixed
        .into_iter()
        .flat_map(|sample| [sample.clamp(-1.0, 1.0); 2])
        .collect::<Vec<_>>();
    let pcm = &audio["pcm"];
    assert_eq!(pcm["frames"], frames);
    assert_eq!(pcm["samples_compared"], frames * 2);
    assert_eq!(
        pcm["nonzero_samples"].as_u64().unwrap(),
        stereo.iter().filter(|sample| **sample != 0.0).count() as u64
    );
    assert_eq!(pcm["callback_blake3"], package::sample_hash(&stereo));
    assert_eq!(
        pcm["pickup_source_blake3"],
        package::sample_hash(&request.source_samples[0])
    );
    assert_eq!(
        pcm["travel_source_blake3"],
        package::sample_hash(&request.source_samples[1])
    );
    assert_eq!(pcm["pickup_source_frames"], request.source_samples[0].len());
    assert_eq!(pcm["travel_source_frames"], request.source_samples[1].len());
    assert_eq!(pcm["sample_rate"], 48_000);
    assert_eq!(pcm["channels"], 2);
    assert_eq!(pcm["sample_encoding"], "f32-le");
    assert_eq!(pcm["accepted_pickups"], 3);
    assert_eq!(pcm["accepted_travels"], 4);
    assert_eq!(pcm["scope_applications"], scopes.len());
    assert_eq!(pcm["stale_scope_cancellations"], cancellations);
    assert!(pcm["maximum_absolute_error"].as_f64().unwrap() <= 0.000_001);
}
