//! A separately compiled package uses the same inventory in its Editor and native recipe.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_engine::{
    ProductRecipe, SchemaContribution,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    diagnostic::DiagnosticValueRef,
    ecs::{Component, World},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession, ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentFieldId, ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponent,
        PersistentComponentProvider, validate_component_value,
    },
    scene::{
        AuthoringFieldEdit, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorWorkspaceCommandReport, SceneCompositionCommand,
    },
};

const OWNER: ComponentSchemaOwnerId = ComponentSchemaOwnerId::new("consumer.inventory");
const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.inventory.components");
const COOLDOWN: &str = "consumer.inventory.Cooldown";
const V2: ComponentSchemaVersion = ComponentSchemaVersion(2);

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.inventory.Health",
    version = 1,
    alias = "Health",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    authoring_initial
)]
pub struct Health {
    #[nara(id = "current", alias = "Current", initial = 100_u16)]
    pub current: u16,
}

// This historical declaration remains separate from the current native inventory.
#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.inventory.Cooldown",
    version = 1,
    alias = "Cooldown",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    authoring_initial
)]
pub struct LegacyCooldown {
    #[nara(id = "seconds", alias = "Seconds", initial = 0.5_f32)]
    pub seconds: f32,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.inventory.Cooldown",
    version = 2,
    alias = "Cooldown",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    authoring_initial
)]
pub struct Cooldown {
    #[nara(
        id = "seconds",
        alias = "Seconds",
        initial = 0.5_f32,
        range(min_inclusive = 0.0_f32, max_inclusive = 30.0_f32)
    )]
    pub seconds: f32,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.inventory.Armor",
    version = 1,
    alias = "Armor",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    authoring_initial
)]
pub struct Armor {
    #[nara(id = "points", alias = "Points", initial = 7_u16)]
    pub points: u16,
}

// The original provider uses the module directly; the successor exercises a renamed root facade.
nara_reflect::persistent_components! { pub mod generation_one = [Health, LegacyCooldown]; }
nara_engine::persistent_components! { pub mod current_types = [Health, Cooldown, Armor]; }

const INITIAL_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    OWNER,
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.inventory.native", 1),
    generation_one::initial_catalog,
    generation_one::register,
);
const CURRENT_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    OWNER,
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.inventory.native", 2).with_migration_version(2),
    current_catalog,
    register_current,
)
.with_predecessor(generation_one::initial_catalog);
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PluginId::new("consumer.inventory"), PluginCategory::Runtime)
        .requires_plugins(&[nara_engine::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[CURRENT_PROVIDER.id()]);

#[derive(Default)]
struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<InventoryPlugin>::for_default([CURRENT_PROVIDER]).unwrap(),
        )
        .unwrap()
}

fn current_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    let predecessor = generation_one::initial_catalog()?;
    let mut catalog = ComponentSchemaCatalog::successor_of(&predecessor).map_err(|_| {
        ComponentSchemaProviderSourceError::new("consumer.inventory.generation-exhausted")
    })?;
    catalog.components = current_types::schemas().map_err(|_| {
        ComponentSchemaProviderSourceError::new("consumer.inventory.schema-invalid")
    })?;
    Ok(catalog)
}

fn register_current(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    current_types::register(registry)?;
    registry.register_component_migration(
        &ComponentTypeId::new(COOLDOWN),
        ComponentSchemaVersion::ONE,
        V2,
        |value| {
            validate_component_value(&Cooldown::persistent_component_schema(), &value)?;
            Ok(value)
        },
    )?;
    Ok(())
}

fn current_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    CURRENT_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    registry
}

#[test]
fn one_current_inventory_extends_explicit_history_and_provider_admission_stays_atomic() {
    let historical = generation_one::initial_catalog().unwrap();
    let fingerprint = historical.fingerprint();
    assert_eq!(historical.components.len(), 2);
    assert!(
        !historical
            .components
            .iter()
            .any(|schema| schema.id().as_str() == "consumer.inventory.Armor")
    );
    let current = current_catalog().unwrap();
    assert_eq!(current.generation(), 2);
    assert_eq!(current.predecessor(), Some(&fingerprint));
    assert_eq!(current.components, current_types::schemas().unwrap());
    assert_eq!(current.components.len(), 3);
    assert_eq!(generation_one::initial_catalog().unwrap(), historical);

    let mut initial = ComponentRegistry::new();
    INITIAL_PROVIDER
        .register_or_validate_into(&mut initial)
        .unwrap();
    initial.freeze().unwrap();
    assert_eq!(
        initial
            .schema(&ComponentTypeId::new(COOLDOWN))
            .unwrap()
            .version(),
        ComponentSchemaVersion::ONE
    );

    let mut registry = ComponentRegistry::new();
    let before = registry.catalog_candidate().clone();
    let rejected = ComponentSchemaProviderDefinition::new(
        OWNER,
        PROVIDER_ID,
        CURRENT_PROVIDER.binding(),
        current_catalog,
        |candidate| {
            register_current(candidate)?;
            Err(ComponentRegistryError::Frozen)
        },
    )
    .with_predecessor(generation_one::initial_catalog);
    assert_eq!(
        rejected.register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::Frozen)
    );
    assert_eq!(registry.catalog_candidate(), &before);
    CURRENT_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.provider_receipts().count(), 1);
    assert_eq!(snapshot.owner_receipts().count(), 1);
    assert_eq!(
        snapshot.provider_receipt(PROVIDER_ID).unwrap().binding(),
        CURRENT_PROVIDER.binding()
    );
    assert_eq!(
        generation_one::initial_catalog().unwrap().fingerprint(),
        fingerprint
    );

    let mut app = App::new();
    app.add_plugins((nara_engine::MinimalPlugins, recipe()))
        .unwrap();
    let app = app.seal().unwrap();
    let direct = nara_engine::reflect::component_registry(app.world()).unwrap();
    assert_eq!(
        direct.schema(&ComponentTypeId::new(COOLDOWN)),
        registry.schema(&ComponentTypeId::new(COOLDOWN))
    );
    assert_eq!(
        direct.snapshot().unwrap().provider_receipt(PROVIDER_ID),
        snapshot.provider_receipt(PROVIDER_ID)
    );
}

#[test]
fn public_data_only_editor_creates_edits_rejects_saves_reopens_and_passes_compiled_check() {
    let project = Project::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        actor(),
    )]));
    let mut editor = project.open_editor();
    select_actor(&mut editor);
    for component in [COOLDOWN, "consumer.inventory.Armor"] {
        let frame = editor.current_frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let outcome = dispatch(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: actor(),
                    component: ComponentTypeId::new(component),
                },
            }),
        );
        assert!(workspace(outcome).applied);
    }
    assert_eq!(seconds(&editor), 0.5);
    assert!(edit_seconds(&mut editor, 2.5).applied);
    assert_eq!(seconds(&editor), 2.5);
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(
        workspace(dispatch(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: scene.document,
                base_revision: scene.revision,
            })
        ))
        .applied
    );
    assert_eq!(seconds(&editor), 0.5);
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(
        workspace(dispatch(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::Redo {
                document: scene.document,
                base_revision: scene.revision,
            })
        ))
        .applied
    );
    assert_eq!(seconds(&editor), 2.5);

    for rejected_value in [-1.0, 31.0] {
        let before = editor.current_frame();
        let document = before.workspace().active_document.unwrap();
        let tab = before.workspace().document(document).unwrap().clone();
        let report = edit_seconds(&mut editor, rejected_value);
        assert!(!report.applied);
        let range = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "nara.reflect.numeric-range")
            .unwrap();
        assert!(
            range
                .fields()
                .iter()
                .any(|field| field.key().as_str() == "component-field-id"
                    && field.value() == DiagnosticValueRef::Identifier("seconds"))
        );
        assert_eq!(
            editor.current_frame().workspace().document(document),
            Some(&tab)
        );
        assert_eq!(seconds(&editor), 2.5);
    }
    for endpoint in [0.0, 30.0, 2.5] {
        assert!(edit_seconds(&mut editor, endpoint).applied);
    }

    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let document = scene.document;
    let revision = scene.revision;
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.current_frame().persistence().result().is_none() {
        assert!(
            Instant::now() < deadline,
            "Editor persistence did not complete"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        matches!(editor.current_frame().persistence().result(), Some(EditorPersistenceResult::Saved { document: saved, revision: saved_revision, .. })
        if saved == document && saved_revision == revision)
    );
    let saved = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
    check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap();
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        saved
    );
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { .. })
    ));
    assert!(edit_seconds(&mut editor, 7.0).applied);
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.current_frame().persistence().result().is_none() {
        assert!(Instant::now() < deadline, "Editor Reopen did not complete");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    select_actor(&mut editor);
    assert_eq!(seconds(&editor), 2.5);
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the public Editor session did not retire its bounded background work");
}

#[test]
fn historical_values_migrate_without_clamping_and_strict_check_uses_the_same_recipe() {
    let registry = current_registry();
    let snapshot = registry.snapshot().unwrap();
    let id = ComponentTypeId::new(COOLDOWN);
    let original = ComponentValue::map([("seconds", ComponentValue::f64(3.1).unwrap())]);
    let migrated = registry
        .migrate_component_value(&id, ComponentSchemaVersion::ONE, &original)
        .unwrap();
    assert_eq!(migrated.version, V2);
    assert_eq!(migrated.value, original);
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    snapshot
        .preflight_component(&id, &migrated.value)
        .unwrap()
        .apply(&mut world, entity)
        .unwrap();
    assert_eq!(world.get::<Cooldown>(entity).unwrap().seconds, 3.1_f32);
    let invalid = ComponentValue::map([("seconds", ComponentValue::f64(31.0).unwrap())]);
    assert!(matches!(
        snapshot.preflight_component(&id, &invalid),
        Err(nara_engine::reflect::ComponentCodecError::Domain(_))
    ));
    assert!(
        registry
            .migrate_component_value(&id, ComponentSchemaVersion::ONE, &invalid)
            .is_err()
    );

    let legacy = SceneDocument::from_ordered_roots([SceneEntityRecord::new(actor())
        .with_component(
            id.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, original),
        )]);
    let project = Project::new(legacy);
    let before = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
    check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap();
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        before
    );
    let invalid_history = SceneDocument::from_ordered_roots([SceneEntityRecord::new(actor())
        .with_component(
            id,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, invalid),
        )]);
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        invalid_history.to_json_string().unwrap(),
    )
    .unwrap();
    let error =
        check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap_err();
    assert!(
        error
            .content_error()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "nara.reflect.numeric-range")
    );
}

fn actor() -> SceneEntityId {
    SceneEntityId::new("actor").unwrap()
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let frame = editor.current_frame();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn workspace(outcome: EditorActionOutcome) -> EditorWorkspaceCommandReport {
    match outcome {
        EditorActionOutcome::Workspace(report) => report,
        other => panic!("unexpected action result: {other:?}"),
    }
}

fn select_actor(editor: &mut EditorProjectSession) {
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(
        workspace(dispatch(
            editor,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document,
                entity: Some(actor()),
            })
        ))
        .applied
    );
}

fn edit_seconds(editor: &mut EditorProjectSession, value: f64) -> EditorWorkspaceCommandReport {
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    workspace(dispatch(
        editor,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
            document: scene.document,
            base_revision: scene.revision,
            edit: AuthoringFieldEdit::single(
                actor(),
                ComponentTypeId::new(COOLDOWN),
                V2,
                ComponentFieldId::new("seconds"),
                ComponentValue::f64(value).unwrap(),
            )
            .unwrap(),
        }),
    ))
}

fn seconds(editor: &EditorProjectSession) -> f64 {
    let frame = editor.current_frame();
    let selected = frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let component = selected
        .components
        .iter()
        .find(|component| component.component.as_str() == COOLDOWN)
        .unwrap();
    component
        .fields
        .iter()
        .find(|field| field.id.as_str() == "seconds")
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .as_f64()
        .unwrap()
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(document: SceneDocument) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-component-inventory-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '46044116-21ed-563c-ac41-77db155ac518'\nnative_document_identity = 'project_location'\nname = \"Component inventory consumer\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        let project = Self { root };
        let capability = project.capability();
        let manifest = capability
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn open_editor(&self) -> EditorProjectSession {
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(self.capability(), None);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Opened(editor) => return editor,
                EditorProjectOpenProgress::Rejected(failure) => {
                    panic!("public authoring open failed: {:?}", failure.diagnostics())
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("public authoring Open timed out")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    }
    #[cfg(not(windows))]
    {
        File::open(path).unwrap()
    }
}
