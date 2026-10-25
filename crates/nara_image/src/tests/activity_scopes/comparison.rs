//! Bounded R4b evidence, not a product benchmark or P99 estimate. Both candidates have two
//! aggregate IO workers and identical activity custody; total-saturation fairness is not promised.

use super::*;
use nara_tasks::TaskTerminal;
use serde_json::{Value, json};
use std::sync::Condvar;

const WAIT: Duration = Duration::from_secs(5);
const HOLD: Duration = Duration::from_millis(30);

fn observe(facilities: &[Arc<TaskFacilities>], workers: &[usize]) -> Value {
    let stats: Vec<_> = facilities.iter().map(|facility| facility.stats()).collect();
    for (facility, stats) in facilities.iter().zip(&stats) {
        for kind in [TaskPoolKind::Compute, TaskPoolKind::AsyncCompute] {
            assert_eq!(facility.config().kind(kind).workers().get(), 1);
            let dormant = stats.for_kind(kind);
            assert_eq!(dormant.live_workers, 0);
            assert_eq!(dormant.admitted, 0);
            assert_eq!(dormant.started, 0);
        }
    }
    let io: Vec<_> = stats
        .iter()
        .map(|stats| stats.for_kind(TaskPoolKind::Io))
        .collect();
    let observation = json!({
        "facilities": facilities.len(),
        "active_facilities": io.iter().filter(|stats| stats.live_workers != 0).count(),
        "io_live": io.iter().map(|stats| stats.live_workers).collect::<Vec<_>>(),
        "io_running": io.iter().map(|stats| stats.running).collect::<Vec<_>>(),
        "io_queued": io.iter().map(|stats| stats.queued).collect::<Vec<_>>(),
        "compute_live": stats.iter().map(|stats| stats.for_kind(TaskPoolKind::Compute).live_workers).collect::<Vec<_>>(),
        "async_compute_live": stats.iter().map(|stats| stats.for_kind(TaskPoolKind::AsyncCompute).live_workers).collect::<Vec<_>>(),
    });
    assert_eq!(observation["io_live"], json!(workers));
    observation
}

struct ImageOwner {
    runtime: RuntimeInstance,
    handle: Handle<ImageAsset>,
    directory: PathBuf,
    version: AssetVersion,
    applied: u32,
    pixels: [u8; 4],
}

impl ImageOwner {
    fn assert_image(&self) {
        assert_reloaded_image(
            &self.runtime,
            self.handle,
            &self.pixels,
            self.version,
            self.applied,
        );
    }

    fn reload(&mut self, record: &AssetRecord, pixels: [u8; 4], clock: Instant) -> Value {
        // Encode/write outside the interval; time actual SourceChange through PNG application.
        fs::write(
            self.directory.join(record.path().as_str()),
            rgba_png(1, 1, &pixels),
        )
        .unwrap();
        let started = Instant::now();
        edit_image_pipeline(
            &mut self.runtime,
            ImagePipelineEdit::SourceChange {
                path: record.path().clone(),
                kind: AssetSourceChangeKind::Modified,
            },
        );
        let expected = self.applied + 1;
        let mut drive_max = Duration::ZERO;
        let mut admitted_at = None;
        while self.runtime.world().resource::<ImageReloadStats>().applied != expected
            && started.elapsed() < WAIT
        {
            let drive_started = Instant::now();
            self.runtime.drive(Duration::ZERO).unwrap();
            drive_max = drive_max.max(drive_started.elapsed());
            if admitted_at.is_none()
                && self.runtime.world().resource::<ImageReloadStats>().spawned == expected
            {
                admitted_at = Some(Instant::now());
            }
            if self.runtime.world().resource::<ImageReloadStats>().applied != expected {
                thread::sleep(Duration::from_millis(1));
            }
        }
        let completed = Instant::now();
        self.applied = expected;
        self.pixels = pixels;
        self.assert_image();
        json!({ "reload_wall_us": completed.duration_since(started).as_micros(),
            "owner_drive_max_us": drive_max.as_micros(),
            "request_offset_us": started.duration_since(clock).as_micros(),
            "image_admission_observed_offset_us": admitted_at.expect("PNG job was admitted").duration_since(clock).as_micros(),
            "completion_offset_us": completed.duration_since(clock).as_micros() })
    }

    fn probe(&self) -> TaskHandle<()> {
        self.runtime
            .world()
            .resource::<TaskPools>()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, IMAGE_RELOAD_TASK_DOMAIN),
                |_| (),
            )
            .into_handle()
            .unwrap()
    }
}

fn completed<T: fmt::Debug>(handle: &mut TaskHandle<T>) -> T {
    let started = Instant::now();
    while handle.return_observer().state() == TaskReturnState::Pending && started.elapsed() < WAIT {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(handle.return_observer().state(), TaskReturnState::Returned);
    match handle.try_take() {
        Some(TaskTerminal::Completed(value)) => value,
        terminal => panic!("expected completed task, got {terminal:?}"),
    }
}

fn probe_us(mut handle: TaskHandle<()>) -> u128 {
    completed(&mut handle);
    handle
        .admission_to_start()
        .expect("IO probe must actually start")
        .as_micros()
}

fn wait_idle(facilities: &[Arc<TaskFacilities>]) {
    let started = Instant::now();
    while facilities.iter().any(|facility| {
        let io = facility.stats().for_kind(TaskPoolKind::Io);
        io.queued != 0 || io.running != 0
    }) {
        assert!(
            started.elapsed() < WAIT,
            "IO work did not physically return"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

struct ReleaseOnDrop(Arc<(Mutex<Option<Instant>>, Condvar)>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let (released, changed) = &*self.0;
        released
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_or_insert_with(Instant::now);
        changed.notify_all();
    }
}

fn interference(
    owners: &mut [ImageOwner; 2],
    facilities: &[Arc<TaskFacilities>],
    record: &AssetRecord,
    shared: bool,
) -> Value {
    wait_idle(facilities);
    let workers: &[usize] = if shared { &[2] } else { &[1, 1] };
    let gate = Arc::new((Mutex::new(None), Condvar::new()));
    let _release_on_failure = ReleaseOnDrop(Arc::clone(&gate));
    let (entered, started) = mpsc::channel();
    let mut held = Vec::new();
    let pools = owners[0].runtime.world().resource::<TaskPools>();
    for index in 0..2 {
        let gate = Arc::clone(&gate);
        let entered = entered.clone();
        let outcome = pools.spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(0, IMAGE_RELOAD_TASK_DOMAIN),
            move |_| {
                let _ = entered.send(());
                let (released, changed) = &*gate;
                let (released, _) = changed
                    .wait_timeout_while(released.lock().unwrap(), WAIT, |at| at.is_none())
                    .unwrap();
                (*released, Instant::now())
            },
        );
        held.push(outcome.into_handle().unwrap());
        if index == 0 || shared {
            started.recv_timeout(WAIT).unwrap();
        }
    }
    let before = observe(facilities, workers);
    assert_eq!(
        before["io_running"],
        if shared { json!([2]) } else { json!([1, 0]) }
    );
    assert_eq!(
        before["io_queued"],
        if shared { json!([0]) } else { json!([1, 0]) }
    );
    let interactive_started = Instant::now();
    thread::scope(|scope| {
        let release = ReleaseOnDrop(Arc::clone(&gate));
        let releaser = scope.spawn(move || {
            thread::sleep((interactive_started + HOLD).saturating_duration_since(Instant::now()));
            drop(release);
        });
        // This real sibling-IO queue probe is not Image's private handle or its queue latency.
        let probe = owners[1].probe();
        let reload = owners[1].reload(record, [91, 37, 201, 255], interactive_started);
        releaser.join().unwrap();
        let released_at = gate.0.lock().unwrap().unwrap();
        let admission_to_start_us = probe_us(probe);
        let mut held_exit_us = Vec::new();
        for handle in &mut held {
            let (observed_release, exited) = completed(handle);
            assert_eq!(observed_release, Some(released_at));
            held_exit_us.push(exited.duration_since(interactive_started).as_micros());
        }
        owners[0].assert_image();
        json!({ "held_tasks_from_owner_0": 2, "held": before,
            "release_target_us": HOLD.as_micros(),
            "actual_release_us": released_at.duration_since(interactive_started).as_micros(),
            "held_exit_us": held_exit_us, "reload": reload,
            "sibling_io_probe_admission_to_start_us": admission_to_start_us })
    })
}

fn trial(shared: bool, trial: usize) {
    let root = unique_temp_root();
    let record = image_record("textures/player.png");
    let workers: &[usize] = if shared { &[2] } else { &[1, 1] };
    let started = Instant::now();
    let facilities: Vec<_> = workers
        .iter()
        .map(|&workers| {
            Arc::new(
                TaskFacilities::try_new_lazy(bounded_task_config(workers, workers * 4)).unwrap(),
            )
        })
        .collect();
    let facilities_creation_us = started.elapsed().as_micros();
    let creation = observe(&facilities, &vec![0; facilities.len()]);
    assert_eq!(
        Arc::ptr_eq(&facilities[0], facilities.last().unwrap()),
        shared
    );
    let mut server = AssetServer::new();
    server.reserve_record::<ImageAsset>(&record).unwrap();
    let started = Instant::now();
    let mut owners: [ImageOwner; 2] = std::array::from_fn(|owner| {
        let directory = root.join(owner.to_string());
        let pixels = [255, owner as u8, 0, 255];
        fs::create_dir_all(directory.join("textures")).unwrap();
        let (app, handle) = shared_image_app(
            &facilities[if shared { 0 } else { owner }],
            &directory,
            &record,
            server.clone(),
            &pixels,
        );
        let runtime = RuntimeInstance::start(app).unwrap();
        let version = runtime
            .world()
            .resource::<AssetStates>()
            .version(handle.id())
            .unwrap();
        ImageOwner {
            runtime,
            handle,
            directory,
            version,
            applied: 0,
            pixels,
        }
    });
    let runtime_pair_creation_us = started.elapsed().as_micros();
    assert_eq!(owners[0].handle.id(), owners[1].handle.id());
    for owner in &mut owners {
        owner.runtime.drive(Duration::ZERO).unwrap();
        owner.assert_image();
    }
    thread::sleep(Duration::from_millis(2));
    let idle = observe(&facilities, &vec![0; facilities.len()]);
    let mut first_use = Vec::new();
    let mut warm = Vec::new();
    let mut post_reload_probe_us = Vec::new();
    for index in 0..6 {
        let owner = index % 2;
        let reload = owners[owner].reload(&record, [index as u8 + 1, 89, 211, 255], Instant::now());
        for owner in &owners {
            owner.assert_image();
        }
        post_reload_probe_us.push(probe_us(owners[owner].probe()));
        if index < 2 {
            let expected: &[usize] = if shared {
                &[2]
            } else if index == 0 {
                &[1, 0]
            } else {
                workers
            };
            first_use.push(json!({ "owner": owner, "reload": reload,
                "multiplicity": observe(&facilities, expected) }));
        } else {
            warm.push(json!({ "owner": owner, "reload": reload }));
        }
    }
    let interference = interference(&mut owners, &facilities, &record, shared);
    wait_idle(&facilities);
    let io: Vec<_> = facilities
        .iter()
        .map(|facility| facility.stats().for_kind(TaskPoolKind::Io))
        .collect();
    assert_eq!(io.iter().map(|stats| stats.admitted).sum::<u64>(), 16);
    for stats in io {
        assert_eq!(stats.started, stats.admitted);
        assert_eq!(stats.completed, stats.admitted);
        for count in [
            stats.rejected,
            stats.coalesced,
            stats.failed,
            stats.cancelled,
        ] {
            assert_eq!(count, 0);
        }
    }
    let active = observe(&facilities, workers);
    for mut owner in owners {
        let runtime = &mut owner.runtime;
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        assert!(drive_until(runtime, |runtime| runtime.state() == RuntimeState::Stopped).unwrap());
        let report = runtime.world().resource::<TaskScopeCloseReport>();
        assert!(report.is_complete() && !report.cleanup_failed);
        assert!(!runtime.world().contains_resource::<TaskShutdownReport>());
        owner.runtime.finish().unwrap();
        observe(&facilities, workers);
    }
    for facility in &facilities {
        let report = facility.shutdown_blocking().unwrap();
        assert!(report.is_complete() && !report.has_cleanup_failure());
    }
    observe(&facilities, &vec![0; facilities.len()]);
    println!(
        "{}",
        json!({ "r4b": "trial", "trial": trial,
        "candidate": if shared { "lazy_shared" } else { "lazy_per_runtime" },
        "facilities_creation_us": facilities_creation_us, "runtime_pair_fixture_creation_us": runtime_pair_creation_us,
        "creation": creation, "idle_after_owner_drives_and_2ms_sleep": idle, "active": active,
        "first_use": first_use, "warm": warm, "post_reload_io_probe_admission_to_start_us": post_reload_probe_us,
        "interference": interference, "image_jobs": 7, "io_probe_jobs": 7, "held_jobs": 2, "submitted_total": 16 })
    );
    remove_temp_root(&root);
}

#[test]
#[ignore = "bounded R4b measurements; caller owns serial execution and interpretation"]
fn measure_lazy_runtime_vs_shared_image_facilities() {
    println!(
        "{}",
        json!({ "r4b": "method", "units": "microseconds",
        "cold_trials_per_candidate": 16, "first_use_reload_samples_per_candidate": 32,
        "warm_reload_samples_per_candidate": 64, "interference_trials_per_candidate": 16,
        "aggregate_io_workers": 2, "aggregate_io_pending_capacity": 8,
        "first_use_scope": "owner 0 activates IO; owner 1 first reload reuses IO only in shared candidate",
        "wall_scope": "SourceChange through owner-observed PNG application; excludes source write",
        "probe_scope": "TaskHandle accepted-admission-to-start on same IO queue; not Image queue latency",
        "post_reload_probe_order": "owner 0 first, owner 1 first, then four alternating warm reloads",
        "poll_interval_us": 1000, "no_p99_or_product_performance_claim": true,
        "total_saturation_fairness_guaranteed": false })
    );
    for index in 0..16 {
        for shared in [index % 2 != 0, index % 2 == 0] {
            trial(shared, index);
        }
    }
}
