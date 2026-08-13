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

    let identity = &capture.identity;
    assert!(identity.packet_primitives > 0);
    assert!(identity.packet_glyph_primitives > 0);
    assert_eq!(
        identity.packet_glyph_primitives,
        identity.glyph_page_references.len()
    );
    assert!(!identity.glyph_page_keys.is_empty());
    let packet = identity.packet;
    let durations = capture
        .durations_ns
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let glyph_page_keys = identity
        .glyph_page_keys
        .iter()
        .map(|key| {
            format!(
                "{{\"font_revision\":{},\"page_index\":{},\"generation\":{}}}",
                key.font_revision(),
                key.page_index(),
                key.generation(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        concat!(
            "{{\"schema\":\"nara-reference-game.ui-planner-performance-v2\",",
            "\"target\":{{\"window_id\":{},\"physical_size\":[{},{}]}},",
            "\"packet\":{{\"surface_id\":{},\"frame_revision\":{},",
            "\"source_revision\":{},\"slot_generation\":{},",
            "\"route_revision\":{},\"geometry_generation\":{},",
            "\"presentation_revision\":{},\"theme_revision\":{},",
            "\"font_revision\":{}}},\"warmup_samples\":{},",
            "\"measurement_samples\":{},",
            "\"durations_ns\":[{}],\"packet_primitives\":{},",
            "\"packet_glyph_primitives\":{},\"glyph_page_keys\":[{}]}}"
        ),
        identity.target_window_id,
        identity.physical_width,
        identity.physical_height,
        packet.surface_id().get(),
        packet.frame_revision().get(),
        packet.source_revision().get(),
        packet.slot_generation().get(),
        packet.route_revision().get(),
        packet.geometry_generation().get(),
        packet.presentation_revision().get(),
        packet.theme_revision().get(),
        packet.font_revision().get(),
        capture.warmup_samples,
        capture.durations_ns.len(),
        durations,
        identity.packet_primitives,
        identity.packet_glyph_primitives,
        glyph_page_keys,
    );
}
