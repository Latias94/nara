use std::sync::mpsc;

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError, PluginId,
    PluginPlan, RuntimeAdmissionReservation, RuntimeCandidate, RuntimeCandidateRetirementState,
    RuntimeCandidateScope, RuntimeClosePolicy, RuntimeObligationLedger, StartupStage,
};
use nara_core::ByteLimit;
use nara_ecs::{Res, ResMut, Resource, World};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
    ActionMapLimits, ActionMaps, InputPlugin, InputSeatId, InputSelectionError, KeyCode,
    MAX_SEAT_ACTION_MAPS, finalize_input_selection,
};

#[derive(Debug, Default, Resource)]
struct StartupObservations(Vec<(ActionMapBindings, bool)>);

fn observe_startup(maps: Res<ActionMaps>, mut observed: ResMut<StartupObservations>) {
    let map = maps.get(InputSeatId::PRIMARY).unwrap();
    observed.0.push((
        map.bindings_handle(),
        map.is_context_enabled(&ActionContext::gameplay()),
    ));
}

#[derive(Default)]
struct StartupObserverPlugin;

impl Plugin for StartupObserverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.input-selection.startup-observer"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(StartupObservations::default())?;
        app.add_systems(StartupStage::Core, observe_startup)?;
        Ok(())
    }
}

#[derive(Resource)]
struct SavedActionMap(ActionMap);

#[derive(Resource)]
struct SavedActionMaps(ActionMaps);

fn candidate(with_input: bool) -> RuntimeCandidate {
    let observer = PluginDefinition::for_default::<StartupObserverPlugin>();
    let plan = if with_input {
        PluginPlan::resolve((PluginDefinition::for_default::<InputPlugin>(), observer))
    } else {
        PluginPlan::resolve(observer)
    }
    .unwrap();
    plan.instantiate_runtime_candidate(
        RuntimeAdmissionReservation::try_acquire().unwrap(),
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::default(),
    )
    .unwrap()
}

fn with_scope<T>(
    candidate: &mut RuntimeCandidate,
    operation: impl FnOnce(&mut RuntimeCandidateScope<'_>) -> T,
) -> T {
    let outcome = candidate.with_admission_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn action_map(action: &str, key: KeyCode) -> ActionMap {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new(action).unwrap(), key))
        .unwrap();
    ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap()
}

fn assert_startup_selection(
    candidate: RuntimeCandidate,
    expected: &ActionMapBindings,
    gameplay_enabled: bool,
) {
    assert!(
        candidate
            .world()
            .resource::<StartupObservations>()
            .0
            .is_empty()
    );
    let runtime = candidate.complete_startup().unwrap().promote();
    let observed = &runtime.world().resource::<StartupObservations>().0;
    assert_eq!(observed.len(), 1);
    assert!(observed[0].0.shares_owner_with(expected));
    assert_eq!(observed[0].1, gameplay_enabled);
    runtime.finish().unwrap();
}

fn retire(candidate: RuntimeCandidate) {
    let mut retirement = candidate.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn selected_map_is_visible_in_the_first_startup_stage() {
    let mut candidate = candidate(true);
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();

    assert_eq!(
        with_scope(&mut candidate, |scope| {
            finalize_input_selection(scope, Some(selected))
        }),
        Ok(())
    );
    assert!(
        candidate
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&expected)
    );
    assert_startup_selection(candidate, &expected, true);
}

#[test]
fn an_explicit_default_collection_before_input_plugin_refuses_file_selection() {
    let mut app = App::new();
    app.insert_resource(ActionMaps::default()).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let result = with_scope(&mut candidate, |scope| {
        finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space)))
    });
    retire(candidate);
    assert_eq!(result, Err(InputSelectionError::CompetingConfiguration));
}

#[test]
fn another_explicit_seat_does_not_compete_with_primary_selection() {
    let mut candidate = candidate(true);
    let other_seat = InputSeatId::new(1);
    let mut secondary = action_map("code.confirm", KeyCode::Enter);
    secondary
        .disable_context(&ActionContext::gameplay())
        .unwrap();
    let secondary_bindings = secondary.bindings_handle();
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            assert!(
                world
                    .resource_mut::<ActionMaps>()
                    .insert(other_seat, secondary)
                    .unwrap()
                    .is_none()
            );
        });
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    let secondary = candidate
        .world()
        .resource::<ActionMaps>()
        .get(other_seat)
        .unwrap();
    assert!(
        secondary
            .bindings_handle()
            .shares_owner_with(&secondary_bindings)
    );
    assert!(!secondary.is_context_enabled(&ActionContext::gameplay()));
    assert_startup_selection(candidate, &expected, true);
}

#[cfg(feature = "serde")]
#[test]
fn source_and_processed_primary_selection_share_bindings_with_an_independent_second_seat() {
    use std::sync::Arc;

    use nara_asset::RuntimeAssetCodec;
    use nara_input::{ActionMapFileCandidate, ActionMapRuntimeCodec};

    let (_, definition) = ActionMapFileCandidate::decode_json_bytes(include_bytes!(
        "fixtures/formats/v1/action-map.json"
    ))
    .unwrap()
    .into_parts();
    let source_owner = Arc::new(definition);
    let source_retained = Arc::downgrade(&source_owner);
    let owner = Arc::new(
        ActionMapRuntimeCodec::decode_runtime(include_bytes!(
            "fixtures/formats/v1/runtime-action-map.json"
        ))
        .unwrap(),
    );
    let retained = Arc::downgrade(&owner);
    for bindings in [
        ActionMapBindings::from_owner(source_owner),
        ActionMapBindings::from_owner(owner),
    ] {
        let mut candidate = candidate(true);
        let selected = ActionMap::new(
            bindings.clone(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        let mut secondary = ActionMap::new(
            bindings.clone(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        secondary
            .disable_context(&ActionContext::gameplay())
            .unwrap();
        let other_seat = InputSeatId::new(1);
        with_scope(&mut candidate, |scope| {
            scope.apply_command(move |world: &mut World| {
                assert!(
                    world
                        .resource_mut::<ActionMaps>()
                        .insert(other_seat, secondary)
                        .unwrap()
                        .is_none()
                );
            });
            finalize_input_selection(scope, Some(selected)).unwrap();
        });
        let maps = candidate.world().resource::<ActionMaps>();
        let primary = maps.get(InputSeatId::PRIMARY).unwrap();
        let secondary = maps.get(other_seat).unwrap();
        assert!(primary.bindings_handle().shares_owner_with(&bindings));
        assert!(secondary.bindings_handle().shares_owner_with(&bindings));
        assert!(primary.is_context_enabled(&ActionContext::gameplay()));
        assert!(!secondary.is_context_enabled(&ActionContext::gameplay()));
        assert_startup_selection(candidate, &bindings, true);
    }
    assert!(source_retained.upgrade().is_none());
    assert!(retained.upgrade().is_none());
}

#[test]
fn a_full_seat_collection_returns_the_supplied_map_and_still_selects_primary() {
    let mut candidate = candidate(true);
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let mut maps = world.resource_mut::<ActionMaps>();
            for index in 1..MAX_SEAT_ACTION_MAPS {
                assert!(
                    maps.insert(
                        InputSeatId::new(u32::try_from(index).unwrap()),
                        ActionMap::default()
                    )
                    .unwrap()
                    .is_none()
                );
            }
        });
    });
    let supplied = action_map("refused.confirm", KeyCode::Enter);
    let expected_supplied = supplied.bindings_handle();
    let refused_seat = InputSeatId::new(999);
    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            let mut maps = world.resource_mut::<ActionMaps>();
            let refusal = maps.insert(refused_seat, supplied).unwrap_err();
            assert_eq!(refusal.seat(), refused_seat);
            assert!(maps.get(refused_seat).is_none());
            let recovered = refusal.into_map();
            assert!(
                recovered
                    .bindings_handle()
                    .shares_owner_with(&expected_supplied)
            );
            assert!(recovered.is_context_enabled(&ActionContext::gameplay()));
            let freed_seat = InputSeatId::new(1);
            assert!(maps.remove(freed_seat).is_some());
            assert!(maps.insert(refused_seat, recovered).unwrap().is_none());
        });
    });
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    assert!(
        candidate
            .world()
            .resource::<ActionMaps>()
            .get(refused_seat)
            .is_some()
    );
    assert_startup_selection(candidate, &expected, true);
}

#[test]
fn explicit_code_maps_including_empty_maps_refuse_file_selection() {
    for empty in [false, true] {
        let mut candidate = candidate(true);
        let code_map = if empty {
            ActionMap::default()
        } else {
            action_map("code.confirm", KeyCode::Enter)
        };
        let expected = code_map.bindings_handle();
        with_scope(&mut candidate, |scope| {
            scope.apply_command(move |world: &mut World| {
                world.insert_resource(ActionMaps::new(code_map));
            });
        });

        assert_eq!(
            with_scope(&mut candidate, |scope| {
                finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space)))
            }),
            Err(InputSelectionError::CompetingConfiguration)
        );
        let current = candidate
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .unwrap();
        assert!(current.bindings_handle().shares_owner_with(&expected));
        assert_eq!(
            current.is_context_enabled(&ActionContext::gameplay()),
            !empty
        );
        retire(candidate);
    }
}

#[test]
fn replacing_the_default_with_another_owners_pristine_map_refuses_selection() {
    let mut donor = candidate(true);
    let (sender, receiver) = mpsc::channel();
    with_scope(&mut donor, |scope| {
        scope.apply_command(move |world: &mut World| {
            sender
                .send(
                    world
                        .resource_mut::<ActionMaps>()
                        .remove(InputSeatId::PRIMARY)
                        .unwrap(),
                )
                .unwrap();
        });
    });
    let foreign_map = receiver.try_recv().unwrap();
    let foreign_bindings = foreign_map.bindings_handle();
    retire(donor);

    let mut candidate = candidate(true);
    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            world.insert_resource(ActionMaps::new(foreign_map));
        });
    });
    assert_eq!(
        with_scope(&mut candidate, |scope| {
            finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space)))
        }),
        Err(InputSelectionError::CompetingConfiguration)
    );
    assert!(
        candidate
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&foreign_bindings)
    );
    retire(candidate);
}

#[test]
fn selection_requires_the_input_plugin_owner_even_when_a_map_is_present() {
    let mut candidate = candidate(false);
    let map = action_map("code.confirm", KeyCode::Enter);
    let expected = map.bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            world.insert_resource(ActionMaps::new(map));
        });
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space))),
            Err(InputSelectionError::MissingInputOwner)
        );
        assert_eq!(
            finalize_input_selection(scope, None),
            Err(InputSelectionError::MissingInputOwner)
        );
    });
    assert!(
        candidate
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&expected)
    );
    retire(candidate);
}

#[test]
fn a_missing_collection_refuses_selection_without_consuming_the_installation_slot() {
    let mut candidate = candidate(true);
    let original = candidate
        .world()
        .resource::<ActionMaps>()
        .get(InputSeatId::PRIMARY)
        .unwrap()
        .bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let maps = world.remove_resource::<ActionMaps>().unwrap();
            world.insert_resource(SavedActionMaps(maps));
        });
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space))),
            Err(InputSelectionError::MissingActionMap)
        );
        assert_eq!(
            finalize_input_selection(scope, None),
            Err(InputSelectionError::MissingActionMap)
        );
    });
    assert!(!candidate.world().contains_resource::<ActionMaps>());
    assert!(
        candidate
            .world()
            .resource::<SavedActionMaps>()
            .0
            .get(InputSeatId::PRIMARY)
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&original)
    );

    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let saved = world.remove_resource::<SavedActionMaps>().unwrap();
            world.insert_resource(saved.0);
        });
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    assert_startup_selection(candidate, &expected, true);
}

#[test]
fn a_missing_primary_slot_refuses_selection_until_its_original_map_returns() {
    let mut candidate = candidate(true);
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let original = world
                .resource_mut::<ActionMaps>()
                .remove(InputSeatId::PRIMARY)
                .unwrap();
            world.insert_resource(SavedActionMap(original));
        });
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space))),
            Err(InputSelectionError::MissingActionMap)
        );
    });
    assert!(
        candidate
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .is_none()
    );
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let original = world.remove_resource::<SavedActionMap>().unwrap().0;
            assert!(
                world
                    .resource_mut::<ActionMaps>()
                    .insert(InputSeatId::PRIMARY, original)
                    .unwrap()
                    .is_none()
            );
        });
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    assert_startup_selection(candidate, &expected, true);
}

#[test]
fn a_successful_selection_refuses_all_duplicate_finalizations() {
    let mut candidate = candidate(true);
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        finalize_input_selection(scope, Some(selected)).unwrap();
        assert_eq!(
            finalize_input_selection(scope, None),
            Err(InputSelectionError::AlreadyFinalized)
        );
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("other.confirm", KeyCode::Enter))),
            Err(InputSelectionError::AlreadyFinalized)
        );
    });
    assert_startup_selection(candidate, &expected, true);
}

#[test]
fn no_selection_preserves_explicit_code_state_and_finalizes_once() {
    for empty in [false, true] {
        let mut candidate = candidate(true);
        let mut code_map = if empty {
            ActionMap::default()
        } else {
            action_map("code.confirm", KeyCode::Enter)
        };
        if !empty {
            assert!(
                code_map
                    .disable_context(&ActionContext::gameplay())
                    .unwrap()
            );
        }
        let expected = code_map.bindings_handle();
        with_scope(&mut candidate, |scope| {
            scope.apply_command(move |world: &mut World| {
                world.insert_resource(ActionMaps::new(code_map));
            });
            finalize_input_selection(scope, None).unwrap();
            assert_eq!(
                finalize_input_selection(scope, None),
                Err(InputSelectionError::AlreadyFinalized)
            );
            assert_eq!(
                finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space))),
                Err(InputSelectionError::AlreadyFinalized)
            );
        });
        assert_startup_selection(candidate, &expected, false);
    }
}

#[test]
fn failed_selection_preserves_configuration_and_can_retry_in_the_same_scope() {
    let mut candidate = candidate(true);
    let mut code_map = action_map("code.confirm", KeyCode::Enter);
    code_map
        .disable_context(&ActionContext::gameplay())
        .unwrap();
    let code_bindings = code_map.bindings_handle();
    let original_bindings = candidate
        .world()
        .resource::<ActionMaps>()
        .get(InputSeatId::PRIMARY)
        .unwrap()
        .bindings_handle();
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();

    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            let original = world
                .resource_mut::<ActionMaps>()
                .insert(InputSeatId::PRIMARY, code_map)
                .unwrap()
                .unwrap();
            world.insert_resource(SavedActionMap(original));
        });
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("rejected.jump", KeyCode::Space))),
            Err(InputSelectionError::CompetingConfiguration)
        );
        scope.apply_command(move |world: &mut World| {
            let current = world
                .resource::<ActionMaps>()
                .get(InputSeatId::PRIMARY)
                .unwrap();
            assert!(current.bindings_handle().shares_owner_with(&code_bindings));
            assert!(!current.is_context_enabled(&ActionContext::gameplay()));
            assert!(world.resource::<StartupObservations>().0.is_empty());
            let saved = world.remove_resource::<SavedActionMap>().unwrap();
            assert!(
                saved
                    .0
                    .bindings_handle()
                    .shares_owner_with(&original_bindings)
            );
            world
                .resource_mut::<ActionMaps>()
                .insert(InputSeatId::PRIMARY, saved.0)
                .unwrap();
        });
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    assert!(!candidate.world().contains_resource::<SavedActionMap>());
    assert_startup_selection(candidate, &expected, true);
}
