use std::{error::Error, fmt, sync::Arc};

use nara_app::{App, PluginError, RuntimeFaultDetail, StartupStage};
use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticReport, ReportDiagnostic as Diagnostic, SafeSummary,
};
use nara_ecs::{
    Res, ResMut, Resource, SystemSet, World,
    bevy_ecs::{error::BevyError, system::ScheduleSystem},
    schedule::IntoScheduleConfigs,
};

#[cfg(test)]
use crate::CompiledSceneCandidate;
use crate::{
    SceneComponentBindings, SceneOccurrenceKey, SceneSpawnReport, SpawnedSceneInstance,
    scene_component_bindings, validate_scene_component_bindings_authority,
};

use super::{CurrentScene, SceneSource, rejected};
use crate::candidate::SceneMaterializationInput;

#[derive(Resource)]
struct StartupSceneActivationInput(CurrentScene);

#[derive(Resource)]
struct StartupSceneActivationAuthority {
    phase: StartupSceneActivationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupSceneActivationPhase {
    Open,
    Materialized,
    Finalized,
}

/// Selects the active primary Scene's bindings during or after its startup activation.
pub fn current_scene_inspection_bindings(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<SceneComponentBindings, crate::SceneSpawnReport> {
    if !world
        .get_resource::<StartupSceneActivationAuthority>()
        .is_some_and(|authority| authority.phase == StartupSceneActivationPhase::Materialized)
        || world.contains_resource::<StartupSceneActivationInput>()
    {
        current_scene_bindings(world, expected)
    } else {
        bindings_for_current(world, expected)
    }
}

/// Runs startup systems after the primary Scene is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct StartupSceneActivationSet;

/// Registers startup systems that inspect the newly activated Scene.
pub fn add_startup_scene_activation_systems<'a, M>(
    app: &'a mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&'a mut App, PluginError> {
    app.add_systems(
        StartupStage::Runtime,
        systems.in_set(StartupSceneActivationSet),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum StartupSceneActivationInternalSet {
    Consume,
    Finalize,
}

/// Installs Scene startup phases into an existing integration's App.
pub fn install_startup_scene_activation(app: &mut App) -> Result<(), PluginError> {
    app.insert_resource(StartupSceneActivationAuthority {
        phase: StartupSceneActivationPhase::Open,
    })?
    .configure_sets(
        StartupStage::Runtime,
        (
            StartupSceneActivationInternalSet::Consume,
            StartupSceneActivationSet,
            StartupSceneActivationInternalSet::Finalize,
        )
            .chain(),
    )?
    .add_systems(
        StartupStage::Runtime,
        consume_startup_scene_activation.in_set(StartupSceneActivationInternalSet::Consume),
    )?
    .add_systems(
        StartupStage::Runtime,
        finalize_startup_scene_activation.in_set(StartupSceneActivationInternalSet::Finalize),
    )?;
    Ok(())
}

/// Spawns one retained source and publishes only its matching successful receipt.
///
/// The operation must run while the caller has exclusive access to an unpublished managed-App
/// candidate. Every predictable rejection occurs before scene spawning. A scene failure publishes
/// neither pending input nor activation. The complete component selection is always derived from
/// and validated against the candidate World; callers cannot substitute another selection.
pub fn materialize_startup_scene(
    world: &mut World,
    source: SceneSource,
) -> Result<DiagnosticReport, StartupSceneMaterializeError> {
    if !world.contains_resource::<StartupSceneActivationAuthority>() {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.activation-authority-missing",
            "Startup scene activation authority is unavailable",
        ));
    }
    match world.resource::<StartupSceneActivationAuthority>().phase {
        StartupSceneActivationPhase::Open => {}
        StartupSceneActivationPhase::Materialized => {
            return Err(StartupSceneMaterializeError::single(
                "startup.scene.activation-already-present",
                "Startup scene activation is already present",
            ));
        }
        StartupSceneActivationPhase::Finalized => {
            return Err(StartupSceneMaterializeError::single(
                "startup.scene.activation-window-closed",
                "Startup scene activation window is closed",
            ));
        }
    }
    if world.contains_resource::<StartupSceneActivationInput>()
        || world.contains_resource::<CurrentScene>()
    {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.activation-state-conflict",
            "Startup scene activation state conflicts with its open authority",
        ));
    }

    validate_scene_component_bindings_authority(world).map_err(|_| {
        StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-authority-invalid",
            "Startup scene component bindings authority is invalid",
        )
    })?;
    let bindings = scene_component_bindings(world).cloned().ok_or_else(|| {
        StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-unavailable",
            "Startup scene component bindings are unavailable",
        )
    })?;
    if !source.matches_component_bindings(&bindings) {
        return Err(StartupSceneMaterializeError::single(
            "startup.scene.component-bindings-mismatch",
            "Startup scene source does not match the installed component definitions",
        ));
    }

    // The product owns one current-Scene slot for the complete managed runtime lifetime.
    let occurrence = SceneOccurrenceKey::new("nara.product", "current-scene")
        .expect("the product declares a valid startup occurrence");
    let prepared = match source.materialization_input() {
        SceneMaterializationInput::Compiled(candidate) => {
            bindings.prepare_scene(world, candidate, &occurrence, source.asset_resolution())
        }
        #[cfg(feature = "serde")]
        SceneMaterializationInput::Runtime(candidate) => {
            bindings.prepare_runtime_scene(world, candidate, &occurrence)
        }
    }
    .map_err(|report| StartupSceneMaterializeError::new(report.diagnostics))?;
    let mut report = prepared.commit(world);
    if report.diagnostics.has_errors() {
        return Err(StartupSceneMaterializeError::new(report.diagnostics));
    }
    let receipt = report
        .instance
        .take()
        .expect("successful scene spawning always publishes an instance receipt");
    world.insert_resource(StartupSceneActivationInput(CurrentScene {
        source: Arc::new(source),
        receipt,
    }));
    world
        .resource_mut::<StartupSceneActivationAuthority>()
        .phase = StartupSceneActivationPhase::Materialized;
    Ok(report.diagnostics)
}

/// Inspects the instance materialized for the upcoming startup schedule.
#[must_use]
pub fn materialized_startup_scene_receipt(world: &World) -> Option<&SpawnedSceneInstance> {
    world
        .get_resource::<StartupSceneActivationInput>()
        .map(|pending| &pending.0.receipt)
}

fn consume_startup_scene_activation(world: &mut World) -> Result<(), BevyError> {
    if !world.contains_resource::<StartupSceneActivationInput>() {
        return Ok(());
    }
    if world.contains_resource::<CurrentScene>() {
        return Err(classified_startup_error(
            "startup.scene.activation-conflict",
            "Startup scene activation authority conflicts with pending input",
        ));
    }
    let pending = world
        .remove_resource::<StartupSceneActivationInput>()
        .expect("the exclusive consume system observed one pending input");
    world.insert_resource(pending.0);
    Ok(())
}

fn finalize_startup_scene_activation(
    mut authority: ResMut<StartupSceneActivationAuthority>,
    pending: Option<Res<StartupSceneActivationInput>>,
    active: Option<Res<CurrentScene>>,
) -> Result<(), BevyError> {
    let phase = authority.phase;
    authority.phase = StartupSceneActivationPhase::Finalized;
    if pending.is_some() {
        return Err(classified_startup_error(
            "startup.scene.activation-unconsumed",
            "Startup scene activation input was not promoted",
        ));
    }
    if phase == StartupSceneActivationPhase::Materialized && active.is_none() {
        return Err(classified_startup_error(
            "startup.scene.activation-owner-missing",
            "Startup scene activation owner is unavailable",
        ));
    }
    if phase == StartupSceneActivationPhase::Open && active.is_some() {
        return Err(classified_startup_error(
            "startup.scene.activation-owner-unexpected",
            "Startup scene activation owner exists without materialization authority",
        ));
    }
    Ok(())
}

fn classified_startup_error(code: &'static str, summary: &'static str) -> BevyError {
    let detail = RuntimeFaultDetail::new(code, summary, "nara.startup-scene")
        .expect("engine-owned startup diagnostic metadata is valid");
    detail.into_bevy_error()
}

/// Failure from the sealed startup-scene materialization operation.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupSceneMaterializeError {
    diagnostics: Box<DiagnosticReport>,
}

impl StartupSceneMaterializeError {
    fn new(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
        }
    }

    fn single(code: &'static str, summary: &'static str) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::new(code).expect("engine-owned diagnostic codes are valid"),
            SafeSummary::new(summary).expect("engine-owned diagnostic summaries are safe"),
        ));
        Self::new(diagnostics)
    }

    /// Returns the structured diagnostics that rejected materialization.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}

impl fmt::Display for StartupSceneMaterializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("startup scene materialization failed")
    }
}

impl Error for StartupSceneMaterializeError {}

/// Selects the active primary Scene's bindings after startup completes.
pub fn current_scene_bindings(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<SceneComponentBindings, SceneSpawnReport> {
    if !world
        .get_resource::<StartupSceneActivationAuthority>()
        .is_some_and(|authority| authority.phase == StartupSceneActivationPhase::Finalized)
    {
        return Err(rejected(
            "scene.current.startup-not-finalized",
            "Current Scene replacement requires completed startup",
        ));
    }
    bindings_for_current(world, expected)
}

fn bindings_for_current(
    world: &World,
    expected: &SpawnedSceneInstance,
) -> Result<SceneComponentBindings, SceneSpawnReport> {
    let Some(owner) = world.get_resource::<CurrentScene>() else {
        return Err(rejected(
            "scene.current.owner-missing",
            "Current Scene ownership is unavailable",
        ));
    };
    if owner.receipt != *expected {
        return Err(rejected(
            "scene.current.expected-instance-mismatch",
            "Current Scene no longer matches the expected instance",
        ));
    }
    if validate_scene_component_bindings_authority(world).is_err() {
        return Err(rejected(
            "scene.current.component-bindings-authority-invalid",
            "Current Scene component bindings authority is invalid",
        ));
    }
    let Some(bindings) = scene_component_bindings(world).cloned() else {
        return Err(rejected(
            "scene.current.component-bindings-unavailable",
            "Current Scene component bindings are unavailable",
        ));
    };
    Ok(bindings)
}

#[cfg(test)]
pub(crate) fn compile_test_scene(
    document: &crate::SceneDocument,
    bindings: &SceneComponentBindings,
    source: crate::SourceDocumentKey,
) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(document, &crate::InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .candidate
        .expect("the fixture compiles")
        .bind_source(source)
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use crate::{CompiledSceneDocument, SceneDocument, spawn_scene};
    use nara_ecs::bevy_ecs::system::SystemState;
    use nara_reflect::ComponentRegistryBuilder;

    use super::*;
    use crate::current_scene::{
        RetainedSceneCandidate, SceneSourceView, StartupSceneActivationPlugin,
    };
    use nara_ecs::Res;

    fn scene_candidate(document: Arc<SceneDocument>) -> CompiledSceneCandidate {
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        compile_test_scene(
            &document,
            &SceneComponentBindings::new(registry, []).unwrap(),
            crate::SourceDocumentKey::named(
                "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
                "startup-scene-fixture",
                crate::SourceDocumentKind::Scene,
            )
            .unwrap(),
        )
    }

    #[test]
    fn borrowed_current_scene_is_absent_without_activation() {
        let world = World::new();
        assert!(CurrentScene::get(&world).is_none());
    }

    #[test]
    fn borrowed_and_scheduled_current_scene_share_owner_without_retaining_source() {
        let mut world = World::new();
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(SceneSource::direct(scene_candidate(Arc::clone(&document))));
        let RetainedSceneCandidate::Compiled(compiled) = &source.candidate else {
            panic!("the fixture retains a source compilation");
        };
        let compiled_weak = Arc::downgrade(compiled);
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let report = spawn_scene(
            &mut world,
            &registry,
            source.compiled_scene().unwrap(),
            &SceneOccurrenceKey::new("startup-fixture", "current").unwrap(),
            None,
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        world.insert_resource(CurrentScene {
            source,
            receipt: report.instance.unwrap(),
        });
        let mut access = SystemState::<Res<CurrentScene>>::new(&mut world);
        let (source_view, scheduled_view) = {
            let current = CurrentScene::get(&world).unwrap();
            let scheduled = access.get(&world).unwrap();
            assert!(std::ptr::eq(
                current.source(),
                current.source.compiled_scene().unwrap().document()
            ));
            assert!(std::ptr::eq(current.source(), scheduled.source()));
            assert!(std::ptr::eq(current.receipt(), scheduled.receipt()));
            #[cfg(feature = "runtime-catalog")]
            {
                assert_eq!(current.source_content_stamp(), None);
                assert_eq!(scheduled.source_content_stamp(), None);
            }
            let source_view = current.source_view();
            let scheduled_view = scheduled.source_view();
            assert!(std::sync::Weak::ptr_eq(&source_view.0, &scheduled_view.0));
            assert_eq!(
                source_view.with_document(|source| source as *const CompiledSceneDocument),
                Some(current.source() as *const CompiledSceneDocument)
            );
            assert_eq!(Arc::strong_count(&current.source), 1);
            (source_view, scheduled_view)
        };

        drop(world);
        assert_eq!(source_view.with_document(|_| ()), None);
        assert_eq!(scheduled_view.with_document(|_| ()), None);
        assert!(compiled_weak.upgrade().is_none());
        assert_eq!(Arc::strong_count(&document), 1);
    }

    #[test]
    fn source_view_cannot_extend_the_root_document_lifetime() {
        let document = Arc::new(SceneDocument::from_ordered_roots([]));
        let source = Arc::new(SceneSource::direct(scene_candidate(Arc::clone(&document))));
        let view = SceneSourceView(Arc::downgrade(&source));
        let clone = view.clone();
        let RetainedSceneCandidate::Compiled(compiled) = &source.candidate else {
            panic!("the fixture retains a source compilation");
        };
        let compiled_weak = Arc::downgrade(compiled);

        assert_eq!(Arc::strong_count(&source), 1);
        assert_eq!(
            view.with_document(|source| source.entities().len()),
            Some(0)
        );
        assert_eq!(
            view.with_compiled_scene(|candidate, database| {
                assert!(candidate.canonical_provenance().is_empty());
                assert!(database.is_none());
                candidate.document().entities().len()
            }),
            Some(0)
        );

        drop(source);
        assert!(compiled_weak.upgrade().is_none());
        assert_eq!(Arc::strong_count(&document), 1);
        assert_eq!(view.with_document(|source| source.entities().len()), None);
        assert_eq!(clone.with_document(|source| source.entities().len()), None);
        assert_eq!(view.with_compiled_scene(|_, _| ()), None);
    }

    #[test]
    fn direct_materialization_uses_candidate_bindings_and_promotes_once() {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            crate::SceneComponentsPlugin,
            crate::SceneComponentBindingsPlugin::new([]),
            StartupSceneActivationPlugin,
        ))
        .unwrap();
        add_startup_scene_activation_systems(&mut app, |world: &World| {
            let current = CurrentScene::get(world).unwrap();
            let report = current_scene_bindings(world, current.receipt()).unwrap_err();
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.current.startup-not-finalized"
            }));
            assert!(current_scene_inspection_bindings(world, current.receipt()).is_ok());
        })
        .unwrap();
        let sealed = app.seal().unwrap();
        let mut candidate = nara_app::RuntimeCandidate::admit(
            sealed,
            nara_app::RuntimeObligationLedger::new(),
            nara_app::RuntimeClosePolicy::default(),
        )
        .unwrap();
        let source = SceneSource::direct(scene_candidate(Arc::new(
            SceneDocument::from_ordered_roots([]),
        )));
        let materialization = Arc::new(OnceLock::new());
        let command_result = Arc::clone(&materialization);

        candidate
            .with_admission_scope(move |scope| {
                scope.apply_command(move |world: &mut World| {
                    assert!(
                        command_result
                            .set(materialize_startup_scene(world, source))
                            .is_ok(),
                        "one direct admission command publishes one result"
                    );
                });
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap();
        let diagnostics = Arc::try_unwrap(materialization)
            .ok()
            .and_then(OnceLock::into_inner)
            .expect("the direct admission command returned a result")
            .unwrap();

        assert!(!diagnostics.has_errors());
        assert!(
            candidate
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(!candidate.world().contains_resource::<CurrentScene>());
        let runtime = candidate.complete_startup().unwrap().promote();
        assert!(
            !runtime
                .world()
                .contains_resource::<StartupSceneActivationInput>()
        );
        assert!(runtime.world().contains_resource::<CurrentScene>());
        assert_eq!(
            runtime
                .world()
                .resource::<StartupSceneActivationAuthority>()
                .phase,
            StartupSceneActivationPhase::Finalized
        );
        let mut retirement = runtime.begin_retirement();
        assert_eq!(
            retirement.drive_retirement(),
            nara_app::RuntimeCandidateRetirementState::Retired
        );
    }

    #[test]
    fn direct_materialization_is_rejected_after_startup_finalizes() {
        let mut world = World::new();
        world.insert_resource(StartupSceneActivationAuthority {
            phase: StartupSceneActivationPhase::Finalized,
        });
        let entity_count = world.entities().len();
        let source = SceneSource::direct(scene_candidate(Arc::new(
            SceneDocument::from_ordered_roots([]),
        )));
        let error = materialize_startup_scene(&mut world, source).unwrap_err();

        assert_eq!(
            error.diagnostics().iter().next().unwrap().code().as_str(),
            "startup.scene.activation-window-closed"
        );
        assert_eq!(world.entities().len(), entity_count);
        assert!(!world.contains_resource::<StartupSceneActivationInput>());
        assert!(!world.contains_resource::<CurrentScene>());
        assert_eq!(
            world.resource::<StartupSceneActivationAuthority>().phase,
            StartupSceneActivationPhase::Finalized
        );
    }
}
