#![cfg(feature = "scene-work-observation")]

#[path = "../src/bin/runtime_workload/work.rs"]
#[allow(dead_code)]
mod work;

#[test]
fn original_worker_entries_are_observed_and_consecutive_runs_keep_their_own_facts() {
    let collector = work::Collector::install().unwrap();
    let first = collector.begin().unwrap();
    {
        let _owner = tracing::info_span!(target: "nara::scene::work", "scene_publish").entered();
        std::thread::spawn(|| {
            let _worker =
                tracing::info_span!(target: "nara::scene::work", "scene_expand").entered();
            let _nested =
                tracing::info_span!(target: "nara::scene::work", "scene_structure").entered();
        })
        .join()
        .unwrap();
    }
    let report = first.finish().unwrap();
    assert_eq!(report["phases"]["scene_publish"]["owner_thread_calls"], 1);
    assert_eq!(report["phases"]["scene_expand"]["worker_thread_calls"], 1);
    assert_eq!(
        report["phases"]["scene_structure"]["worker_thread_calls"],
        1
    );
    assert_eq!(report["total_calls"], 3);
    assert_eq!(report["retained_spans"], 0);
    let second = collector.begin().unwrap();
    {
        let _owner = tracing::info_span!(target: "nara::scene::work", "scene_bind").entered();
    }
    let report = second.finish().unwrap();
    assert_eq!(report["phases"]["scene_expand"]["calls"], 0);
    assert_eq!(report["phases"]["scene_bind"]["owner_thread_calls"], 1);
    assert_eq!(report["total_calls"], 1);
    retained_or_reentrant_spans_refuse_an_incomplete_observation(&collector);
    let active = collector.begin().unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let _span = tracing::info_span!(target: "nara::scene::work", "scene_expand").entered();
        entered_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let refused = active.finish().is_err();
    let next = collector.begin().unwrap();
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    assert!(refused);
    assert_eq!(next.finish().unwrap()["total_calls"], 0);
}

fn retained_or_reentrant_spans_refuse_an_incomplete_observation(collector: &work::Collector) {
    let first = collector.begin().unwrap();
    let retained = tracing::info_span!(target: "nara::scene::work", "scene_expand");
    assert!(first.finish().is_err());
    let second = collector.begin().unwrap();
    {
        let _old = retained.enter();
    }
    drop(retained);
    assert_eq!(second.finish().unwrap()["total_calls"], 0);
    let third = collector.begin().unwrap();
    {
        let span = tracing::info_span!(target: "nara::scene::work", "scene_bind");
        let _first = span.enter();
        let _again = span.enter();
    }
    assert!(third.finish().is_err());
}
