use std::{fs, path::Path, time::Instant};

use nara::{
    input::{ActionBinding, ActionId, ActionMapFileCandidate, GamepadButton, KeyCode},
    reflect::AuthoringSchemaBundle,
};
use serde_json::Value;

use super::super::SCENE;

pub const INPUT_MAP: &str = "input/starter.actions.json";
pub const PROOF_INPUT: &str = "NARA_PUSH_PROOF_INPUT";
pub const PROOF_TICKS: &str = "17";
pub const SCHEMA: &str = "schema/authoring.bundle.json";

pub fn install_puzzle(root: &Path) {
    assert!(!root.join("src/proof.rs").exists());
    for (path, bytes) in [
        (
            "src/gameplay.rs",
            include_bytes!("../game/gameplay.rs").as_slice(),
        ),
        (
            "src/proof.rs",
            include_bytes!("../game/proof.rs").as_slice(),
        ),
        (
            SCENE,
            include_bytes!("../game/startup.scene.json").as_slice(),
        ),
    ] {
        fs::write(root.join(path), bytes).unwrap();
    }

    let library = root.join("src/lib.rs");
    let generated = fs::read_to_string(&library).unwrap();
    fs::write(
        library,
        replace_once(
            &generated,
            ".add_plugin::<gameplay::GameplayPlugin>()",
            ".add_contribution(gameplay::contribution()?)",
        ),
    )
    .unwrap();

    let headless = root.join("src/bin/headless.rs");
    let generated = fs::read_to_string(&headless).unwrap();
    let extended = replace_once(
        &generated,
        "NonZeroU32::new(1)",
        &format!("NonZeroU32::new({PROOF_TICKS})"),
    );
    fs::write(
        headless,
        replace_once(
            &extended,
            "if outcome.fixed_tick == 1 =>",
            &format!("if outcome.fixed_tick == {PROOF_TICKS} =>"),
        ),
    )
    .unwrap();
}

pub fn assert_puzzle_schema(root: &Path) {
    let bundle =
        AuthoringSchemaBundle::from_json_bytes(&fs::read(root.join(SCHEMA)).unwrap()).unwrap();
    let owner = bundle
        .owners()
        .iter()
        .find(|owner| owner.owner_id().as_str() == "push-box.components")
        .expect("schema --write must capture the compiled game contribution");
    let components = owner.current().components();
    assert_eq!(components.len(), 2);
    for id in ["push-box.PressurePlate", "push-box.ExitDoor"] {
        assert!(
            components
                .iter()
                .any(|component| component.id().as_str() == id)
        );
    }
}

pub fn assert_default_directions(bytes: &[u8]) {
    let candidate = ActionMapFileCandidate::decode_json_bytes(bytes).unwrap();
    let bindings = candidate.definition().bindings();
    for (action, key, button) in [
        ("move.up", 'w', GamepadButton::DPadUp),
        ("move.left", 'a', GamepadButton::DPadLeft),
        ("move.down", 's', GamepadButton::DPadDown),
        ("move.right", 'd', GamepadButton::DPadRight),
    ] {
        let id = ActionId::new(action).unwrap();
        assert!(bindings.contains(&ActionBinding::key(id.clone(), KeyCode::Character(key))));
        assert!(bindings.contains(&ActionBinding::gamepad_button(id, button.code())));
    }
}

pub fn remove_keyboard_right(root: &Path) -> Instant {
    let path = root.join(INPUT_MAP);
    let bytes = fs::read(&path).unwrap();
    let before = ActionMapFileCandidate::decode_json_bytes(&bytes).unwrap();
    let right = ActionBinding::key(
        ActionId::new("move.right").unwrap(),
        KeyCode::Character('d'),
    );
    let mut edited: Value = serde_json::from_slice(&bytes).unwrap();
    let bindings = edited["payload"]["bindings"].as_array_mut().unwrap();
    let removed = serde_json::to_value(&right).unwrap();
    let original_count = bindings.len();
    bindings.retain(|binding| binding != &removed);
    assert_eq!(bindings.len() + 1, original_count);

    let edited = serde_json::to_vec_pretty(&edited).unwrap();
    let after = ActionMapFileCandidate::decode_json_bytes(&edited).unwrap();
    assert_eq!(after.product(), before.product());
    assert_eq!(
        after.definition().contexts(),
        before.definition().contexts()
    );
    let retained = before
        .definition()
        .bindings()
        .iter()
        .filter(|binding| *binding != &right);
    assert!(after.definition().bindings().iter().eq(retained));
    let mut right_bindings = after
        .definition()
        .bindings()
        .iter()
        .filter(|binding| binding.action == right.action && binding.context == right.context);
    assert_eq!(
        right_bindings.next(),
        Some(&ActionBinding::gamepad_button(
            right.action.clone(),
            GamepadButton::DPadRight.code()
        ))
    );
    assert_eq!(right_bindings.next(), None);
    let started = Instant::now();
    fs::write(path, edited).unwrap();
    started
}

pub fn observed_snapshots(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("push_proof tick="))
        .collect()
}

pub fn bind_keyboard_right_to_up(default_input: &[u8]) -> Vec<u8> {
    let mut document: Value = serde_json::from_slice(default_input).unwrap();
    let up = ActionBinding::key(ActionId::new("move.up").unwrap(), KeyCode::Character('d'));
    document["payload"]["bindings"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(up).unwrap());
    let bytes = serde_json::to_vec_pretty(&document).unwrap();
    ActionMapFileCandidate::decode_json_bytes(&bytes).unwrap();
    bytes
}

pub fn blocked_door_scene(saved: &[u8]) -> Vec<u8> {
    let mut document: Value = serde_json::from_slice(saved).unwrap();
    let plate = document["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == "pressure-plate")
        .unwrap();
    let column =
        &mut plate["components"]["push-box.PressurePlate"]["value"]["value"]["column"]["value"];
    assert_eq!(column.as_i64(), Some(3));
    *column = Value::from(5);
    serde_json::to_vec_pretty(&document).unwrap()
}

pub fn assert_snapshots(stdout: &str, opening_tick: Option<u32>, maximum_pushes: u32) {
    let pushes = [
        (1, 0),
        (2, 1),
        (3, 1),
        (4, 1),
        (6, 2),
        (8, 2),
        (10, 3),
        (11, 4),
        (12, 4),
        (14, 4),
        (16, 4),
    ];
    let expected = pushes.map(|(tick, pushes)| {
        let actor = pushes.min(maximum_pushes);
        let box_column = actor + 1;
        let door_open = opening_tick.is_some_and(|opening| tick >= opening);
        let won = actor == 4;
        format!("push_proof tick={tick} actor=({actor},0) box=({box_column},0) door_open={door_open} won={won}")
    });
    assert_eq!(
        observed_snapshots(stdout),
        expected.each_ref().map(String::as_str),
        "{stdout}"
    );
}

fn replace_once(source: &str, original: &str, replacement: &str) -> String {
    assert_eq!(source.matches(original).count(), 1, "expected {original:?}");
    source.replacen(original, replacement, 1)
}
