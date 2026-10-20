use super::*;

#[test]
fn consecutive_retries_publish_the_current_scene_receipt_used_by_the_wave() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(run_initialization_audit_plugin());
    drive_adapter_fixed(&mut runtime);
    let mut previous = current_wave_receipt(&runtime, 1);

    for generation in 2..=3 {
        drive_adapter_to_terminal(&mut runtime);
        submit_retry_for_next_tick(&mut runtime, generation);
        drive_adapter_to_run_generation(&mut runtime, generation);

        let current = current_wave_receipt(&runtime, generation);
        assert_ne!(current.instance_id(), previous.instance_id());
        assert_ne!(current, previous);
        let previous_player = previous
            .entity_ids()
            .iter()
            .find(|id| id.as_str() == "player")
            .unwrap();
        assert!(matches!(
            previous.resolve(runtime.world(), previous_player),
            EntityLookup::Tombstoned(Some(tombstone))
                if tombstone.cause() == TombstoneCause::Replaced
        ));
        assert_eq!(
            runtime
                .world()
                .resource::<WaveRetryStatus>()
                .applied_generation(),
            Some(generation)
        );
        previous = current;
    }

    stop_adapter_retry_runtime(runtime);
}

fn current_wave_receipt(runtime: &RuntimeInstance, generation: u64) -> SpawnedSceneInstance {
    let world = runtime.world();
    let receipt = world
        .resource::<RunInitializationAudit>()
        .scene_receipts
        .get(&generation)
        .expect("the current Scene view must publish a receipt for each run")
        .clone();
    assert_eq!(world.resource::<WaveRunGeneration>().get(), generation);
    let (player, _) = player_scene_identity(world);
    let source = world.get::<SceneEntitySource>(player).unwrap();
    assert_eq!(source.instance_id, receipt.instance_id());
    assert_eq!(
        receipt.resolve(world, &source.entity_id),
        EntityLookup::Resolved(player)
    );
    assert_eq!(world.resource::<WaveSnapshot>().run_generation, generation);
    assert_eq!(
        world.resource::<WaveSnapshot>().player.id,
        source.entity_id.as_str()
    );
    receipt
}
