use super::*;

#[test]
fn walking_uses_elapsed_nanoseconds_at_exact_frame_boundaries() {
    for (elapsed_nanos, expected) in [
        (0, ExplorerFrame::WalkOne),
        (99_999_999, ExplorerFrame::WalkOne),
        (100_000_000, ExplorerFrame::WalkTwo),
        (199_999_999, ExplorerFrame::WalkTwo),
        (200_000_000, ExplorerFrame::WalkThree),
        (299_999_999, ExplorerFrame::WalkThree),
        (300_000_000, ExplorerFrame::WalkOne),
        (u64::MAX, ExplorerFrame::WalkTwo),
    ] {
        let clock = GameClock {
            elapsed_nanos,
            ..GameClock::default()
        };
        assert_eq!(
            explorer_frame(clock, [false, true, false, false]),
            expected,
            "elapsed_nanos={elapsed_nanos}",
        );
        assert_eq!(explorer_frame(clock, [false; 4]), ExplorerFrame::Idle);
    }
}

#[test]
fn held_direction_truth_table_covers_opposition_and_diagonals() {
    let clock = GameClock {
        elapsed_nanos: 100_000_000,
        ..GameClock::default()
    };
    for (held, expected) in [
        ([false, false, false, false], ExplorerFrame::Idle),
        ([true, false, false, false], ExplorerFrame::WalkTwo),
        ([false, true, false, false], ExplorerFrame::WalkTwo),
        ([true, true, false, false], ExplorerFrame::Idle),
        ([false, false, true, false], ExplorerFrame::WalkTwo),
        ([true, false, true, false], ExplorerFrame::WalkTwo),
        ([false, true, true, false], ExplorerFrame::WalkTwo),
        ([true, true, true, false], ExplorerFrame::WalkTwo),
        ([false, false, false, true], ExplorerFrame::WalkTwo),
        ([true, false, false, true], ExplorerFrame::WalkTwo),
        ([false, true, false, true], ExplorerFrame::WalkTwo),
        ([true, true, false, true], ExplorerFrame::WalkTwo),
        ([false, false, true, true], ExplorerFrame::Idle),
        ([true, false, true, true], ExplorerFrame::WalkTwo),
        ([false, true, true, true], ExplorerFrame::WalkTwo),
        ([true, true, true, true], ExplorerFrame::Idle),
    ] {
        assert_eq!(explorer_frame(clock, held), expected, "held={held:?}");
    }
}

#[test]
fn frames_cover_the_exact_four_atlas_quarters() {
    for (frame, index, min_x, max_x) in [
        (ExplorerFrame::Idle, 0, 0.0, 0.25),
        (ExplorerFrame::WalkOne, 1, 0.25, 0.5),
        (ExplorerFrame::WalkTwo, 2, 0.5, 0.75),
        (ExplorerFrame::WalkThree, 3, 0.75, 1.0),
    ] {
        assert_eq!(frame.index(), index);
        let region = frame.texture_region();
        assert_eq!(region.min.to_array(), [min_x, 0.0]);
        assert_eq!(region.size.to_array(), [0.25, 1.0]);
        assert_eq!(region.max().to_array(), [max_x, 1.0]);
    }
}

#[test]
fn complete_atlas_product_matches_the_exact_committed_metadata() {
    let metadata: serde_json::Value =
        serde_json::from_str(include_str!("../../assets/textures/explorer.png.meta")).unwrap();
    assert_eq!(
        metadata,
        serde_json::json!({
            "kind": "asset_meta",
            "format_version": 1,
            "engine_min_version": "0.1.0",
            "generator": {"name": "nara", "version": "0.1.0"},
            "payload": {
                "asset": {
                    "stable_id": "2eece9cd-6332-4868-b0d2-289879df9de8",
                    "path": "textures/explorer.png",
                    "source_kind": "image"
                },
                "settings": {"color_space": "srgb"}
            }
        }),
    );
    assert_eq!(metadata["payload"]["asset"]["path"], ATLAS_PATH);
    assert_eq!(metadata["payload"]["asset"]["stable_id"], ATLAS_SOURCE_ID);
    let metadata_product: AssetProductRef = serde_json::from_value(serde_json::json!({
        "source": metadata["payload"]["asset"]["stable_id"],
        "product": {"kind": "primary"}
    }))
    .unwrap();
    assert_eq!(atlas_product(), metadata_product);
}
