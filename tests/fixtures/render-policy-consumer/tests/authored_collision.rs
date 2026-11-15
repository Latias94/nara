#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::Duration,
};

use nara_render_wgpu::{
    WgpuImageOutput, WgpuRenderHost, WgpuRenderProducer, WgpuStockPolicy, WgpuSubmissionPrefix,
    WgpuWorldColor,
};

#[path = "support/material_readback.rs"]
mod material_readback;
#[path = "support/authored_collision/model.rs"]
mod model;
#[path = "support/authored_collision/oracle.rs"]
mod oracle;
#[path = "support/authored_collision/scene.rs"]
mod scene;
#[path = "support/authored_collision/source.rs"]
mod source;
pub mod support;

use model::{HEIGHT, Sample, TIMESTEP, Variant, WIDTH};
use scene::Scene;
use support::image;

struct Record {
    sample: Sample,
    output: WgpuImageOutput,
    prefix: WgpuSubmissionPrefix,
    pixels: Vec<[u8; 4]>,
}

struct Harness {
    scene: Option<Scene>,
    host: WgpuRenderHost,
    producers: Vec<WgpuRenderProducer>,
    active_producer: Option<usize>,
    records: Vec<Record>,
    host_frame: u64,
    edge_candidates: usize,
    edge_alternates: usize,
    backend: String,
    fallback: bool,
}

impl Harness {
    fn sample(&mut self, delta: Duration) -> Sample {
        self.scene.as_mut().unwrap().drive(delta)
    }

    fn publish(&mut self, sample: &Sample) -> usize {
        oracle::assert_spatial(sample);
        self.host_frame += 1;
        self.host.begin_frame(self.host_frame).unwrap();
        let adapter = self.host.runtime_facts().adapter.unwrap();
        assert_eq!(adapter.backend(), self.backend);
        assert_eq!(adapter.device_type() == "cpu", self.fallback);
        if self.host_frame == 1 {
            eprintln!(
                "authored-collision: adapter={adapter:?}, forced_fallback={}",
                self.fallback
            );
        }
        let producer_index = if let Some(index) = self.active_producer {
            index
        } else {
            let producer = self.host.create_producer().unwrap();
            let index = self.producers.len();
            self.producers.push(producer);
            self.active_producer = Some(index);
            index
        };
        let producer = &self.producers[producer_index];
        let target = image::target(WIDTH, HEIGHT);
        let input = sample.input.with_image_target(target).unwrap();
        assert_eq!(input.installation(), sample.input.installation());
        assert_eq!(
            input.packet().source_generation(),
            sample.input.packet().source_generation()
        );
        assert_eq!(
            input.packet().frame_index(),
            sample.input.packet().frame_index()
        );
        assert_eq!(input.packet().views(), sample.input.packet().views());
        assert_eq!(
            input.packet().captured_time(),
            sample.input.packet().captured_time()
        );
        assert!(std::ptr::eq(
            input.captured::<WgpuStockPolicy>().unwrap(),
            sample.input.captured::<WgpuStockPolicy>().unwrap(),
        ));
        let reservation = self
            .host
            .reserve_image_target(producer, input.packet().source_generation(), target)
            .unwrap();
        self.host
            .activate_image_target(producer, reservation)
            .unwrap();
        let candidate = self.host.prepare(producer, &input).unwrap();
        let published = self.host.submit_candidate(producer, candidate).unwrap();
        let final_texture = published.final_texture::<WgpuWorldColor>().unwrap();
        assert_eq!(final_texture.producer_id(), producer.id());
        assert_eq!(
            final_texture.source_generation(),
            sample.input.packet().source_generation()
        );
        assert_eq!(
            final_texture.frame_index(),
            sample.input.packet().frame_index()
        );
        let prefix = published.outputs().submitted_prefix().clone();
        assert!(prefix.submission_index().is_some());
        let output = published.into_image();
        assert_eq!(
            output.receipt().backend_instance_id(),
            self.host.backend_instance_id()
        );
        let index = self.records.len();
        self.records.push(Record {
            sample: sample.clone(),
            output,
            prefix,
            pixels: Vec::new(),
        });
        let record = &mut self.records[index];
        record.pixels = material_readback::read(&mut self.host, &record.output);
        let expected = oracle::expected_image(sample);
        assert_eq!(expected.pixels().len(), record.pixels.len());
        let agreement = oracle::assert_matches(&expected, &record.pixels);
        self.edge_candidates += agreement.ambiguous_pixels;
        self.edge_alternates += agreement.alternate_pixels;
        assert!(record.pixels.contains(&model::BODY_COLOR));
        assert!(record.pixels.contains(&model::PLATFORM_COLOR));
        assert_eq!(
            self.host
                .frame_transaction_stats()
                .surface_acquire_attempts(),
            0
        );
        assert_eq!(self.host.frame_transaction_stats().presents(), 0);
        index
    }

    fn assert_retained(&mut self, index: usize) {
        let record = &self.records[index];
        let pixels = material_readback::read(&mut self.host, &record.output);
        assert_eq!(pixels, record.pixels);
        oracle::assert_matches(&oracle::expected_image(&record.sample), &pixels);
        image::settle(&record.prefix);
    }

    fn close_scene(&mut self) {
        if let Some(scene) = &mut self.scene {
            scene
                .close()
                .expect("the original Runtime/solver must physically retire");
            assert!(scene.world_dropped());
            assert_eq!(scene.native_counts(), (0, 0));
        }
        self.scene = None;
    }

    fn cleanup(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        if catch_unwind(AssertUnwindSafe(|| self.close_scene())).is_err() {
            failures.push("original Runtime/solver retirement failed".to_owned());
        }
        for producer in &self.producers {
            if catch_unwind(AssertUnwindSafe(|| {
                if let Some(prefix) = self.host.retire_producer(producer) {
                    image::settle(&prefix);
                }
            }))
            .is_err()
            {
                failures.push("original producer retirement failed".to_owned());
            }
        }
        self.records.clear();
        self.producers.clear();
        if catch_unwind(AssertUnwindSafe(|| image::finish(&mut self.host))).is_err() {
            failures.push("original Host retirement failed".to_owned());
        }
        for (name, count) in [
            ("retained native bytes", self.host.retained_resource_bytes()),
            (
                "candidate metadata",
                self.host.retained_candidate_metadata_bytes(),
            ),
            ("cleanup faults", self.host.cleanup_fault_count()),
        ] {
            if count != 0 {
                failures.push(format!("{name}: {count}"));
            }
        }
        failures
    }

    fn run(&mut self) {
        self.scene = Some(Scene::new());
        let original = self.sample(TIMESTEP);
        let original_debug = (*original.debug).clone();
        let original_document = self.scene.as_ref().unwrap().document();
        let first = self.publish(&original);
        let unprepared = self.sample(TIMESTEP + TIMESTEP / 2);
        let unprepared_debug = (*unprepared.debug).clone();
        let mut moving = self.sample(TIMESTEP * 3);
        for _ in 0..48 {
            let interpolated = oracle::expected_image(&moving);
            let current_only =
                oracle::image_from_poses(&moving.authored, None, moving.body, 0.0, false);
            if oracle::disjoint_pixels(&interpolated, &current_only) > 0 {
                break;
            }
            moving = self.sample(TIMESTEP);
        }
        let interpolated = oracle::expected_image(&moving);
        let current_only =
            oracle::image_from_poses(&moving.authored, None, moving.body, 0.0, false);
        assert!(
            oracle::disjoint_pixels(&interpolated, &current_only) > 0,
            "the actual moving sample must distinguish interpolation from current-only outside every edge allowance"
        );
        assert!(moving.tick > unprepared.tick);
        assert!(moving.body.translation.y < original.body.translation.y);
        assert_eq!(moving.render_time.interpolation_alpha, 0.5);
        assert_ne!(moving.fixed.as_ref().unwrap().body_before, moving.body);
        assert!((moving.body_presented.z_axis.y - moving.body.translation.y).abs() > 0.000_1);
        let moving_output = self.publish(&moving);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                oracle::assert_matches(&current_only, &self.records[moving_output].pixels);
            }))
            .is_err(),
            "actual GPU pixels must reject the incorrect current-only pose"
        );
        let before_pause = self.sample(Duration::ZERO);
        self.scene.as_mut().unwrap().pause();
        let paused = self.sample(TIMESTEP * 3);
        assert_eq!(paused.tick, before_pause.tick);
        assert_eq!(paused.body, before_pause.body);
        assert!(paused.paused);
        self.publish(&paused);
        let captures = paused.captures;
        let stepped_tick = self.scene.as_mut().unwrap().exact_step();
        assert_eq!(stepped_tick, paused.tick + 1);
        let stepped = self.sample(Duration::ZERO);
        assert_eq!(stepped.captures, captures + 1);
        assert_eq!(stepped.tick, stepped_tick);
        assert!(stepped.body.translation.y < paused.body.translation.y);
        self.publish(&stepped);
        self.scene.as_mut().unwrap().resume();
        let mut settled = self.sample(TIMESTEP);
        for _ in 0..120 {
            settled = self.sample(TIMESTEP);
        }
        oracle::assert_resting(&settled);
        let landed = self.publish(&settled);
        assert_ne!(self.records[first].pixels, self.records[landed].pixels);

        let old_revision = settled.revision;
        let before_invalid = self.scene.as_ref().unwrap().document();
        let rejected = self.scene.as_mut().unwrap().reject_invalid_edit();
        assert!(!rejected.applied);
        assert!(rejected.diagnostics.has_errors());
        assert_eq!(self.scene.as_ref().unwrap().document(), before_invalid);
        let same = self.sample(Duration::ZERO);
        assert_eq!(same.revision, old_revision);
        assert_eq!(same.body, settled.body);
        let edited_report = self.scene.as_mut().unwrap().edit(Variant::Edited);
        assert!(edited_report.applied);
        assert!(!edited_report.diagnostics.has_errors());
        let edited = self.sample(TIMESTEP);
        assert_ne!(edited.revision, old_revision);
        assert_ne!(edited.body_entity, settled.body_entity);
        assert_ne!(edited.platform_entity, settled.platform_entity);
        assert_eq!(edited.authored.variant, Variant::Edited);
        self.publish(&edited);
        let undo_report = self.scene.as_mut().unwrap().undo();
        assert!(undo_report.applied);
        assert!(!undo_report.diagnostics.has_errors());
        assert_eq!(self.scene.as_ref().unwrap().document(), original_document);
        let undone = self.sample(TIMESTEP);
        assert_eq!(undone.authored.variant, Variant::Initial);
        self.publish(&undone);
        let redo_report = self.scene.as_mut().unwrap().redo();
        assert!(redo_report.applied);
        assert!(!redo_report.diagnostics.has_errors());
        let redone = self.sample(TIMESTEP);
        assert_eq!(redone.authored.variant, Variant::Edited);
        self.publish(&redone);
        let mut settled_b = self.sample(TIMESTEP);
        for _ in 0..120 {
            settled_b = self.sample(TIMESTEP);
        }
        oracle::assert_resting(&settled_b);
        let landed_b = self.publish(&settled_b);
        assert!((settled_b.body.translation.y - settled.body.translation.y).abs() > 0.5);
        assert_ne!(self.records[landed].pixels, self.records[landed_b].pixels);

        let saved = self.scene.as_ref().unwrap().document();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("authored-platform.scene.json");
        let encoded = saved.to_json_string().unwrap();
        std::fs::write(&path, encoded.as_bytes()).unwrap();
        let reopened =
            nara::scene::SceneDocumentCandidate::decode_json_bytes(&std::fs::read(&path).unwrap())
                .unwrap()
                .publish(&self.scene.as_ref().unwrap().registry())
                .unwrap()
                .into_document();
        assert_eq!(reopened, saved);
        self.close_scene();
        assert_eq!(*original.debug, original_debug);
        assert_eq!(*unprepared.debug, unprepared_debug);
        self.active_producer = None;
        let replayed = self.publish(&unprepared);
        assert_ne!(self.records[replayed].pixels, self.records[landed_b].pixels);
        for index in 0..self.records.len() {
            self.assert_retained(index);
        }
        self.scene = Some(Scene::from_document(reopened, Variant::Edited));
        self.active_producer = None;
        let mut reopened_settled = self.sample(TIMESTEP);
        for _ in 0..120 {
            reopened_settled = self.sample(TIMESTEP);
        }
        oracle::assert_resting(&reopened_settled);
        let reopened_output = self.publish(&reopened_settled);
        assert_eq!(
            self.records[reopened_output].pixels,
            self.records[landed_b].pixels
        );
        self.close_scene();
        assert_eq!(self.producers.len(), 3);
        for producer in &self.producers {
            if let Some(prefix) = self.host.retire_producer(producer) {
                image::settle(&prefix);
            }
        }
        for index in 0..self.records.len() {
            self.assert_retained(index);
        }
        eprintln!(
            "authored-collision: publications={}, producers={}, edge_candidates={}, edge_alternates={}, old_input_first_realized_after_world_drop=true, source_history_and_disk_reopen=true, native_contact_and_interpolation=true",
            self.records.len(),
            self.producers.len(),
            self.edge_candidates,
            self.edge_alternates
        );
    }
}

#[test]
fn authored_runtime_source_history_and_paused_step_keep_original_capture_boundaries() {
    let mut scene = Scene::new();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let initial = scene.drive(TIMESTEP);
        oracle::assert_spatial(&initial);
        let document = scene.document();
        let retained = (*initial.debug).clone();
        scene.pause();
        let paused = scene.drive(Duration::ZERO);
        let captures = paused.captures;
        let report = scene.edit(Variant::Edited);
        assert!(report.applied && !report.diagnostics.has_errors());
        assert_eq!(scene.capture_count(), captures);
        let maintenance = scene.drive(Duration::ZERO);
        assert_eq!(maintenance.tick, paused.tick);
        assert!(maintenance.paused);
        assert_ne!(maintenance.body_entity, paused.body_entity);
        assert_eq!(maintenance.native_counts, paused.native_counts);
        assert_eq!(maintenance.debug.as_ref(), paused.debug.as_ref());
        assert!(!maintenance.contact);
        assert_eq!(maintenance.body, maintenance.authored.body);
        oracle::assert_spatial(&maintenance);
        let captures = maintenance.captures;
        assert_eq!(scene.exact_step(), paused.tick + 1);
        assert_eq!(scene.capture_count(), captures);
        let stepped = scene.drive(Duration::ZERO);
        assert_eq!(stepped.captures, captures + 1);
        assert_eq!(stepped.tick, paused.tick + 1);
        oracle::assert_spatial(&stepped);
        let rejected = scene.reject_invalid_edit();
        assert!(!rejected.applied && rejected.diagnostics.has_errors());
        let undo = scene.undo();
        assert!(undo.applied && !undo.diagnostics.has_errors());
        assert_eq!(scene.document(), document);
        let undone_maintenance = scene.drive(Duration::ZERO);
        assert_eq!(undone_maintenance.body, undone_maintenance.authored.body);
        oracle::assert_spatial(&undone_maintenance);
        scene.exact_step();
        oracle::assert_spatial(&scene.drive(Duration::ZERO));
        let redo = scene.redo();
        assert!(redo.applied && !redo.diagnostics.has_errors());
        let redone_maintenance = scene.drive(Duration::ZERO);
        assert_eq!(redone_maintenance.body, redone_maintenance.authored.body);
        oracle::assert_spatial(&redone_maintenance);
        scene.exact_step();
        oracle::assert_spatial(&scene.drive(Duration::ZERO));
        assert_eq!(initial.debug.as_ref(), &retained);
        initial
    }));
    let close = scene.close();
    if let Err(payload) = result {
        if let Err(error) = close {
            eprintln!("original CPU Runtime cleanup after failure: {error}");
        }
        resume_unwind(payload);
    }
    close.unwrap();
    assert!(scene.world_dropped());
    assert_eq!(scene.native_counts(), (0, 0));
    let retained = result.unwrap();
    assert_eq!(retained.debug.shapes.len(), 2);
    assert!(retained.input.captured::<WgpuStockPolicy>().is_some());
}

#[test]
#[ignore = "requires an explicitly selected actual native hardware or software adapter"]
fn authored_local_boxes_solve_edit_and_render_their_original_spatial_cuts() {
    let mut harness = native_harness();
    let result = catch_unwind(AssertUnwindSafe(|| harness.run()));
    let cleanup = harness.cleanup();
    if let Err(payload) = result {
        if !cleanup.is_empty() {
            eprintln!("authored-collision cleanup after original failure: {cleanup:?}");
        }
        resume_unwind(payload);
    }
    assert!(
        cleanup.is_empty(),
        "authored-collision cleanup: {cleanup:?}"
    );
}

#[test]
#[ignore = "requires an explicitly selected actual native hardware or software adapter"]
fn an_acceptance_panic_retires_the_original_runtime_solver_and_native_outputs() {
    let mut harness = native_harness();
    let result = catch_unwind(AssertUnwindSafe(|| {
        harness.scene = Some(Scene::new());
        let initial = harness.sample(TIMESTEP);
        harness.publish(&initial);
        panic!("injected authored collision acceptance failure");
    }));
    let cleanup = harness.cleanup();
    assert!(
        cleanup.is_empty(),
        "original owners must retire after the injected panic: {cleanup:?}"
    );
    let payload = result.expect_err("the acceptance fault must actually unwind");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"injected authored collision acceptance failure")
    );
}

fn native_harness() -> Harness {
    let backend = std::env::var("WGPU_BACKEND").expect("select WGPU_BACKEND");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    let fallback = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => panic!("select NARA_WGPU_FORCE_FALLBACK=0 or 1"),
    };
    Harness {
        scene: None,
        host: WgpuRenderHost::offscreen().with_resource_byte_limit(8 * 1024 * 1024),
        producers: Vec::new(),
        active_producer: None,
        records: Vec::new(),
        host_frame: 0,
        edge_candidates: 0,
        edge_alternates: 0,
        backend,
        fallback,
    }
}
