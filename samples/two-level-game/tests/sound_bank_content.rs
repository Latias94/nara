#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[path = "../../../tests/support/sound_bank_content.rs"]
mod support;

#[path = "support/resume_gameplay.rs"]
#[allow(dead_code)]
mod game;

#[test]
fn ordinary_source_import_and_tracked_build_observe_auxiliary_wav_changes() {
    support::source_and_build();
}

#[test]
fn processed_sound_bank_matches_source_and_rejects_foreign_payload_membership() {
    support::processed_and_membership();
}

#[test]
fn escaped_sound_bank_collections_survive_original_world_and_source_retirement() {
    support::world_retirement();
}

#[test]
fn escaped_sound_bank_collection_refunds_its_actual_budget_only_on_last_drop() {
    support::budget_retirement();
}

#[test]
fn a_retained_bank_allows_successive_real_scene_revisions_to_travel() {
    use nara::{
        asset::AssetRef,
        gameplay::{GameplayCommandDraft, GameplayCommandTypeId},
        project_host::ProjectDomainAssets,
    };
    use two_level_game::{
        levels::{LevelId, LevelKey},
        sound_bank::{SoundBankDomain, SoundCue, bank_product},
    };
    let project = game::ProjectCopy::new();
    let mut game = game::Game::from_project(project.root());
    let retained = game
        .session
        .observe(|world| world.resource::<ProjectDomainAssets>().clone())
        .unwrap();
    let selected = AssetRef::Product(bank_product());
    let expected = retained
        .collection::<SoundBankDomain>()
        .unwrap()
        .get(&selected)
        .unwrap();
    let original = [
        expected.samples(SoundCue::Pickup).to_vec(),
        expected.samples(SoundCue::Travel).to_vec(),
    ];
    for (index, level) in [LevelId::Cavern, LevelId::Meadow, LevelId::Cavern]
        .into_iter()
        .enumerate()
    {
        let path = project
            .root()
            .join("scenes")
            .join(level.scene_path().as_str());
        let mut scene: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let pickup = scene["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == "gem-far")
            .unwrap();
        pickup["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]["value"]
            ["x"]["value"] = serde_json::json!(81 + index);
        std::fs::write(&path, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();
        let before = game.snapshot();
        let after = game.command(GameplayCommandDraft::new(
            GameplayCommandTypeId::new(game::TRAVEL).unwrap(),
        ));
        assert_eq!(after.level, Some(LevelKey::main(level)));
        assert_ne!(after.current_instance, before.current_instance);
        assert_eq!(after.travel_count, index as u32 + 1);
        assert_eq!(after.score, 0);
        game.session
            .observe(|world| {
                let assets = world.resource::<ProjectDomainAssets>();
                assert_eq!(
                    assets
                        .collection::<SoundBankDomain>()
                        .unwrap()
                        .get(&selected)
                        .unwrap(),
                    expected
                );
            })
            .unwrap();
    }
    game.close();
    assert_eq!(expected.samples(SoundCue::Pickup), original[0]);
    assert_eq!(expected.samples(SoundCue::Travel), original[1]);
}
