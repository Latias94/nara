//! Decision-local timings through the ordinary data-only Editor interface.

use super::*;
use nara::{
    project_host::{EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress},
    tooling::EditorDocumentDigest,
};

const WARMUP_SAMPLES: usize = 2;
const TIMED_SAMPLES: usize = 16;

#[test]
#[ignore = "decision-local Open/Save/Reopen measurement; run explicitly and retain output"]
fn measure_editor_owner_thread_work() {
    // Exclude schema generation and fixture serialization from all measured intervals.
    let _ = built_in_authoring_schema_bundle();
    for population in [64, 1_024, 4_096] {
        let project = TestProject::new("responsiveness");
        let scene = SceneDocument::from_ordered_roots(
            (0..population)
                .map(|index| SceneEntityRecord::new(scene_id(&format!("entity-{index:05}")))),
        );
        let source = scene.to_json_string().unwrap();
        fs::write(project.scene_path(), &source).unwrap();

        for sample in 0..WARMUP_SAMPLES + TIMED_SAMPLES {
            let measured = sample >= WARMUP_SAMPLES;
            let sample = sample.saturating_sub(WARMUP_SAMPLES);
            // Capability acquisition is separately visible, rather than hidden in phase one.
            let started = Instant::now();
            let capability = project.capability();
            let acquire = started.elapsed();
            if measured {
                emit(
                    population,
                    source.len(),
                    sample,
                    "root-acquisition",
                    acquire,
                );
            }
            let mut host = EditorProjectOpenHost::new();
            let mut attempt = host.begin(capability, None);
            let mut editor = loop {
                let phase = format!("open-{:?}", attempt.phase());
                let started = Instant::now();
                let progress = host.advance(attempt);
                let duration = started.elapsed();
                if measured {
                    emit(population, source.len(), sample, &phase, duration);
                }
                match progress {
                    EditorProjectOpenProgress::Pending(next) => attempt = next,
                    EditorProjectOpenProgress::Opened(editor) => break editor,
                    EditorProjectOpenProgress::Rejected(error) => {
                        panic!("measurement project rejected: {error:?}");
                    }
                }
            };
            let document = editor.current_frame().workspace().active_document.unwrap();
            assert_eq!(
                current_scene_model(&editor, document)
                    .editor
                    .inspector
                    .entities
                    .len(),
                population,
            );
            // Finish the tiny source-index fixture before persistence timings.
            for _ in 0..32 {
                if !editor.current_frame().project_browser().has_pending_work() {
                    break;
                }
                editor.drive_editor_frame(Duration::ZERO);
            }
            assert!(!editor.current_frame().project_browser().has_pending_work());
            let started = Instant::now();
            editor.drive_editor_frame(Duration::ZERO);
            let idle = started.elapsed();
            if measured {
                emit(population, source.len(), sample, "idle-drive", idle);
            }

            assert!(apply_scene_patch(
                &mut editor,
                document,
                add_entity_patch("saved-edit")
            ));
            let captured_revision = editor
                .current_frame()
                .workspace()
                .scene(document)
                .unwrap()
                .revision;
            let started = Instant::now();
            let accepted = editor_driver::request_persistence(
                &mut editor,
                EditorPersistenceCommand::Save {
                    document: Some(document),
                },
            );
            let request = started.elapsed();
            assert_eq!(accepted, EditorPersistenceRequestResult::Accepted);
            let started = Instant::now();
            let frame = editor.drive_editor_frame(Duration::ZERO);
            let drive = started.elapsed();
            let saved_digest = match frame.persistence().result() {
                Some(EditorPersistenceResult::Saved {
                    document: saved,
                    revision,
                    digest,
                }) => {
                    assert_eq!(saved, document);
                    assert_eq!(revision, captured_revision);
                    digest
                }
                other => panic!("Save did not complete: {other:?}"),
            };
            assert!(!frame.workspace().scene(document).unwrap().dirty);
            let persisted = fs::read(project.scene_path()).unwrap();
            assert_eq!(
                saved_digest,
                EditorDocumentDigest::new(
                    persisted.len() as u64,
                    *blake3::hash(&persisted).as_bytes()
                )
            );
            if measured {
                emit(population, source.len(), sample, "save-request", request);
                emit(population, source.len(), sample, "save-drive", drive);
            }
            assert_eq!(
                editor_driver::request_persistence(
                    &mut editor,
                    EditorPersistenceCommand::AcknowledgeResult
                ),
                EditorPersistenceRequestResult::Accepted,
            );
            editor.drive_editor_frame(Duration::ZERO);
            // Reopen must actually replace dirty state with the saved revision.
            assert!(apply_scene_patch(
                &mut editor,
                document,
                add_entity_patch("unsaved-edit")
            ));
            let started = Instant::now();
            let accepted = editor_driver::request_persistence(
                &mut editor,
                EditorPersistenceCommand::Reopen {
                    document: Some(document),
                },
            );
            let request = started.elapsed();
            assert_eq!(accepted, EditorPersistenceRequestResult::Accepted);
            let started = Instant::now();
            let frame = editor.drive_editor_frame(Duration::ZERO);
            let drive = started.elapsed();
            let model = frame.workspace().scene(document).unwrap();
            assert!(matches!(
                frame.persistence().result(),
                Some(EditorPersistenceResult::Opened { document: opened, revision, digest })
                    if opened == document && revision == model.revision && digest == saved_digest
            ));
            assert!(!model.dirty);
            let entities = &frame
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .entities;
            assert!(
                entities
                    .iter()
                    .any(|entity| entity.id == scene_id("saved-edit"))
            );
            assert!(
                !entities
                    .iter()
                    .any(|entity| entity.id == scene_id("unsaved-edit"))
            );
            assert_eq!(
                frame
                    .workspace()
                    .active_scene
                    .as_ref()
                    .unwrap()
                    .editor
                    .inspector
                    .entities
                    .len(),
                population + 1
            );
            if measured {
                emit(population, source.len(), sample, "reopen-request", request);
                emit(population, source.len(), sample, "reopen-drive", drive);
            }
            // Each Open sample starts from identical bytes. Restore outside the measured span.
            drop(frame);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !matches!(
                editor.advance_close(),
                EditorProjectCloseProgress::Complete(_)
            ) {
                assert!(
                    Instant::now() < deadline,
                    "sample retained an unfinished session close"
                );
                thread::sleep(Duration::from_millis(1));
            }
            drop(editor);
            fs::write(project.scene_path(), &source).unwrap();
        }
    }
}

fn emit(population: usize, source_bytes: usize, sample: usize, phase: &str, duration: Duration) {
    println!(
        "{}",
        serde_json::json!({
            "measurement": "editor-owner-thread-v1",
            "population": population,
            "baseline_source_bytes": source_bytes,
            "sample": sample,
            "phase": phase,
            "elapsed_ns": duration.as_nanos(),
            "profile": if cfg!(debug_assertions) { "debug" } else { "optimized" },
        })
    );
}
