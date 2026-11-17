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
        {
            let _preflight =
                tracing::info_span!(target: "nara::scene::work", "scene_unbound_preflight")
                    .entered();
            {
                let _receiving =
                    tracing::info_span!(target: "nara::scene::work", "scene_receiving_validation")
                        .entered();
            }
            let _component =
                tracing::info_span!(target: "nara::scene::work", "scene_component_preflight")
                    .entered();
        }
        {
            let _world =
                tracing::info_span!(target: "nara::scene::work", "scene_unbound_world_prepare")
                    .entered();
        }
        {
            let _receiving =
                tracing::info_span!(target: "nara::scene::work", "scene_receiving_validation")
                    .entered();
        }
        {
            let _component =
                tracing::info_span!(target: "nara::scene::work", "scene_component_preflight")
                    .entered();
        }
        let _materialize =
            tracing::info_span!(target: "nara::scene::work", "scene_unbound_materialize").entered();
    }
    let report = first.finish().unwrap();
    assert_eq!(report["phases"]["scene_publish"]["owner_thread_calls"], 1);
    assert_eq!(report["phases"]["scene_expand"]["worker_thread_calls"], 1);
    assert_eq!(
        report["phases"]["scene_structure"]["worker_thread_calls"],
        1
    );
    let unbound_phases = [
        "scene_unbound_preflight",
        "scene_receiving_validation",
        "scene_component_preflight",
        "scene_unbound_receiving_validation",
        "scene_unbound_component_preflight",
        "scene_unbound_world_prepare",
        "scene_unbound_materialize",
    ];
    for name in unbound_phases {
        assert_eq!(report["phases"][name]["owner_thread_calls"], 1);
        assert_eq!(report["phases"][name]["worker_thread_calls"], 0);
    }
    assert_eq!(report["total_calls"], 10);
    assert_eq!(report["retained_spans"], 0);
    let second = collector.begin().unwrap();
    {
        let _owner = tracing::info_span!(target: "nara::scene::work", "scene_bind").entered();
    }
    let report = second.finish().unwrap();
    assert_eq!(report["phases"]["scene_expand"]["calls"], 0);
    assert_eq!(report["phases"]["scene_bind"]["owner_thread_calls"], 1);
    for name in unbound_phases {
        assert_eq!(report["phases"][name]["calls"], 0);
    }
    assert_eq!(report["total_calls"], 1);
    let associated = collector.begin().unwrap();
    let parent = tracing::info_span!(target: "nara::scene::work", "scene_unbound_preflight");
    let bridge = tracing::info_span!(
        target: "nara::scene::work",
        parent: &parent,
        "scene_decode"
    );
    {
        let _ambient = tracing::info_span!(target: "nara::scene::work", "scene_decode").entered();
        let child = tracing::info_span!(
            target: "nara::scene::work",
            parent: &bridge,
            "scene_receiving_validation"
        );
        let _entered = child.enter();
    }
    drop(bridge);
    drop(parent);
    {
        let _ambient =
            tracing::info_span!(target: "nara::scene::work", "scene_unbound_preflight").entered();
        let child = tracing::info_span!(
            target: "nara::scene::work",
            parent: None,
            "scene_component_preflight"
        );
        let _entered = child.enter();
    }
    let report = associated.finish().unwrap();
    assert_eq!(
        report["phases"]["scene_unbound_receiving_validation"]["calls"],
        1
    );
    assert_eq!(report["phases"]["scene_receiving_validation"]["calls"], 0);
    assert_eq!(
        report["phases"]["scene_unbound_component_preflight"]["calls"],
        0
    );
    assert_eq!(report["phases"]["scene_component_preflight"]["calls"], 1);
    assert_eq!(report["total_calls"], 4);
    assert_eq!(report["retained_spans"], 0);
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
