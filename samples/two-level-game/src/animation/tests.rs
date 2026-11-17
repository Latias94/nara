use super::*;

#[test]
fn held_direction_truth_table_covers_opposition_and_diagonals() {
    for (held, expected) in [
        ([false, false, false, false], "idle"),
        ([true, false, false, false], "walk"),
        ([false, true, false, false], "walk"),
        ([true, true, false, false], "idle"),
        ([false, false, true, false], "walk"),
        ([true, false, true, false], "walk"),
        ([false, true, true, false], "walk"),
        ([true, true, true, false], "walk"),
        ([false, false, false, true], "walk"),
        ([true, false, false, true], "walk"),
        ([false, true, false, true], "walk"),
        ([true, true, false, true], "walk"),
        ([false, false, true, true], "idle"),
        ([true, false, true, true], "walk"),
        ([false, true, true, true], "walk"),
        ([true, true, true, true], "idle"),
    ] {
        assert_eq!(clip_name(held), expected, "held={held:?}");
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
