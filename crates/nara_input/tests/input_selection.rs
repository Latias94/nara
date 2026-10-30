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
    ActionMapLimits, InputPlugin, InputSelectionError, KeyCode, finalize_input_selection,
};

#[derive(Debug, Default, Resource)]
struct StartupObservations(Vec<(ActionMapBindings, bool)>);

fn observe_startup(map: Res<ActionMap>, mut observed: ResMut<StartupObservations>) {
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
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&expected)
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
                world.insert_resource(code_map);
            });
        });

        assert_eq!(
            with_scope(&mut candidate, |scope| {
                finalize_input_selection(scope, Some(action_map("file.jump", KeyCode::Space)))
            }),
            Err(InputSelectionError::CompetingConfiguration)
        );
        let current = candidate.world().resource::<ActionMap>();
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
                .send(world.remove_resource::<ActionMap>().unwrap())
                .unwrap();
        });
    });
    let foreign_map = receiver.try_recv().unwrap();
    let foreign_bindings = foreign_map.bindings_handle();
    retire(donor);

    let mut candidate = candidate(true);
    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            world.insert_resource(foreign_map);
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
            .resource::<ActionMap>()
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
            world.insert_resource(map);
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
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&expected)
    );
    retire(candidate);
}

#[test]
fn a_missing_map_refuses_selection_without_consuming_the_installation_slot() {
    let mut candidate = candidate(true);
    let original = candidate.world().resource::<ActionMap>().bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let map = world.remove_resource::<ActionMap>().unwrap();
            world.insert_resource(SavedActionMap(map));
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
    assert!(!candidate.world().contains_resource::<ActionMap>());
    assert!(
        candidate
            .world()
            .resource::<SavedActionMap>()
            .0
            .bindings_handle()
            .shares_owner_with(&original)
    );

    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();
    with_scope(&mut candidate, |scope| {
        scope.apply_command(|world: &mut World| {
            let saved = world.remove_resource::<SavedActionMap>().unwrap();
            world.insert_resource(saved.0);
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
            assert!(code_map.disable_context(ActionContext::gameplay()).unwrap());
        }
        let expected = code_map.bindings_handle();
        with_scope(&mut candidate, |scope| {
            scope.apply_command(move |world: &mut World| {
                world.insert_resource(code_map);
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
    code_map.disable_context(ActionContext::gameplay()).unwrap();
    let code_bindings = code_map.bindings_handle();
    let original_bindings = candidate.world().resource::<ActionMap>().bindings_handle();
    let selected = action_map("file.jump", KeyCode::Space);
    let expected = selected.bindings_handle();

    with_scope(&mut candidate, |scope| {
        scope.apply_command(move |world: &mut World| {
            let original = world.remove_resource::<ActionMap>().unwrap();
            world.insert_resource(SavedActionMap(original));
            world.insert_resource(code_map);
        });
        assert_eq!(
            finalize_input_selection(scope, Some(action_map("rejected.jump", KeyCode::Space))),
            Err(InputSelectionError::CompetingConfiguration)
        );
        scope.apply_command(move |world: &mut World| {
            let current = world.resource::<ActionMap>();
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
            world.insert_resource(saved.0);
        });
        finalize_input_selection(scope, Some(selected)).unwrap();
    });
    assert!(!candidate.world().contains_resource::<SavedActionMap>());
    assert_startup_selection(candidate, &expected, true);
}
