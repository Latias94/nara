use serde_json::{Value, json};
use two_level_game::levels::{AUTHORED_LEVEL_MEMBERS, LevelId, WALL_ENTITY_IDS};

pub fn scene(observed: &Value) {
    let receipt = &observed["receipt"];
    assert_eq!(receipt["entity_count"], AUTHORED_LEVEL_MEMBERS);
    assert_eq!(receipt["truncated"], false);
    let mut ids: Vec<String> = serde_json::from_value(receipt["entity_ids"].clone()).unwrap();
    ids.sort();
    let mut expected = [
        "camera",
        "checkpoint",
        "explorer",
        "gem-far",
        "gem-near",
        "portal",
    ]
    .into_iter()
    .chain(WALL_ENTITY_IDS)
    .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(ids, expected);
    let level: LevelId = serde_json::from_value(observed["level"].clone()).unwrap();
    let inner_x = match level {
        LevelId::Meadow => 0.0_f32,
        LevelId::Cavern => 20.0,
    };
    let expected = WALL_ENTITY_IDS
        .into_iter()
        .zip([
            ([-294.0, 0.0], [5.0, 164.0]),
            ([294.0, 0.0], [5.0, 164.0]),
            ([0.0, 164.0], [299.0, 5.0]),
            ([0.0, -164.0], [299.0, 5.0]),
            ([inner_x, -12.0], [6.0, 36.0]),
        ])
        .map(|(id, (translation, half))| {
            json!({
                "id": id,
                "root": true,
                "translation": translation,
                "rotation": 0.0_f32,
                "scale": [1.0_f32, 1.0],
                "box": {"center": [0.0_f32, 0.0], "half_width": half[0], "half_height": half[1]},
                "sprite_size": [half[0] * 2.0, half[1] * 2.0],
                "sprite_anchor": [0.0_f32, 0.0],
                "tint": [0.18_f32, 0.22, 0.28, 1.0],
                "textured": false,
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(observed["wall_proofs"], json!(expected));
}
