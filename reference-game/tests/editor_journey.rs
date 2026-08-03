#![cfg(feature = "editor")]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    gameplay::GameplayCommandQueue,
    prelude::{App, Query, Res, ResMut, Resource},
    project_host::{EditorProjectIntent, EditorProjectSession},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{SceneEntitySource, ScenePatchDocument, ScenePatchOperation},
    tooling::{
        EditorPersistenceCommand, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPlayCommand, EditorPlayRequestResult, EditorPlayState, EditorWorkspaceCommand,
        EditorWorkspaceIntent, EditorWorkspaceIntentRequestResult,
    },
    transform::Transform2d,
};
use nara_reference_game::{
    REFERENCE_WAVE_PLUGIN_ID, WaveRunGeneration, WaveSnapshot, retry_command, wave_recipe,
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
    let intent = EditorProjectIntent::new().with_recipe(recipe);
    let mut editor = EditorProjectSession::open(project.capability(), intent)
        .expect("the writable reference project should open in the Editor Host");
    let document = editor
        .workspace()
        .active_document()
        .expect("the reference project should open its startup scene");
    let saved_revision = editor
        .workspace()
        .scene(document)
        .expect("the startup scene should be active")
        .saved_revision();

    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
        document: Some(document),
        patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: nara::identity::SceneEntityId::new("player-weapon").unwrap(),
            component: ComponentTypeId::new("nara.transform.Transform2d"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("translation.x"),
            value: ComponentValue::f64(EDITED_WEAPON_X).unwrap(),
        }]),
    });
    assert!(report.applied, "{:#?}", report.diagnostics);
    let edited_revision = editor.workspace().scene(document).unwrap().revision();
    assert_ne!(edited_revision, saved_revision);
    assert!(editor.workspace().scene(document).unwrap().is_dirty());
    assert_eq!(project.scene_bytes(), source_bytes);

    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running);
    let runtime_generation = editor
        .play_view()
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
    assert_eq!(editor.play_view().generation(), Some(runtime_generation));
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.revision(), edited_revision);
    assert_eq!(slot.saved_revision(), saved_revision);
    assert!(slot.is_dirty());
    assert_eq!(project.scene_bytes(), source_bytes);

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty);
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.revision(), edited_revision);
    assert!(slot.is_dirty());

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor
            .drive_editor_frame(Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Saved {
            document: saved_document,
            revision,
            ..
        }) if saved_document == document && revision == edited_revision
    ));
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(!editor.workspace().scene(document).unwrap().is_dirty());
    assert_ne!(project.scene_bytes(), source_bytes);

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().is_empty());
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: None }),
        EditorPersistenceRequestResult::Accepted
    );
    let reopened = editor.drive_editor_frame(Duration::ZERO).persistence();
    let reopened_document = match reopened.result() {
        Some(EditorPersistenceResult::Opened { document, .. }) => document,
        other => panic!("expected the saved reference scene to reopen, got {other:?}"),
    };
    let reopened_slot = editor.workspace().scene(reopened_document).unwrap();
    assert!(!reopened_slot.is_dirty());
    assert_eq!(
        weapon_translation_x(reopened_slot.session().document()),
        EDITED_WEAPON_X
    );
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
        .add_systems(CoreStage::Last, observe_editor_journey)?;
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
    sources: Query<&SceneEntitySource>,
    mut transforms: Query<(&SceneEntitySource, &mut Transform2d)>,
    mut commands: ResMut<GameplayCommandQueue>,
    mut probe: ResMut<EditorJourneyProbe>,
) {
    let current_generation = generation.get();
    let mut weapon_count = 0;
    let mut observed_weapon = None;
    let mut runtime_weapon_x = None;
    for (source, mut transform) in transforms.iter_mut() {
        if source.entity_id.as_str() != "player-weapon" {
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
                    .filter(|source| source.instance_id.get() == initial_instance_id)
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

fn drive_until(editor: &mut EditorProjectSession, expected: EditorPlayState) {
    let deadline = Instant::now() + EDITOR_JOURNEY_DEADLINE;
    while Instant::now() < deadline {
        if editor.play_view().state() == expected {
            return;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        editor.play_view().state(),
        expected,
        "diagnostics: {:?}",
        editor.diagnostics()
    );
}

fn weapon_translation_x(document: &nara::scene::SceneDocument) -> f64 {
    document
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "player-weapon")
        .and_then(|entity| {
            entity
                .components
                .get(&ComponentTypeId::new("nara.transform.Transform2d"))
        })
        .and_then(|record| record.value.field("translation").ok())
        .and_then(|translation| translation.field_f64("x").ok())
        .expect("the reference weapon should retain its authored translation")
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
        for directory in ["assets", "prefabs", "scenes"] {
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
