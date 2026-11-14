#[path = "../examples/diagnostic_capture/capture.rs"]
mod capture;

use std::{
    collections::BTreeSet,
    fmt, fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use capture::{Capture, GpuIdentity, Limits};
use serde_json::Value;
use tracing::{Level, dispatcher::with_default};

fn capture(events: usize, active_spans: usize) -> Capture {
    Capture::new(Limits {
        events,
        active_spans,
        retained_bytes: 16 * 1024 * 1024,
    })
}

fn output_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "nara-trace-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn export(capture: &Capture) -> Value {
    let path = output_path();
    capture.write_new(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn disabled_uses_no_subscriber_or_retained_storage() {
    let capture = Capture::disabled();
    with_default(&capture.dispatch(), || {
        assert!(tracing::info_span!(target: "nara::editor::work", "disabled").is_disabled());
        tracing::event!(name: "capture_tick", target: "nara::editor::work", Level::INFO, task_id = 1u64);
    });
    capture.fail();
    assert_eq!(capture.report().reserved_bytes, 0);
    assert_eq!(capture.report().incomplete, 0);
    let document = export(&capture);
    assert_eq!(document["traceEvents"], serde_json::json!([]));
    assert_eq!(document["nara_capture"]["events"], 0);
    assert_eq!(document["displayTimeUnit"], "ms");
}

#[test]
fn fixed_bounds_drop_without_growing_and_invalid_limits_latch_faults() {
    let capture = capture(2, 1);
    let reserved = capture.report().reserved_bytes;
    with_default(&capture.dispatch(), || {
        let root = tracing::info_span!(target: "nara::editor::work", "root");
        let overflow = tracing::info_span!(target: "nara::scene::work", "overflow");
        for poll in 0..3u64 {
            tracing::event!(name: "capture_tick", target: "nara::editor::work", Level::INFO, poll);
        }
        drop(overflow);
        drop(root);
    });
    let report = capture.report();
    assert_eq!((report.events, report.active_spans), (2, 0));
    assert_eq!((report.dropped_events, report.dropped_spans), (2, 1));
    assert_eq!(report.reserved_bytes, reserved);
    assert!(report.high_water_bytes > 0 && report.high_water_bytes <= reserved);
    assert!(report.incomplete >= 3);
    for limits in [
        Limits {
            events: 16_385,
            active_spans: 1,
            retained_bytes: usize::MAX,
        },
        Limits {
            events: 1,
            active_spans: 65,
            retained_bytes: usize::MAX,
        },
        Limits {
            events: 1,
            active_spans: 1,
            retained_bytes: 1,
        },
        Limits {
            events: usize::MAX,
            active_spans: usize::MAX,
            retained_bytes: usize::MAX,
        },
    ] {
        let invalid = Capture::new(limits);
        with_default(
            &invalid.dispatch(),
            || tracing::event!(name: "capture_tick", target: "nara::editor::work", Level::INFO, poll = 1u64),
        );
        assert_eq!(invalid.report().faults, 1);
        assert_eq!(invalid.report().reserved_bytes, 0);
        assert_eq!(invalid.report().dropped_events, 1);
    }
}

#[test]
fn nested_and_worker_spans_keep_parents_and_whole_lifetimes() {
    let capture = capture(16, 4);
    let root = with_default(&capture.dispatch(), || {
        let root = tracing::info_span!(target: "nara::editor::work", "root", task_id = 7u64,
            outcome = "pending", current_revision = tracing::field::Empty);
        {
            let _entered = root.enter();
            let _nested = tracing::info_span!(target: "nara::scene::work", "nested");
        }
        root
    });
    let worker_parent = root.clone();
    let dispatch = capture.dispatch();
    std::thread::spawn(move || {
        with_default(&dispatch, || {
            let worker =
                tracing::info_span!(target: "nara::scene::work", parent: &worker_parent, "worker");
            std::thread::sleep(Duration::from_millis(2));
            tracing::event!(name: "published", target: "nara::scene::work", parent: &worker,
            Level::INFO, outcome = "published", admission_to_start_ns = 25u64);
        })
    })
    .join()
    .unwrap();
    assert_eq!(capture.report().active_spans, 1);
    root.record("outcome", "complete");
    root.record("current_revision", 9u64);
    drop(root);
    assert_eq!(
        (capture.report().active_spans, capture.report().incomplete),
        (0, 0)
    );
    let document = export(&capture);
    let events = document["traceEvents"].as_array().unwrap();
    let named = |name| events.iter().find(|event| event["name"] == name).unwrap();
    let root = named("root");
    for name in ["nested", "worker"] {
        assert_eq!(named(name)["args"]["parent_id"], root["args"]["capture_id"]);
    }
    assert_eq!(
        named("published")["args"]["parent_id"],
        named("worker")["args"]["capture_id"]
    );
    assert_eq!(named("published")["args"]["admission_to_start_ns"], 25);
    assert_eq!(root["args"]["current_revision"], 9);
    assert_eq!(root["args"]["outcome"], "complete");
    assert!(root["dur"].as_f64().unwrap() >= 2_000.0);
    assert_eq!(root["ph"], "X");
    assert_eq!(named("published")["ph"], "i");
    assert_eq!(named("published")["s"], "t");
    assert!(
        document["reserved_bytes_scope"]
            .as_str()
            .unwrap()
            .contains("excludes Registry")
    );
}

#[test]
fn concurrent_producers_preserve_complete_lifetimes_within_storage_limits() {
    let capture = capture(1_024, 8);
    let start = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for worker in 0..4u64 {
            let capture = &capture;
            let start = &start;
            scope.spawn(move || {
                with_default(&capture.dispatch(), || {
                    start.wait();
                    for index in 0..64u64 {
                        let task_id = worker * 64 + index;
                        let span = tracing::info_span!(target: "nara::editor::work", "root",
                            task_id, outcome = "pending");
                        tracing::event!(name: "published", target: "nara::editor::work", parent: &span,
                            Level::INFO, task_id, outcome = "published");
                        span.record("outcome", "complete");
                    }
                });
            });
        }
    });
    let report = capture.report();
    assert_eq!(report.incomplete, 0, "{report:?}");
    assert_eq!((report.events, report.active_spans), (512, 0));
    assert!(report.high_water_bytes <= report.reserved_bytes);
    let document = export(&capture);
    let events = document["traceEvents"].as_array().unwrap();
    let roots: std::collections::BTreeMap<_, _> = events
        .iter()
        .filter(|event| event["name"] == "root")
        .map(|event| {
            assert_eq!(event["args"]["outcome"], "complete");
            (
                event["args"]["task_id"].as_u64().unwrap(),
                event["args"]["capture_id"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(roots.len(), 256);
    for event in events.iter().filter(|event| event["name"] == "published") {
        let task_id = event["args"]["task_id"].as_u64().unwrap();
        assert_eq!(event["args"]["parent_id"], roots[&task_id]);
        assert_eq!(event["args"]["outcome"], "published");
    }
}

#[test]
fn cloned_parent_retires_after_children_and_repeated_lifetimes_stay_distinct() {
    let capture = capture(80, 2);
    with_default(&capture.dispatch(), || {
        let parent = tracing::info_span!(target: "nara::editor::work", "parent");
        let clone = parent.clone();
        let child = tracing::info_span!(target: "nara::scene::work", parent: &clone, "child");
        drop(parent);
        drop(clone);
        assert_eq!(
            (capture.report().events, capture.report().active_spans),
            (0, 2)
        );
        drop(child);
        assert_eq!(
            (capture.report().events, capture.report().active_spans),
            (2, 0)
        );
        for _ in 0..64 {
            let _span = tracing::info_span!(target: "nara::editor::work", "reused");
        }
    });
    let document = export(&capture);
    let events = document["traceEvents"].as_array().unwrap();
    let ids: BTreeSet<_> = events
        .iter()
        .map(|event| event["args"]["capture_id"].as_u64().unwrap())
        .collect();
    assert_eq!((events.len(), ids.len()), (66, 66));
    assert_eq!(
        events[0]["args"]["parent_id"],
        events[1]["args"]["capture_id"]
    );
    assert!(
        events[2..]
            .iter()
            .all(|event| event["args"].get("parent_id").is_none())
    );
    assert!(events[2..].windows(2).all(
        |pair| pair[0]["args"]["capture_id"].as_u64() < pair[1]["args"]["capture_id"].as_u64()
    ));
    assert_eq!(capture.report().incomplete, 0);
}

#[test]
fn debug_free_text_paths_and_wrong_numeric_types_are_never_retained() {
    struct NeverFormat;
    impl fmt::Debug for NeverFormat {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            panic!("Debug must not run")
        }
    }
    let capture = capture(8, 2);
    let canary = String::from("C:\\private\\CAPTURE_SECRET_CANARY-token");
    with_default(&capture.dispatch(), || {
        let span = tracing::info_span!(target: "nara::editor::work", "privacy", task_id = 7u64,
            outcome = "pending", document = canary.as_str(), source = ?NeverFormat);
        span.record("outcome", "success");
        tracing::event!(name: "privacy_event", target: "nara::editor::work", Level::INFO,
            bytes = 6u64, revision = -1i64, window = true, runtime = 1.5f64,
            source = 3i64, outcome = canary.as_str(), raw = ?NeverFormat, "{canary}");
        tracing::info!(target: "nara::editor::work_private", secret = canary.as_str());
        tracing::event!(name: "C:\\private\\CAPTURE_SECRET_CANARY", target: "nara::editor::work", Level::INFO, task_id = 1u64);
        tracing::event!(name: "privacy_event", target: "nara::editor::work::CAPTURE_SECRET_CANARY", Level::INFO, task_id = 1u64);
    });
    let document = export(&capture);
    let json = serde_json::to_string(&document).unwrap();
    assert!(!json.contains("CAPTURE_SECRET_CANARY") && !json.contains("private"));
    let events = document["traceEvents"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert!(capture.report().rejected_fields >= 8);
    let event = &events[0];
    assert_eq!(event["args"]["bytes"], 6);
    assert_eq!(event["args"]["source"], 3);
    for name in ["revision", "window", "runtime", "outcome", "raw", "message"] {
        assert!(event["args"].get(name).is_none());
    }
    assert_eq!(events[1]["args"]["outcome"], "success");
}

#[test]
fn failure_is_latched_and_create_new_never_overwrites_existing_output() {
    let capture = capture(4, 1);
    let span = with_default(
        &capture.dispatch(),
        || tracing::info_span!(target: "nara::editor::work", "unfinished"),
    );
    capture.fail();
    capture.fail();
    with_default(
        &capture.dispatch(),
        || tracing::event!(name: "capture_tick", target: "nara::editor::work", Level::INFO, poll = 1u64),
    );
    drop(span);
    assert_eq!(capture.report().faults, 1);
    assert!(capture.report().incomplete > 0);
    let document = export(&capture);
    assert_eq!(document["traceEvents"], serde_json::json!([]));
    assert_eq!(document["nara_capture"]["active_spans"], 1);
    let path = output_path();
    fs::write(&path, b"original").unwrap();
    assert_eq!(
        capture.write_new(&path).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&path).unwrap(), b"original");
    fs::remove_file(path).unwrap();
}

#[test]
fn gpu_interval_uses_original_start_and_duration_and_rejects_invalid_time() {
    let before_capture = Instant::now();
    let capture = capture(4, 0);
    let identity = GpuIdentity {
        runtime: 1,
        app_frame: 2,
        render_frame: 3,
        window: 4,
        surface_generation: 5,
        backend_instance: 6,
        device_epoch: 7,
    };
    let app_started = Instant::now();
    std::thread::sleep(Duration::from_millis(4));
    capture.gpu_complete(app_started, Duration::from_millis(1), identity);
    capture.presented(app_started, Duration::from_micros(500), identity);
    capture.gpu_complete(before_capture, Duration::from_millis(1), identity);
    capture.gpu_complete(app_started, Duration::MAX, identity);
    let document = export(&capture);
    let event = &document["traceEvents"][0];
    assert_eq!(event["name"], "app_start_to_gpu_complete");
    assert_eq!(event["ph"], "X");
    assert!(
        event["ts"].as_f64().unwrap()
            <= app_started.duration_since(before_capture).as_nanos() as f64 / 1_000.0
    );
    assert_eq!(event["dur"], 1_000.0);
    assert_eq!(event["args"]["app_frame"], 2);
    assert_eq!(event["args"]["device_epoch"], 7);
    assert_eq!(
        (capture.report().events, capture.report().dropped_events),
        (2, 2)
    );
    assert!(capture.report().incomplete >= 2);
    let presented = &document["traceEvents"][1];
    assert_eq!(presented["name"], "app_start_to_presented");
    assert_eq!(presented["ts"], event["ts"]);
    assert_eq!(presented["dur"], 500.0);
    assert_eq!(presented["args"]["app_frame"], event["args"]["app_frame"]);
    assert_eq!(
        presented["args"]["render_frame"],
        event["args"]["render_frame"]
    );
    assert_ne!(presented["tid"], event["tid"]);
    assert!(
        document["gpu_interval_scope"]
            .as_str()
            .unwrap()
            .contains("not pure GPU execution")
    );
}

#[test]
fn crossing_lifetimes_use_distinct_bounded_viewer_tracks() {
    let capture = capture(4, 2);
    with_default(&capture.dispatch(), || {
        let first = tracing::info_span!(target: "nara::editor::work", "crossing_a");
        std::thread::sleep(Duration::from_millis(1));
        let second = tracing::info_span!(target: "nara::editor::work", "crossing_b");
        std::thread::sleep(Duration::from_millis(1));
        drop(first);
        std::thread::sleep(Duration::from_millis(1));
        drop(second);
    });
    let document = export(&capture);
    let events = document["traceEvents"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_ne!(events[0]["tid"], events[1]["tid"]);
    let first_start = events[0]["ts"].as_f64().unwrap();
    let second_start = events[1]["ts"].as_f64().unwrap();
    let first_end = first_start + events[0]["dur"].as_f64().unwrap();
    let second_end = second_start + events[1]["dur"].as_f64().unwrap();
    assert!(first_start < second_start && second_start < first_end && first_end < second_end);
    assert_eq!(capture.report().incomplete, 0);
}
