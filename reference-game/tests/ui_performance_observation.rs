#[allow(unused_imports)]
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;
#[allow(dead_code, unused_imports)]
#[path = "support/ui_product_fixture.rs"]
mod ui_product_fixture;

use ui_product_fixture::capture_planner_performance;

#[test]
fn warmed_product_frame_planner_emits_bounded_raw_observations() {
    let capture = capture_planner_performance();

    assert_eq!(capture.warmup_samples, 4);
    assert_eq!(capture.durations_ns.len(), 16);
    assert!(capture.durations_ns.iter().all(|duration| *duration > 0));
    assert!(capture.packet_primitives > 0);
    assert!(capture.glyph_pages > 0);

    let durations = capture
        .durations_ns
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    println!(
        concat!(
            "{{\"schema\":\"nara-reference-game.ui-planner-performance-v1\",",
            "\"target\":[1280,720],\"warmup_samples\":{},",
            "\"durations_ns\":[{}],\"packet_primitives\":{},",
            "\"glyph_pages\":{}}}"
        ),
        capture.warmup_samples,
        durations,
        capture.packet_primitives,
        capture.glyph_pages,
    );
}
