#![cfg(feature = "editor")]

#[path = "support/editor_persistence_driver.rs"]
mod editor_persistence_driver;

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    ecs::With,
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    gameplay::GameplayCommandQueue,
    prelude::{App, Parent, Query, Res, ResMut, Resource},
    project_host::{EditorProjectSession, TrustedCompiledEditorProjectIntent},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{SceneEntitySource, ScenePatchDocument, ScenePatchOperation},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
        EditorPersistenceRequestResult, EditorPersistenceResult, EditorPlayAction,
        EditorPlayRequestResult, EditorPlayState, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentRequestResult,
    },
    transform::Transform2d,
};
use nara_reference_game::{
    PlayerRole, REFERENCE_WAVE_PLUGIN_ID, WaveRunGeneration, WaveSnapshot, Weapon, retry_command,
    wave_recipe,
};

const EDITED_WEAPON_X: f64 = 2.75;
const RUNTIME_SENTINEL_WEAPON_X: f32 = -91.25;
const EDITOR_FRAME: Duration = Duration::from_millis(20);
const EDITOR_JOURNEY_DEADLINE: Duration = Duration::from_secs(10);
const JOURNEY_PROBE_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.editor-journey");
const JOURNEY_PROBE_REQUIREMENTS: &[PluginId] = &[REFERENCE_WAVE_PLUGIN_ID];
const JOURNEY_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(JOURNEY_PROBE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(JOURNEY_PROBE_REQUIREMENTS);

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static JOURNEY_PROBE_SENDER: Mutex<Option<SyncSender<JourneyObservation>>> = Mutex::new(None);

#[test]
fn unsaved_editor_source_survives_retry_then_save_close_and_reopen() {
    let project = WritableReferenceProject::new();
    let source_bytes = project.scene_bytes();
    let (sender, receiver) = mpsc::sync_channel(4);
    let replaced_sender = JOURNEY_PROBE_SENDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(sender);
    assert!(replaced_sender.is_none());
    let recipe = wave_recipe()
        .and_then(|recipe| recipe.add_plugin::<EditorJourneyProbePlugin>())
        .expect("the Editor journey recipe should be statically valid");
    let intent = TrustedCompiledEditorProjectIntent::new().with_recipe(recipe);
    let mut editor = EditorProjectSession::open_trusted_compiled(project.capability(), intent)
        .expect("the writable reference project should open in the Editor Host");
    let initial_frame = editor.current_frame();
    let document = initial_frame
        .workspace()
        .active_document
        .expect("the reference project should open its startup scene");
    let saved_revision = initial_frame
        .workspace()
        .scene(document)
        .expect("the startup scene should be active")
        .revision;

    let outcome = dispatch_editor_action(
        &mut editor,
        &initial_frame,
        EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document,
            base_revision: saved_revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: nara::identity::SceneEntityId::new("player-weapon").unwrap(),
                component: ComponentTypeId::new("nara.transform.Transform2d"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("translation.x"),
                value: ComponentValue::f64(EDITED_WEAPON_X).unwrap(),
            }]),
        }),
    );
    assert!(matches!(
        outcome,
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let edited_frame = editor.drive_editor_frame(Duration::ZERO);
    let edited_revision = edited_frame.workspace().scene(document).unwrap().revision;
    assert_ne!(edited_revision, saved_revision);
    assert!(edited_frame.workspace().scene(document).unwrap().dirty);
    assert_eq!(project.scene_bytes(), source_bytes);

    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &edited_frame,
            EditorAction::Play(EditorPlayAction::Play {
                document,
                base_revision: edited_revision,
            }),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let running_frame = drive_until(&mut editor, EditorPlayState::Running);
    let runtime_generation = running_frame
        .play()
        .generation()
        .expect("Editor Play should publish one runtime generation");

    let mut initial_run = None;
    let mut retried_run = None;
    let retry_deadline = Instant::now() + EDITOR_JOURNEY_DEADLINE;
    while Instant::now() < retry_deadline {
        editor.drive_editor_frame(EDITOR_FRAME);
        while let Ok(observation) = receiver.try_recv() {
            match observation {
                JourneyObservation::Run(run) if run.generation == 1 => initial_run = Some(run),
                JourneyObservation::Run(run) if run.generation >= 2 => retried_run = Some(run),
                JourneyObservation::Run(_) => {}
                JourneyObservation::RetrySubmissionFailed => {
                    panic!("the Editor probe could not submit the semantic Retry command")
                }
                JourneyObservation::RuntimePerturbationLost { weapon_x } => panic!(
                    "the Editor probe lost its runtime-only perturbation before Retry: {weapon_x}"
                ),
            }
        }
        if retried_run.is_some() {
            break;
        }
    }

    let initial_run = initial_run.expect("the initial Editor Play generation should be observed");
    let retried_run = retried_run.expect("the semantic Retry generation should be observed");
    assert_eq!(initial_run.weapon_count, 1);
    assert_eq!(initial_run.weapon_x, EDITED_WEAPON_X as f32);
    assert_eq!(retried_run.weapon_count, 1);
    assert_eq!(retried_run.weapon_x, EDITED_WEAPON_X as f32);
    assert_ne!(retried_run.instance_id, initial_run.instance_id);
    assert_eq!(retried_run.prior_instance_members, 0);
    let retried_frame = editor.current_frame();
    assert_eq!(retried_frame.play().generation(), Some(runtime_generation));
    let slot = retried_frame.workspace().scene(document).unwrap();
    assert_eq!(slot.revision, edited_revision);
    assert!(slot.dirty);
    assert_eq!(project.scene_bytes(), source_bytes);

    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &retried_frame,
            EditorAction::Play(EditorPlayAction::Stop),
        ),
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    ));
    let stopped_frame = drive_until(&mut editor, EditorPlayState::Empty);
    let slot = stopped_frame.workspace().scene(document).unwrap();
    assert_eq!(slot.revision, edited_revision);
    assert!(slot.dirty);

    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &stopped_frame,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: edited_revision,
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    let saved_frame = editor_persistence_driver::drive_until_result(&mut editor);
    assert!(matches!(
        saved_frame.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: saved_document,
            revision,
            ..
        }) if saved_document == document && revision == edited_revision
    ));
    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &saved_frame,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    ));
    assert_ne!(project.scene_bytes(), source_bytes);

    let acknowledged_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(
        !acknowledged_frame
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &acknowledged_frame,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                EditorWorkspaceIntent::CloseDocument { document },
            )),
        ),
        EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
    ));
    let closed_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(closed_frame.workspace().scenes.is_empty());
    assert_eq!(
        closed_frame.reopen_target(),
        nara::tooling::EditorReopenTarget::Unavailable
    );
    close_editor(&mut editor);
    drop(editor);

    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new().with_recipe(wave_recipe().unwrap()),
    )
    .expect("a new Editor session should reopen the saved reference scene");
    let reopened_frame = editor.current_frame();
    let reopened_document_id = reopened_frame
        .workspace()
        .active_document
        .expect("the saved startup scene should be active after reopening");
    assert!(
        !reopened_frame
            .workspace()
            .scene(reopened_document_id)
            .unwrap()
            .dirty
    );
    let weapon = nara::identity::SceneEntityId::new("player-weapon").unwrap();
    assert!(matches!(
        dispatch_editor_action(
            &mut editor,
            &reopened_frame,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: reopened_document_id,
                entity: Some(weapon.clone()),
            }),
        ),
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let inspected_frame = editor.drive_editor_frame(Duration::ZERO);
    let field = inspected_frame
        .workspace()
        .active_scene
        .as_ref()
        .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
        .filter(|entity| entity.id == weapon)
        .and_then(|entity| {
            entity.components.iter().find(|component| {
                component.component == ComponentTypeId::new("nara.transform.Transform2d")
            })
        })
        .and_then(|component| {
            component
                .fields
                .iter()
                .find(|field| field.id == ComponentFieldId::new("translation.x"))
        })
        .and_then(|field| field.value.as_ref())
        .and_then(ComponentValue::as_f64)
        .expect("the reopened Inspector should expose the saved weapon translation");
    assert_eq!(field, EDITED_WEAPON_X);
    close_editor(&mut editor);
}

fn close_editor(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + EDITOR_JOURNEY_DEADLINE;
    loop {
        if matches!(
            editor.advance_close(),
            nara::project_host::EditorProjectCloseProgress::Complete(_)
        ) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the Editor journey must physically close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Debug, Default)]
struct EditorJourneyProbePlugin;

impl Plugin for EditorJourneyProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &JOURNEY_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let sender = JOURNEY_PROBE_SENDER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("the Editor journey should install its probe sender before Play");
        app.insert_resource(EditorJourneyProbe {
            sender,
            initial_reported: false,
            initial_instance_id: None,
            retry_submitted: false,
            retry_reported: false,
        })?
        .add_systems(CoreStage::Cleanup, observe_editor_journey)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct EditorJourneyProbe {
    sender: SyncSender<JourneyObservation>,
    initial_reported: bool,
    initial_instance_id: Option<u64>,
    retry_submitted: bool,
    retry_reported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ObservedRun {
    generation: u64,
    instance_id: u64,
    weapon_count: usize,
    weapon_x: f32,
    prior_instance_members: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum JourneyObservation {
    Run(ObservedRun),
    RetrySubmissionFailed,
    RuntimePerturbationLost { weapon_x: f32 },
}

fn observe_editor_journey(
    fixed_time: Res<FixedTime>,
    generation: Res<WaveRunGeneration>,
    snapshot: Res<WaveSnapshot>,
    sources: Query<(&SceneEntitySource, Option<&PlayerRole>)>,
    mut transforms: Query<(&SceneEntitySource, &Parent, &mut Transform2d), With<Weapon>>,
    mut commands: ResMut<GameplayCommandQueue>,
    mut probe: ResMut<EditorJourneyProbe>,
) {
    let current_generation = generation.get();
    let mut weapon_count = 0;
    let mut observed_weapon = None;
    let mut runtime_weapon_x = None;
    for (source, parent, mut transform) in transforms.iter_mut() {
        if !sources
            .get(parent.parent())
            .is_ok_and(|(player, role)| role.is_some() && player.instance_id == source.instance_id)
        {
            continue;
        }
        weapon_count += 1;
        observed_weapon.get_or_insert((source.instance_id.get(), transform.translation.x));
        if current_generation == 1 && !probe.initial_reported {
            transform.translation.x = RUNTIME_SENTINEL_WEAPON_X;
        }
        runtime_weapon_x.get_or_insert(transform.translation.x);
    }
    let Some((instance_id, weapon_x)) = observed_weapon else {
        return;
    };
    let runtime_weapon_x = runtime_weapon_x.expect("the observed weapon should have a transform");
    if current_generation == 1 && !probe.initial_reported {
        let _ = probe.sender.try_send(JourneyObservation::Run(ObservedRun {
            generation: current_generation,
            instance_id,
            weapon_count,
            weapon_x,
            prior_instance_members: 0,
        }));
        probe.initial_reported = true;
        probe.initial_instance_id = Some(instance_id);
    }
    if current_generation == 1 && snapshot.is_terminal() && !probe.retry_submitted {
        if weapon_count != 1 || runtime_weapon_x != RUNTIME_SENTINEL_WEAPON_X {
            let _ = probe
                .sender
                .try_send(JourneyObservation::RuntimePerturbationLost {
                    weapon_x: runtime_weapon_x,
                });
            probe.retry_submitted = true;
            return;
        }
        let Some(tick) = fixed_time.tick().checked_add(1) else {
            let _ = probe
                .sender
                .try_send(JourneyObservation::RetrySubmissionFailed);
            probe.retry_submitted = true;
            return;
        };
        let submission = retry_command(tick, 90_001)
            .expect("the engine-owned Editor journey Retry command is valid");
        if commands.submit(submission).is_err() {
            let _ = probe
                .sender
                .try_send(JourneyObservation::RetrySubmissionFailed);
        }
        probe.retry_submitted = true;
    }
    if current_generation >= 2 && !probe.retry_reported {
        let prior_instance_members = probe
            .initial_instance_id
            .map(|initial_instance_id| {
                sources
                    .iter()
                    .filter(|(source, _)| source.instance_id.get() == initial_instance_id)
                    .count()
            })
            .unwrap_or(0);
        let _ = probe.sender.try_send(JourneyObservation::Run(ObservedRun {
            generation: current_generation,
            instance_id,
            weapon_count,
            weapon_x,
            prior_instance_members,
        }));
        probe.retry_reported = true;
    }
}

fn dispatch_editor_action(
    editor: &mut EditorProjectSession,
    frame: &EditorFrameModel,
    action: EditorAction,
) -> EditorActionOutcome {
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
        .expect("the Editor journey always dispatches from its current frame");
    let mut outcomes = report.into_outcomes();
    assert_eq!(outcomes.len(), 1);
    outcomes.pop().unwrap()
}

fn drive_until(
    editor: &mut EditorProjectSession,
    expected: EditorPlayState,
) -> Arc<EditorFrameModel> {
    let deadline = Instant::now() + EDITOR_JOURNEY_DEADLINE;
    while Instant::now() < deadline {
        let frame = editor.current_frame();
        if frame.play().state() == expected {
            return frame;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let frame = editor.current_frame();
    assert_eq!(
        frame.play().state(),
        expected,
        "diagnostics: {:?}",
        frame.diagnostics()
    );
    frame
}

struct WritableReferenceProject {
    root: PathBuf,
}

impl WritableReferenceProject {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_reference_editor_journey_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir(&root).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        fs::copy(source.join("nara.toml"), root.join("nara.toml")).unwrap();
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            copy_tree(&source.join(directory), &root.join(directory));
        }
        Self { root }
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn scene_bytes(&self) -> Vec<u8> {
        fs::read(self.root.join("scenes/startup.scene.json")).unwrap()
    }
}

impl Drop for WritableReferenceProject {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let project_root = self.root.canonicalize().unwrap();
        assert!(project_root.starts_with(&temporary_root));
        assert!(
            project_root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara_reference_editor_journey_"))
        );
        fs::remove_dir_all(&project_root).unwrap();
    }
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.file_type().unwrap();
        let target_path = target.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&entry.path(), &target_path);
        } else if metadata.is_file() {
            fs::copy(entry.path(), target_path).unwrap();
        } else {
            panic!("the committed reference fixture must not contain links or special files");
        }
    }
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}
