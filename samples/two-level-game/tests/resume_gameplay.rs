#![forbid(unsafe_code)]

#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    app::{
        FixedTime, PluginCategory, PluginDeclaration, PluginId, RuntimeState,
        RuntimeTimeContinuationError, RuntimeTimeContinuationRejection,
    },
    asset::{
        AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
        AssetSourceKind, Assets, SourceAssetImporter, StableAssetId, allocate_anonymous_handle,
    },
    core::ByteLimit,
    gameplay::GameplayCommandDraft,
    material::{
        AlphaMode2d, Material2d, Material2dAsset, Material2dDeclaration, Material2dDomain,
        Material2dSource, MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue,
        MaterialUniformField,
    },
    prelude::{App, Plugin, PluginError},
    project_host::{HeadlessSession, HeadlessSessionClose, HeadlessSessionError},
};
use support::{
    DEADLINE, Game, INSTANCE, MARKER, MAX_WALK_STEPS, ProjectCopy, TRAVEL, action, movement,
};
use two_level_game::{
    GamePhase, GameResumeError, RunOptions, Scenario, capture_resume, configured_recipe,
    levels::{LevelId, LevelKey},
    product_recipe, request_resume,
    resume::{
        CURRENT_FORMAT_VERSION, GAME_COMPATIBILITY, ResumeError, ResumeState, decode_resume,
        encode_resume,
    },
};

fn collect_near_pickup(game: &mut Game, score: u32) {
    let deadline = Instant::now() + DEADLINE;
    game.command(movement(1, true));
    for _ in 0..MAX_WALK_STEPS {
        if game.snapshot().score == score {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "walking to the authored pickup timed out"
        );
        assert!(game.snapshot().score < score);
        game.step();
    }
    let snapshot = game.snapshot();
    assert_eq!(snapshot.score, score, "{snapshot:?}");
    assert_eq!(snapshot.visible_pickups, 1);
    assert_eq!(snapshot.held, [false, true, false, false]);
}

fn meaningful_game(game: &mut Game) -> ResumeState {
    let anchor = game.snapshot().player.to_array();
    assert_eq!(game.snapshot().level, Some(LevelKey::main(LevelId::Meadow)));
    collect_near_pickup(game, 10);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 1);
    let cavern = game.command(action(TRAVEL));
    assert_eq!(cavern.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_eq!(cavern.held, [false; 4]);
    assert_eq!(cavern.visible_pickups, 2);
    collect_near_pickup(game, 20);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 2);
    let second = LevelKey::new(LevelId::Cavern, 1).unwrap();
    assert_eq!(game.command(action(INSTANCE)).level, Some(second));
    collect_near_pickup(game, 30);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 3);

    let state = game.capture();
    assert_ne!(state.player.translation, anchor);
    assert_eq!(state.held, [false, true, false, false]);
    assert_eq!(state.progress.location.level, second);
    assert_eq!(state.progress.score, 30);
    assert_eq!(state.progress.collected.len(), 3);
    assert_eq!(state.progress.markers.len(), 3);
    assert_eq!(state.progress.next_marker_id.get(), 4);
    for (index, level) in [
        LevelKey::main(LevelId::Meadow),
        LevelKey::main(LevelId::Cavern),
        second,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            state
                .progress
                .collected
                .iter()
                .any(|pickup| pickup.level == level)
        );
        assert!(
            state
                .progress
                .markers
                .iter()
                .any(|marker| { marker.level == level && marker.id.get() == index as u64 + 1 })
        );
    }
    assert_eq!(state.compatibility.levels.len(), 2);
    assert_eq!(state.counters.travel_count, 2);
    assert_eq!(state.counters.restores, 0);
    assert!(state.clock.ticks > 0);
    assert_eq!(
        state.clock.elapsed_nanos,
        state.clock.ticks * state.clock.timestep_nanos
    );
    state
}

fn restore_candidate(original: &Game, candidate: &mut Game, state: &ResumeState) {
    let retained = original.frozen();
    let fresh = candidate.frozen();
    assert_eq!(fresh.tick, 0);
    assert_ne!(retained.generation, fresh.generation);
    request_resume(&mut candidate.session, state).expect("public reconstruction request");
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(
        candidate.tick(),
        1,
        "only the bounded resume command may execute"
    );
    assert!(candidate.snapshot().restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(_))
    ));

    let deadline = Instant::now() + DEADLINE;
    while candidate.snapshot().restoring {
        assert_eq!(
            original.frozen(),
            retained,
            "incumbent changed during reconstruction"
        );
        assert!(
            candidate.snapshot().error.is_none(),
            "{:?}",
            candidate.snapshot()
        );
        assert!(
            Instant::now() < deadline,
            "candidate reconstruction timed out: {:?}",
            candidate.snapshot()
        );
        candidate.maintenance();
        std::thread::sleep(Duration::from_millis(1));
    }
    let reconstructed = candidate.settle();
    assert_eq!(reconstructed.phase, GamePhase::Running);
    assert!(reconstructed.error.is_none());
    assert_eq!(candidate.tick(), 1);
    assert_eq!(
        candidate.capture(),
        *state,
        "all semantic state must match before continuation"
    );
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(state).unwrap()
    );
    let (elapsed, timestep, remainder, debt) = candidate
        .session
        .observe(|world| {
            let time = world.resource::<FixedTime>();
            (
                time.elapsed(),
                time.timestep(),
                time.remainder(),
                time.debt(),
            )
        })
        .unwrap();
    assert_eq!(elapsed, timestep, "the Runtime clock must remain fresh");
    let pending = Duration::from_nanos(state.runtime_time.pending_nanos);
    let expected_remainder =
        Duration::from_nanos(state.runtime_time.pending_nanos % state.runtime_time.timestep_nanos);
    assert_eq!(remainder, expected_remainder);
    assert_eq!(debt, pending - expected_remainder);
    assert_ne!(candidate.frozen().domain, retained.domain);
    assert_ne!(
        (candidate.frozen().domain, reconstructed.current_instance),
        (retained.domain, Some(retained.instance)),
    );
    assert_ne!(reconstructed.current_instance, Some(fresh.instance));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert_eq!(original.frozen(), retained);
}

fn compare_games(original: &Game, candidate: &Game) -> ResumeState {
    let state = original.capture();
    assert_eq!(candidate.capture(), state);
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let a = original.snapshot();
    let b = candidate.snapshot();
    assert_eq!(a.visible_pickups, b.visible_pickups);
    assert_eq!(a.markers, b.markers);
    assert_eq!(a.scene_members, b.scene_members);
    assert_ne!(a.current_instance, b.current_instance);
    assert_ne!(
        original.session.runtime_generation(),
        candidate.session.runtime_generation()
    );
    state
}

fn command_pair(
    original: &mut Game,
    candidate: &mut Game,
    command: GameplayCommandDraft,
) -> ResumeState {
    original.command(command.clone());
    candidate.command(command);
    compare_games(original, candidate)
}

fn step_pair(original: &mut Game, candidate: &mut Game) -> ResumeState {
    original.step();
    candidate.step();
    compare_games(original, candidate)
}

fn reject_candidate(original: &Game, candidate: &mut Game, state: &ResumeState) -> String {
    let retained = original.frozen();
    let generation = candidate.session.runtime_generation();
    let reason = match request_resume(&mut candidate.session, state) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        Err(error) => panic!("expected a game compatibility refusal, got {error:?}"),
        Ok(()) => {
            let deadline = Instant::now() + DEADLINE;
            loop {
                assert_eq!(original.frozen(), retained);
                let snapshot = candidate.snapshot();
                if let Some(error) = &snapshot.error {
                    break error.clone();
                }
                assert!(
                    snapshot.restoring,
                    "incompatible state was accepted: {snapshot:?}"
                );
                assert!(
                    Instant::now() < deadline,
                    "incompatible candidate did not finish: {snapshot:?}"
                );
                candidate.maintenance();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(candidate.session.runtime_generation(), generation);
    assert!(
        !candidate.snapshot().restoring,
        "failed reconstruction must clear pending state"
    );
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(_))
    ));
    assert_eq!(original.frozen(), retained);
    reason
}

#[test]
fn startup_first_travel_and_returned_meadow_share_runtime_content_compatibility() {
    let mut original = Game::source();
    for expected in [LevelId::Meadow, LevelId::Cavern, LevelId::Meadow] {
        assert_eq!(original.snapshot().level, Some(LevelKey::main(expected)));
        let state = original.capture();
        let mut candidate = Game::source();
        restore_candidate(&original, &mut candidate, &state);
        assert_eq!(candidate.capture(), state);
        candidate.close();
        if state.counters.travel_count < 2 {
            original.command(action(TRAVEL));
        }
    }
    original.close();
}

#[test]
fn public_resume_reconstructs_live_three_instance_state_and_continues_every_semantic_step() {
    let mut original = Game::source();
    let captured = meaningful_game(&mut original);
    let bytes = encode_resume(&captured).unwrap();
    let decoded = decode_resume(&bytes).unwrap();
    assert_eq!(decoded, captured);
    assert_eq!(encode_resume(&decoded).unwrap(), bytes);
    let mut reordered = captured.clone();
    reordered.progress.collected.reverse();
    reordered.progress.markers.reverse();
    reordered.compatibility.levels.reverse();
    assert_eq!(encode_resume(&reordered).unwrap(), bytes);

    let mut candidate = Game::source();
    restore_candidate(&original, &mut candidate, &decoded);
    let tick_offset = original.tick() - candidate.tick();
    assert!(tick_offset > 0, "engine time must remain fresh");
    let resumed = step_pair(&mut original, &mut candidate);
    assert!(resumed.player.translation[0] > captured.player.translation[0]);
    assert_eq!(
        resumed.held, captured.held,
        "held input must continue without resubmission"
    );
    assert_eq!(resumed.clock.ticks, captured.clock.ticks + 1);
    let released = command_pair(&mut original, &mut candidate, movement(1, false));
    assert_eq!(released.player, resumed.player);
    assert_eq!(released.held, [false; 4]);

    command_pair(&mut original, &mut candidate, movement(2, true));
    let deadline = Instant::now() + DEADLINE;
    for _ in 0..MAX_WALK_STEPS {
        if original.snapshot().player.y >= 40.0 {
            break;
        }
        assert!(Instant::now() < deadline);
        step_pair(&mut original, &mut candidate);
    }
    assert!((40.0..=44.0).contains(&original.snapshot().player.y));
    command_pair(&mut original, &mut candidate, movement(2, false));
    command_pair(&mut original, &mut candidate, movement(1, true));
    let deadline = Instant::now() + DEADLINE;
    for _ in 0..MAX_WALK_STEPS {
        if original.snapshot().score == 40 {
            break;
        }
        assert!(Instant::now() < deadline);
        step_pair(&mut original, &mut candidate);
    }
    assert_eq!(
        original.snapshot().score,
        40,
        "fresh continuation must collect gem-far"
    );
    assert_eq!(original.snapshot().visible_pickups, 0);
    command_pair(&mut original, &mut candidate, movement(1, false));
    let marked = command_pair(&mut original, &mut candidate, action(MARKER));
    assert_eq!(marked.progress.markers.last().unwrap().id.get(), 4);
    assert_eq!(marked.progress.next_marker_id.get(), 5);

    let main_cavern = command_pair(&mut original, &mut candidate, action(INSTANCE));
    assert_eq!(
        main_cavern.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [2]
    );
    let meadow = command_pair(&mut original, &mut candidate, action(TRAVEL));
    assert_eq!(
        meadow.progress.location.level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [1]
    );
    let meadow_marker = command_pair(&mut original, &mut candidate, action(MARKER));
    assert_eq!(meadow_marker.progress.markers.last().unwrap().id.get(), 5);
    command_pair(&mut original, &mut candidate, action(TRAVEL));
    let revisited = command_pair(&mut original, &mut candidate, action(INSTANCE));
    assert_eq!(
        revisited.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(original.snapshot().visible_pickups, 0);
    assert_eq!(revisited.progress.score, 40);
    assert_eq!(revisited.counters.travel_count, 6);
    assert_eq!(original.tick() - candidate.tick(), tick_offset);
    candidate.close();
    original.close();
}

#[test]
fn capture_refuses_future_commands_without_consuming_the_original_queue() {
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let tick = original.tick();
    let before = original.snapshot();
    let prior_queue = original.queue();
    original.submit_at(tick + 3, action(MARKER));
    let pending = original.queue();
    assert_eq!(pending.accepted, prior_queue.accepted + 1);
    assert_eq!(pending.pending_commands, 1);
    assert_eq!(pending.furthest_pending_tick, Some(tick + 3));
    for _ in 0..2 {
        assert!(matches!(
            capture_resume(&original.session),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
        ));
        assert_eq!(original.queue(), pending);
        assert_eq!(original.tick(), tick);
        assert_eq!(original.snapshot().player, before.player);
        assert_eq!(original.snapshot().logical_tick, before.logical_tick);
        assert_eq!(original.snapshot().markers, before.markers);
        original.maintenance();
    }
    for expected in [tick + 1, tick + 2] {
        assert_eq!(original.session.step().unwrap(), None);
        assert_eq!(original.tick(), expected);
        assert_eq!(original.queue().pending_commands, 1);
        assert_eq!(original.queue().acknowledged, prior_queue.acknowledged);
        assert!(matches!(
            capture_resume(&original.session),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
        ));
    }
    original.step();
    let continued = original.capture();
    assert_eq!(original.tick(), tick + 3);
    assert_eq!(original.queue().pending_commands, 0);
    assert_eq!(original.queue().retained_commands, 0);
    assert_eq!(original.queue().acknowledged, prior_queue.acknowledged + 1);
    assert_eq!(continued.clock.ticks, state.clock.ticks + 3);
    assert_eq!(
        continued.progress.markers.len(),
        state.progress.markers.len() + 1
    );
    assert_eq!(
        continued.progress.markers.last().unwrap().id,
        state.progress.next_marker_id
    );
    original.close();
}

#[test]
fn incompatible_wire_schema_and_configuration_preserve_the_paused_original_byte_exactly() {
    let project = ProjectCopy::new();
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();

    for (field, version) in [
        ("format_version", CURRENT_FORMAT_VERSION + 1),
        ("game_compatibility", GAME_COMPATIBILITY + 1),
    ] {
        let mut candidate = Game::source();
        let fresh = candidate.frozen();
        let mut wire: serde_json::Value = serde_json::from_slice(&retained.bytes).unwrap();
        wire[field] = serde_json::json!(version);
        let refused = decode_resume(&serde_json::to_vec(&wire).unwrap()).unwrap_err();
        match (field, refused) {
            ("format_version", ResumeError::UnsupportedVersion(actual))
            | ("game_compatibility", ResumeError::UnsupportedGameCompatibility(actual)) => {
                assert_eq!(actual, version)
            }
            (_, error) => panic!("wrong format refusal: {error:?}"),
        }
        assert_eq!(
            candidate.frozen(),
            fresh,
            "invalid wire must not reach a candidate"
        );
        assert_eq!(original.frozen(), retained);
        candidate.close();
    }
    for schema in [true, false] {
        let mut incompatible = state.clone();
        if schema {
            incompatible.compatibility.schema[0] ^= 1;
        } else {
            incompatible.compatibility.configuration[0] ^= 1;
        }
        incompatible.validate().unwrap();
        let mut candidate = Game::source();
        let reason = reject_candidate(&original, &mut candidate, &incompatible);
        assert!(
            reason.contains(if schema { "Schema" } else { "configuration" }),
            "{reason}"
        );
        candidate.close();
        assert_eq!(original.frozen(), retained);
    }
    project.change_fixed_timestep();
    let mut configured_candidate = Game::from_project(project.root());
    let configured = configured_candidate.capture();
    assert_eq!(configured.compatibility.schema, state.compatibility.schema);
    assert_ne!(configured.clock.timestep_nanos, state.clock.timestep_nanos);
    assert_ne!(
        configured.compatibility.configuration,
        state.compatibility.configuration
    );
    let fresh = configured_candidate.frozen();
    assert!(matches!(
        request_resume(&mut configured_candidate.session, &state),
        Err(GameResumeError::Session(
            HeadlessSessionError::TimeContinuation(RuntimeTimeContinuationError::Rejected(
                RuntimeTimeContinuationRejection::PolicyMismatch
            ))
        ))
    ));
    assert_eq!(configured_candidate.frozen(), fresh);
    configured_candidate.close();
    assert_eq!(original.frozen(), retained);

    let continued = original.command(movement(1, false));
    assert_eq!(continued.logical_tick, state.clock.ticks + 1);
    assert_eq!(original.capture().progress, state.progress);
    original.close();
}

#[test]
fn changed_cavern_content_in_a_verified_copy_refuses_resume_and_leaves_original_playable() {
    let project = ProjectCopy::new();
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/cavern.scene.json");
    let source_bytes = fs::read(&source_path).unwrap();
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();

    let mut unchanged = Game::from_project(project.root());
    let unchanged_state = unchanged.capture();
    assert_eq!(unchanged_state.compatibility.levels.len(), 1);
    assert!(
        unchanged_state
            .compatibility
            .levels
            .iter()
            .all(|stamp| { state.compatibility.levels.contains(stamp) })
    );
    restore_candidate(&original, &mut unchanged, &state);
    unchanged.close();
    project.change_cavern_pickup();
    assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
    assert_ne!(
        fs::read(project.root().join("scenes/cavern.scene.json")).unwrap(),
        source_bytes
    );
    let mut candidate = Game::from_project(project.root());
    let fresh = candidate.capture();
    assert_eq!(fresh.compatibility.schema, state.compatibility.schema);
    assert_eq!(
        fresh.compatibility.configuration,
        state.compatibility.configuration
    );
    let reason = reject_candidate(&original, &mut candidate, &state);
    assert!(reason.contains("content"), "{reason}");
    assert_eq!(original.frozen(), retained);
    let retried = candidate.command(action("two-level-game.retry"));
    assert_eq!(retried.phase, GamePhase::Running);
    assert_eq!(retried.restarts, 1);
    assert!(retried.error.is_none());
    assert!(!retried.restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("rejected")
    ));
    assert_eq!(original.frozen(), retained);
    let moved = candidate.command(movement(1, true));
    assert!(moved.player.x > retried.player.x);
    assert_eq!(moved.held, [false, true, false, false]);
    assert_eq!(moved.phase, GamePhase::Running);
    assert!(moved.error.is_none());
    assert!(!moved.restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("rejected")
    ));
    assert_eq!(original.frozen(), retained);
    candidate.close();
    assert_eq!(original.frozen(), retained);
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
    let continued = original.command(action(MARKER));
    assert_eq!(
        continued
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(original.capture().progress.score, state.progress.score);
    original.close();
}

#[test]
fn resume_requires_a_fresh_candidate_with_an_empty_original_command_queue() {
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();
    let mut candidate = Game::source();
    candidate.submit(movement(1, true));
    let pending = candidate.queue();
    assert!(matches!(
        request_resume(&mut candidate.session, &state),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
    ));
    assert_eq!(candidate.tick(), 0);
    assert_eq!(candidate.queue(), pending);
    assert_eq!(candidate.snapshot().held, [false; 4]);
    candidate.step();
    assert_eq!(candidate.capture().held, [false, true, false, false]);
    let nonfresh = candidate.frozen();
    assert!(matches!(
        request_resume(&mut candidate.session, &state),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("fresh")
    ));
    assert_eq!(candidate.frozen(), nonfresh);
    assert_eq!(original.frozen(), retained);
    candidate.close();
    original.command(movement(1, false));
    assert_eq!(original.capture().clock.ticks, state.clock.ticks + 1);
    original.close();
}

#[derive(Default)]
struct UnknownResumeService;

impl Plugin for UnknownResumeService {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-unsupported-resume-service"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn resume_test_material() -> Material2dAsset {
    let declaration = Material2dDeclaration::new(
        "two-level-game.test-material",
        1,
        AlphaMode2d::Blend,
        MaterialParameterLayout::new(
            vec![
                MaterialUniformField::new("tint", "tint", MaterialParameterKind::Vec4, 0).unwrap(),
            ],
            16,
        )
        .unwrap(),
        Vec::new(),
    )
    .unwrap();
    let source = Material2dSource::new(
        declaration,
        vec![AssetPath::new("resume-test.wgsl").unwrap()],
        BTreeMap::from([("tint".to_owned(), MaterialParameterValue::Vec4([1.0; 4]))]),
        BTreeMap::new(),
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("84d00000-0000-4000-8000-000000000001").unwrap(),
            AssetPath::new("resume-test.material").unwrap(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(metadata.as_bytes()).unwrap();
    let shader = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.tint;
}
"#;
    let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, Infallible> {
        Ok(Arc::from(shader.as_bytes()))
    };
    Material2dDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        source.encode().unwrap().into(),
        &mut reader,
    ))
    .unwrap()
}

#[derive(Default)]
struct MaterialResumeState<const COMPONENT: bool>;

impl<const COMPONENT: bool> Plugin for MaterialResumeState<COMPONENT> {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.desktop-capture"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::material::MATERIAL_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let material = resume_test_material();
        let handle = allocate_anonymous_handle().unwrap();
        let world = app.world_mut()?;
        if COMPONENT {
            world.spawn(
                Material2d::new(
                    handle,
                    material.generation().declaration(),
                    material.default_parameters().clone(),
                    BTreeMap::new(),
                )
                .unwrap(),
            );
        } else {
            world
                .resource_mut::<Assets<Material2dAsset>>()
                .insert(handle, material);
        }
        Ok(())
    }
}

#[test]
fn capture_refuses_material_assets_and_components_outside_resume_state() {
    for (recipe, component, reason) in [
        (
            product_recipe()
                .add_plugin::<MaterialResumeState<false>>()
                .unwrap(),
            false,
            "material assets",
        ),
        (
            product_recipe()
                .add_plugin::<MaterialResumeState<true>>()
                .unwrap(),
            true,
            "material components",
        ),
    ] {
        let mut game = Game::with_recipe(recipe);
        let before = game.snapshot();
        let tick = game.tick();
        let queue = game.queue();
        for _ in 0..2 {
            game.session
                .observe(|world| {
                    assert_eq!(
                        world.resource::<Assets<Material2dAsset>>().is_empty(),
                        component
                    );
                    assert_eq!(
                        world
                            .iter_entities()
                            .any(|entity| entity.contains::<Material2d>()),
                        component
                    );
                })
                .unwrap();
            assert!(matches!(
                capture_resume(&game.session),
                Err(GameResumeError::Ineligible(actual)) if actual.contains(reason)
            ));
            assert_eq!(game.tick(), tick);
            assert_eq!(game.queue(), queue);
            assert_eq!(game.snapshot().player, before.player);
            assert_eq!(game.snapshot().logical_tick, before.logical_tick);
            game.maintenance();
        }
        let continued = game.command(movement(1, true));
        assert!(continued.player.x > before.player.x);
        assert_eq!(continued.phase, GamePhase::Running);
        assert!(continued.error.is_none());
        assert!(matches!(
            capture_resume(&game.session),
            Err(GameResumeError::Ineligible(actual)) if actual.contains(reason)
        ));
        game.close();
    }
}

#[derive(Default)]
struct TimeInitializationGuard;

impl Plugin for TimeInitializationGuard {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-time-initialization-fault"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.__install_runtime_authority_validator(
            Self::declaration().id,
            std::sync::Arc::new(|world, _| {
                if world.resource::<FixedTime>().remainder() == Duration::from_millis(13) {
                    Err(nara::app::RuntimeFault::engine(
                        nara::app::RuntimeFaultKind::RuntimeAuthority,
                        "two-level-game.test-time-initialization-fault",
                    ))
                } else {
                    Ok(())
                }
            }),
        )?;
        Ok(())
    }
}

#[test]
fn headless_time_initialization_preserves_applied_fault_and_closes_the_original_owner() {
    let recipe = product_recipe()
        .add_plugin::<TimeInitializationGuard>()
        .unwrap();
    let mut session = HeadlessSession::new(two_level_game::open_project_root().unwrap(), recipe);
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.start().unwrap();
        session.pause().unwrap();
        let mut state = two_level_game::resume::RuntimeTimeState::from_continuation(
            session.capture_time_continuation().unwrap(),
        )
        .unwrap();
        state.pending_nanos = 13_000_000;
        let error = session
            .restore_time_continuation(state.to_continuation().unwrap())
            .unwrap_err();
        let HeadlessSessionError::TimeContinuation(error) = error else {
            panic!("original typed time error must be retained");
        };
        assert!(error.applied());
        assert_eq!(
            error.fault().unwrap().source(),
            "two-level-game.test-time-initialization-fault"
        );
        assert_eq!(session.state(), Some(RuntimeState::Faulted));
        assert!(session.capture(|_| ()).is_err());
        assert!(session.capture_time_continuation().is_err());
        assert!(
            session
                .restore_time_continuation(state.to_continuation().unwrap())
                .is_err()
        );
        assert!(session.frame(Duration::ZERO).is_err());
    }));
    let deadline = Instant::now() + DEADLINE;
    let terminal = loop {
        match session.close() {
            HeadlessSessionClose::Pending if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            HeadlessSessionClose::Pending => {
                std::mem::forget(session);
                panic!("original faulted Host cleanup remains retained");
            }
            terminal => break terminal,
        }
    };
    checked.unwrap();
    assert_eq!(terminal, HeadlessSessionClose::Failed);
    assert!(session.is_closed());
}

#[test]
fn capture_refuses_running_sessions_and_additional_custom_plugins() {
    let mut game = Game::source();
    let retained = game.frozen();
    assert_eq!(game.session.resume().unwrap(), None);
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Session(HeadlessSessionError::NotPaused))
    ));
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.frozen(), retained);
    game.close();

    let recipe = product_recipe()
        .add_plugin::<UnknownResumeService>()
        .unwrap();
    let mut unsupported = Game::with_recipe(recipe);
    let tick = unsupported.tick();
    let queue = unsupported.queue();
    assert!(matches!(
        capture_resume(&unsupported.session),
        Err(GameResumeError::Ineligible(reason))
            if reason.contains("unsupported services")
                && reason.contains("two-level-game.test-unsupported-resume-service")
    ));
    assert_eq!(unsupported.tick(), tick);
    assert_eq!(unsupported.queue(), queue);
    let continued = unsupported.command(movement(1, true));
    assert!(continued.player.x > -120.0);
    assert_eq!(continued.phase, GamePhase::Running);
    assert!(continued.error.is_none());
    assert!(matches!(
        capture_resume(&unsupported.session),
        Err(GameResumeError::Ineligible(reason))
            if reason.contains("two-level-game.test-unsupported-resume-service")
    ));
    unsupported.close();
}

#[test]
fn nonzero_runtime_phase_reconstructs_meaningful_game_and_matches_next_deltas() {
    let mut game = Game::source();
    let state = meaningful_game(&mut game);
    let tick = game.tick();
    let fraction = Duration::from_millis(13);
    assert_eq!(game.session.resume().unwrap(), None);
    assert_eq!(game.session.frame(fraction).unwrap(), None);
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.tick(), tick);
    assert_eq!(game.snapshot().logical_tick, state.clock.ticks);
    assert_eq!(game.snapshot().player.to_array(), state.player.translation);
    let time = game
        .session
        .observe(|world| *world.resource::<FixedTime>())
        .unwrap();
    assert_eq!(time.remainder(), fraction);
    assert_eq!(time.debt(), Duration::ZERO);
    let queue = game.queue();
    let captured = game.capture();
    assert_eq!(
        captured.runtime_time.pending_nanos,
        fraction.as_nanos() as u64
    );
    assert_eq!(captured.clock, state.clock);
    assert_eq!(game.queue(), queue);
    assert_eq!(
        game.session
            .observe(|world| *world.resource::<FixedTime>())
            .unwrap(),
        time
    );

    let mut candidate = Game::source();
    restore_candidate(&game, &mut candidate, &captured);
    let offset = game.tick() as i128 - candidate.tick() as i128;
    for millis in [3, 4, 5, 29, 60] {
        for current in [&mut game, &mut candidate] {
            assert_eq!(current.session.resume().unwrap(), None);
            assert_eq!(
                current
                    .session
                    .frame(Duration::from_millis(millis))
                    .unwrap(),
                None
            );
            assert_eq!(current.session.pause().unwrap(), None);
            current.settle();
        }
        compare_games(&game, &candidate);
        assert_eq!(game.tick() as i128 - candidate.tick() as i128, offset);
    }
    let continued = game.capture();
    assert_eq!(game.tick(), tick + 5);
    assert_eq!(continued.clock.ticks, state.clock.ticks + 5);
    assert_eq!(continued.runtime_time.pending_nanos, 14_000_000);
    assert_eq!(continued.held, state.held);
    assert!(continued.player.translation[0] > state.player.translation[0]);
    candidate.close();
    game.close();
}

#[test]
fn incompatible_time_policy_preserves_a_fresh_candidate_and_the_incumbent() {
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();
    let mut incompatible = state.clone();
    incompatible.runtime_time.max_steps_per_frame += 1;
    incompatible.validate().unwrap();
    let mut candidate = Game::source();
    let fresh = candidate.frozen();
    assert!(matches!(
        request_resume(&mut candidate.session, &incompatible),
        Err(GameResumeError::Session(
            HeadlessSessionError::TimeContinuation(RuntimeTimeContinuationError::Rejected(
                RuntimeTimeContinuationRejection::PolicyMismatch
            ))
        ))
    ));
    assert_eq!(candidate.frozen(), fresh);
    assert_eq!(original.frozen(), retained);
    restore_candidate(&original, &mut candidate, &state);
    compare_games(&original, &candidate);
    candidate.close();
    original.close();
}

#[test]
fn preserved_whole_tick_debt_reconstructs_without_changing_the_fresh_runtime_history() {
    let project = ProjectCopy::new();
    project.preserve_fixed_debt();
    let mut original = Game::from_project(project.root());
    meaningful_game(&mut original);
    original.session.resume().unwrap();
    original.session.frame(Duration::from_millis(250)).unwrap();
    original.session.pause().unwrap();
    original.settle();
    let captured = original.capture();
    assert_eq!(
        captured.runtime_time.catch_up_policy,
        two_level_game::resume::RuntimeCatchUpPolicy::PreserveDebt
    );
    assert!(captured.runtime_time.pending_nanos >= captured.runtime_time.timestep_nanos);
    assert_eq!(captured.runtime_time.pending_nanos, 150_000_000);
    let mut candidate = Game::from_project(project.root());
    restore_candidate(&original, &mut candidate, &captured);
    let offset = original.tick() - candidate.tick();
    for millis in [0, 7, 13, 40] {
        for current in [&mut original, &mut candidate] {
            current.session.resume().unwrap();
            current
                .session
                .frame(Duration::from_millis(millis))
                .unwrap();
            current.session.pause().unwrap();
            current.settle();
        }
        compare_games(&original, &candidate);
        assert_eq!(original.tick() - candidate.tick(), offset);
    }
    assert_eq!(original.capture().runtime_time.pending_nanos, 10_000_000);
    candidate.close();
    original.close();
}

#[test]
fn capture_refuses_an_open_save_slot_without_disabling_ordinary_gameplay() {
    let project = ProjectCopy::new();
    let recipe = configured_recipe(RunOptions {
        scenario: Scenario::Interactive,
        save_path: Some(project.save_path()),
    });
    let mut game = Game::from_project_with_recipe(project.root(), recipe);
    let tick = game.tick();
    let queue = game.queue();
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("SaveSlot")
    ));
    assert_eq!(game.tick(), tick);
    assert_eq!(game.queue(), queue);
    let continued = game.command(movement(1, true));
    assert_eq!(continued.phase, GamePhase::Running);
    assert!(continued.error.is_none());
    assert!(continued.player.x > -120.0);
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("SaveSlot")
    ));
    game.close();
}
