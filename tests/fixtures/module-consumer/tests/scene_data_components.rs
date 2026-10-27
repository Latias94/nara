//! Public file-backed native/data workflows, without a language runtime or native data codecs.

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use nara_asset::AssetRef;
use nara_ecs::{Entity, World};
use nara_engine::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession,
    },
};
use nara_identity::{EntityLookup, EntityReference, SpawnedSceneInstance};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, ExpandedSceneCandidate, InMemoryPrefabSourceResolver, Name,
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, SCENE_COMPONENTS_SCHEMA_PROVIDER,
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, SceneEntitySource, SceneExportOptions, SceneExportOutput,
    SceneOccurrenceKey, ScenePatchDocument, ScenePatchOperation, SourceDocumentKey,
    SourceDocumentKind, retire_and_despawn_scene_entity, retire_and_despawn_scene_instance,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
    EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
};

const MOTION: &str = "consumer.scene_data.Motion";
const SENSOR: &str = "consumer.scene_data.Sensor";
const NAME: &str = "nara.scene.Name";
const SCENE: &str = "scenes/startup.scene.json";
const PREFAB: &str = "prefabs/actor.prefab.ron";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn data_record(kind: &str, value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner(kind))
}

fn motion(amount: u64) -> SceneComponentRecord {
    data_record(
        MOTION,
        ComponentValue::map([("amount", ComponentValue::U64(amount))]),
    )
}

fn sensor(target: SceneEntityId) -> SceneComponentRecord {
    data_record(
        SENSOR,
        ComponentValue::map([(
            "targets",
            ComponentValue::List(vec![
                ComponentValue::EntityReference(EntityReference::SceneLocal { entity: target }),
                ComponentValue::Null,
            ]),
        )]),
    )
}

fn named(entity: &str, name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(entity)).with_component(
        component(NAME),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String(name.into()),
        ),
    )
}

fn actors(amount: u64) -> SceneDocument {
    SceneDocument::from_ordered_roots([
        named("actor", "Actor")
            .with_component(component(MOTION), motion(amount))
            .with_component(component(SENSOR), sensor(id("friend"))),
        named("friend", "Friend"),
    ])
}

struct Definition {
    bindings: SceneComponentBindings,
    bundle: AuthoringSchemaBundle,
}

impl Definition {
    fn new() -> Self {
        let motion = ComponentSchema::new(component(MOTION), "Motion", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
            .with_authoring_initial_value(motion(2).value);
        let sensor = ComponentSchema::new(component(SENSOR), "Sensor", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("targets"),
                "Targets",
                ComponentFieldPath::from_fields(["targets"]),
                ComponentValueShape::list(ComponentValueShape::optional(
                    ComponentValueShape::EntityRef,
                )),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)])
            .with_authoring_initial_value(ComponentValue::map([(
                "targets",
                ComponentValue::List(Vec::new()),
            )]));
        // Each independently admitted owner has one data schema and no Rust component type.
        let owners = [(MOTION, motion), (SENSOR, sensor)].map(|(name, schema)| {
            ComponentSchemaOwnerSnapshot::admit(
                owner(name),
                ComponentSchemaCatalog {
                    components: vec![schema],
                    ..Default::default()
                },
                None,
            )
            .unwrap()
        });
        let native = AuthoringSchemaBundle::from_trusted_provider_definitions([
            SCENE_COMPONENTS_SCHEMA_PROVIDER,
        ])
        .unwrap();
        let bundle = AuthoringSchemaBundle::new(
            native
                .owners()
                .iter()
                .chain(owners.iter())
                .map(AuthoringSchemaBundleOwner::from),
        )
        .unwrap();
        let mut registry = ComponentRegistry::new();
        SCENE_COMPONENTS_SCHEMA_PROVIDER
            .register_or_validate_into(&mut registry)
            .unwrap();
        registry.freeze().unwrap();
        Self {
            bindings: SceneComponentBindings::new(registry.snapshot().unwrap(), owners).unwrap(),
            bundle,
        }
    }

    fn read_scene(&self, path: &Path, source: &str) -> SceneDocument {
        let bytes = fs::read(path).unwrap();
        let candidate = if path.extension().unwrap() == "ron" {
            SceneDocumentCandidate::decode_ron_bytes(&bytes)
        } else {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        }
        .unwrap();
        // Metadata-only decoding preserves the complete source. Runtime eligibility is checked
        // by the strict mixed compiler below, never by an authoring preview projection.
        candidate
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path(source).unwrap()),
                &self.bundle,
                &[],
            )
            .unwrap()
            .document()
            .clone()
    }

    fn compile(
        &self,
        document: &SceneDocument,
        resolver: &InMemoryPrefabSourceResolver,
        source: &str,
    ) -> ExpandedSceneCandidate {
        let report = self.bindings.expand_prefabs(document, resolver);
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        report
            .candidate
            .unwrap()
            .bind_source(source_key(source, SourceDocumentKind::Scene))
    }
}

fn source_key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "62082771-8f56-4497-8dc7-62be850de5ba".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn spawn(
    bindings: &SceneComponentBindings,
    world: &mut World,
    candidate: &ExpandedSceneCandidate,
    slot: &str,
) -> SpawnedSceneInstance {
    let report = bindings
        .prepare_scene(
            world,
            candidate,
            &SceneOccurrenceKey::new("consumer.scene-data", slot).unwrap(),
            None,
        )
        .unwrap()
        .commit(world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.instance.unwrap()
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, id: &SceneEntityId) -> Entity {
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected a live Scene entity: {lookup:?}"),
    }
}

fn export(bindings: &SceneComponentBindings, world: &World) -> SceneExportOutput {
    let report = bindings.export_scene_with_options(world, SceneExportOptions::default(), None);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.into_output().unwrap()
}

fn exported<'a>(
    output: &'a SceneExportOutput,
    instance: &SpawnedSceneInstance,
    id: &SceneEntityId,
) -> &'a SceneEntityRecord {
    let mapped = output.remap.get(&instance.locator(id).unwrap()).unwrap();
    output
        .document
        .entities
        .iter()
        .find(|entity| &entity.id == mapped)
        .unwrap()
}

#[test]
fn source_attach_edit_undo_save_reopen_then_runtime_replace_and_retire() {
    let definition = Definition::new();
    let initial =
        SceneDocument::from_ordered_roots([named("actor", "Actor"), named("friend", "Friend")]);
    let project = Project::new(&definition.bundle, &initial);
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let attach = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: id("actor"),
            component: component(MOTION),
            value: motion(2),
        },
        ScenePatchOperation::AddComponent {
            entity: id("actor"),
            component: component(SENSOR),
            value: data_record(
                SENSOR,
                definition
                    .bindings
                    .schema(&component(SENSOR))
                    .unwrap()
                    .authoring_initial_value()
                    .unwrap()
                    .clone(),
            ),
        },
        ScenePatchOperation::SetField {
            entity: id("actor"),
            component: component(SENSOR),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("targets"),
            value: sensor(id("friend")).value.field("targets").unwrap().clone(),
        },
    ]);
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor.prepare_authoring_edit(&basis, &attach).unwrap();
    applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    assert_eq!(
        definition.read_scene(&project.root().join(SCENE), SCENE),
        initial
    );

    let edit = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("actor"),
        component: component(MOTION),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("amount"),
        value: ComponentValue::U64(7),
    }]);
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor.prepare_authoring_edit(&basis, &edit).unwrap();
    let ScenePatchOperation::SetField { value, .. } = &prepared.inverse().operations[0] else {
        panic!("the source owner must prepare an exact field inverse");
    };
    assert_eq!(value, &ComponentValue::U64(2));
    applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    for redo in [false, true] {
        let base_revision = editor.document_query_basis(document).unwrap().revision;
        let mutation = if redo {
            EditorDocumentMutation::Redo {
                document,
                base_revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document,
                base_revision,
            }
        };
        applied(dispatch(&mut editor, EditorAction::Mutate(mutation)));
        let basis = editor.document_query_basis(document).unwrap();
        let inverse = editor.prepare_authoring_edit(&basis, &edit).unwrap();
        let ScenePatchOperation::SetField { value, .. } = &inverse.inverse().operations[0] else {
            panic!("the source owner must prepare an exact field inverse");
        };
        assert_eq!(value, &ComponentValue::U64(if redo { 7 } else { 2 }));
    }
    assert!(
        editor
            .active_authoring_unavailable_record_manifest()
            .unwrap()
            .entries()
            .is_empty()
    );
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    wait(&mut editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
            if saved == document && revision == base_revision
    ));
    let saved_bytes = fs::read(project.root().join(SCENE)).unwrap();
    let saved = definition.read_scene(&project.root().join(SCENE), SCENE);
    assert_eq!(saved, actors(7));
    let attempt = editor.current_frame().persistence().attempt().unwrap();
    dispatch(
        &mut editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    wait(&mut editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    let reopened = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .document(reopened)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    let reopened_basis = editor.document_query_basis(reopened).unwrap();
    let observed = editor
        .prepare_authoring_edit(&reopened_basis, &edit)
        .unwrap();
    assert_eq!(observed.inverse(), &edit);
    drop(observed);

    let candidate = definition.compile(&saved, &InMemoryPrefabSourceResolver::new(), SCENE);
    let bindings = &definition.bindings;
    let mut world = World::new();
    let current = spawn(bindings, &mut world, &candidate, "authored");
    let entity = resolve(&world, &current, &id("actor"));
    let sibling = bindings
        .data_component(&world, entity, &component(SENSOR))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(bindings.data_components(&world, entity).unwrap().count(), 2);
    bindings
        .set_data_component(
            &mut world,
            entity,
            &component(MOTION),
            motion(9).value,
            None,
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap();
    assert_eq!(
        bindings
            .data_component(&world, entity, &component(SENSOR))
            .unwrap(),
        Some(&sibling)
    );
    assert_eq!(
        exported(&export(bindings, &world), &current, &id("actor")).components[&component(MOTION)],
        motion(9)
    );
    assert!(
        bindings
            .remove_data_component(&mut world, entity, &component(MOTION))
            .unwrap()
    );
    assert!(
        bindings
            .data_component(&world, entity, &component(MOTION))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        bindings
            .data_component(&world, entity, &component(SENSOR))
            .unwrap(),
        Some(&sibling)
    );
    assert_eq!(world.get::<Name>(entity), Some(&Name::new("Actor")));
    assert_eq!(fs::read(project.root().join(SCENE)).unwrap(), saved_bytes);
    assert_eq!(candidate.document(), &saved);
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .document(reopened)
            .unwrap()
            .history
            .undo_depth,
        0
    );

    let report = bindings
        .prepare_scene_replacement(&mut world, &candidate, &current, None)
        .unwrap()
        .commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let successor = report.instance.unwrap();
    let replacement = resolve(&world, &successor, &id("actor"));
    assert_ne!(entity, replacement);
    assert_ne!(
        current.locator(&id("actor")),
        successor.locator(&id("actor"))
    );
    assert_eq!(
        bindings
            .data_component(&world, replacement, &component(MOTION))
            .unwrap(),
        Some(&motion(7))
    );
    assert_eq!(
        bindings
            .data_component(&world, replacement, &component(SENSOR))
            .unwrap(),
        Some(&sibling)
    );
    assert!(
        bindings
            .set_data_component(
                &mut world,
                entity,
                &component(MOTION),
                motion(99).value,
                None,
                ComponentValueMaterializationLimits::default(),
            )
            .is_err()
    );
    assert!(retire_and_despawn_scene_instance(&mut world, &current).is_err());
    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &successor).unwrap(),
        2
    );
    assert!(
        bindings
            .data_component(&world, replacement, &component(SENSOR))
            .is_err()
    );
    assert!(export(bindings, &world).document.entities.is_empty());
    close(&mut editor);
}

#[test]
fn disk_prefab_occurrences_export_current_membership_and_retire_independently() {
    let definition = Definition::new();
    let prefab_scene = actors(2);
    let prefab = PrefabDocument::new(prefab_scene.roots, prefab_scene.entities);
    let anchor = |name| {
        let mut entity = named(name, name);
        entity.prefab = Some(PrefabInstance {
            source: AssetRef::path(PREFAB).unwrap(),
            overrides: None,
        });
        entity
    };
    let source = SceneDocument::from_ordered_roots([
        anchor("left"),
        anchor("right"),
        named("disposable", "Disposable"),
    ]);
    let project = Project::new(&definition.bundle, &source);
    fs::write(project.root().join(PREFAB), prefab.to_ron_string().unwrap()).unwrap();
    let prefab_bytes = fs::read(project.root().join(PREFAB)).unwrap();
    let decoded = PrefabDocumentCandidate::decode_ron_bytes(&prefab_bytes)
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path(PREFAB).unwrap()),
            &definition.bundle,
            &[],
        )
        .unwrap();
    assert_eq!(decoded.document(), &prefab);
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        AssetRef::path(PREFAB).unwrap(),
        source_key(PREFAB, SourceDocumentKind::Prefab),
        decoded.document().clone(),
    );
    let source = definition.read_scene(&project.root().join(SCENE), SCENE);
    let candidate = definition.compile(&source, &resolver, SCENE);
    let projected = |anchor: &str, local: &str| {
        candidate
            .provenance()
            .iter()
            .find(|(_, provenance)| {
                provenance.local_entity_id() == &id(local)
                    && provenance
                        .projection_path()
                        .is_some_and(|path| path.root_anchor() == &id(anchor))
            })
            .unwrap()
            .0
            .clone()
    };
    let left = projected("left", "actor");
    let right = projected("right", "actor");
    let left_friend = projected("left", "friend");
    let right_friend = projected("right", "friend");
    assert_ne!(left, right);
    assert_ne!(left_friend, right_friend);
    let bindings = &definition.bindings;
    let mut world = World::new();
    let first = spawn(bindings, &mut world, &candidate, "prefabs");
    let left_entity = resolve(&world, &first, &left);
    let right_entity = resolve(&world, &first, &right);
    assert_ne!(left_entity, right_entity);
    assert_ne!(first.locator(&left), first.locator(&right));
    for (actor, friend) in [(&left, &left_friend), (&right, &right_friend)] {
        let entity = resolve(&world, &first, actor);
        assert_eq!(
            bindings
                .data_component(&world, entity, &component(SENSOR))
                .unwrap(),
            Some(&sensor(friend.clone()))
        );
        assert_ne!(entity, resolve(&world, &first, friend));
    }
    let initial_count = export(bindings, &world).document.entities.len();
    assert_eq!(initial_count, 7);
    let disposable = id("disposable");
    let retired_locator = first.locator(&disposable).unwrap();
    let retired = resolve(&world, &first, &disposable);
    retire_and_despawn_scene_entity(&mut world, retired).unwrap();
    assert!(
        bindings
            .remove_data_component(&mut world, left_entity, &component(MOTION))
            .unwrap()
    );
    world.get_mut::<Name>(left_entity).unwrap().0 = "Runtime actor".into();
    assert_eq!(
        bindings
            .data_components(&world, left_entity)
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        bindings
            .data_component(&world, right_entity, &component(MOTION))
            .unwrap(),
        Some(&motion(2))
    );

    let later_path = "scenes/later.scene.json";
    fs::write(
        project.root().join(later_path),
        actors(30).to_json_string().unwrap(),
    )
    .unwrap();
    let later_source = definition.read_scene(&project.root().join(later_path), later_path);
    let later_candidate = definition.compile(&later_source, &resolver, later_path);
    let later = spawn(bindings, &mut world, &later_candidate, "later");
    let output = export(bindings, &world);
    assert_eq!(output.document.entities.len(), initial_count - 1 + 2);
    assert!(output.remap.get(&retired_locator).is_none());
    let left_output = exported(&output, &first, &left);
    assert!(!left_output.components.contains_key(&component(MOTION)));
    assert_eq!(
        left_output.components[&component(NAME)].value,
        ComponentValue::String("Runtime actor".into())
    );
    for (actor, friend) in [(&left, &left_friend), (&right, &right_friend)] {
        let mapped_friend = output.remap.get(&first.locator(friend).unwrap()).unwrap();
        assert_eq!(
            exported(&output, &first, actor).components[&component(SENSOR)],
            sensor(mapped_friend.clone())
        );
    }
    assert_eq!(
        exported(&output, &first, &right).components[&component(MOTION)],
        motion(2)
    );
    assert_eq!(
        exported(&output, &later, &id("actor")).components[&component(MOTION)],
        motion(30)
    );
    let export_path = "scenes/live.scene.ron";
    fs::write(
        project.root().join(export_path),
        output.document.to_ron_string().unwrap(),
    )
    .unwrap();
    let reopened = definition.read_scene(&project.root().join(export_path), export_path);
    assert_eq!(reopened, output.document);
    let replay_candidate =
        definition.compile(&reopened, &InMemoryPrefabSourceResolver::new(), export_path);
    let mut replay = World::new();
    let replay_instance = spawn(bindings, &mut replay, &replay_candidate, "replay");
    let replay_left = resolve(&replay, &replay_instance, &left_output.id);
    assert_eq!(
        bindings
            .data_component(&replay, replay_left, &component(SENSOR))
            .unwrap(),
        left_output.components.get(&component(SENSOR))
    );
    assert_eq!(
        world.get::<Name>(left_entity),
        replay.get::<Name>(replay_left)
    );
    assert_eq!(
        retire_and_despawn_scene_instance(&mut replay, &replay_instance).unwrap(),
        8
    );
    assert_eq!(fs::read(project.root().join(PREFAB)).unwrap(), prefab_bytes);
    assert_eq!(candidate.document().entities.len(), initial_count);

    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &first).unwrap(),
        initial_count - 1
    );
    assert!(
        bindings
            .data_component(&world, left_entity, &component(SENSOR))
            .is_err()
    );
    assert!(
        bindings
            .data_component(&world, right_entity, &component(MOTION))
            .is_err()
    );
    let remaining = export(bindings, &world);
    assert_eq!(remaining.document.entities.len(), 2);
    assert_eq!(
        exported(&remaining, &later, &id("actor")).components[&component(MOTION)],
        motion(30)
    );
    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &later).unwrap(),
        2
    );
    assert!(export(bindings, &world).document.entities.is_empty());
    assert!(
        world
            .query::<&SceneEntitySource>()
            .iter(&world)
            .next()
            .is_none()
    );
}

fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(outcome, EditorActionOutcome::Workspace(ref report) if report.applied),
        "{outcome:?}"
    );
}

fn dispatch(editor: &mut EditorProjectSession, action: EditorAction) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    report.into_outcomes().into_iter().next().unwrap()
}

fn wait(editor: &mut EditorProjectSession, ready: impl Fn(&EditorProjectSession) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "public Editor operation did not settle"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "public Editor close did not settle"
        );
        std::thread::yield_now();
    }
}

struct Project(tempfile::TempDir);

impl Project {
    fn new(bundle: &AuthoringSchemaBundle, scene: &SceneDocument) -> Self {
        let project = Self(tempfile::tempdir().unwrap());
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(project.root().join(directory)).unwrap();
        }
        fs::write(
            project.root().join("nara.toml"),
            concat!(
                "schema_version = 1\n[project]\n",
                "source_universe = '62082771-8f56-4497-8dc7-62be850de5ba'\n",
                "native_document_identity = 'project_location'\nname = 'Scene data consumer'\n",
                "[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n",
                "[startup]\ndefault_scene = 'startup.scene.json'\n",
                "[authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n",
                "[runtime]\npreset = 'local-headless'\n",
                "[capabilities]\nrequested = ['runtime-2d']\n",
            ),
        )
        .unwrap();
        fs::write(
            project.root().join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(project.root().join(SCENE), scene.to_json_string().unwrap()).unwrap();
        project
    }

    fn root(&self) -> &Path {
        self.0.path()
    }

    fn open(&self) -> EditorProjectSession {
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(self.root())
                .unwrap()
        };
        #[cfg(not(windows))]
        let directory = fs::File::open(self.root()).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let mut host = EditorProjectOpenHost::new();
        let mut attempt = host.begin(capability, None);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(editor) => return editor,
                EditorProjectOpenProgress::Pending(next) => attempt = next,
                EditorProjectOpenProgress::Rejected(failure) => {
                    panic!("{:?}", failure.diagnostics())
                }
            }
            assert!(
                Instant::now() < deadline,
                "public Editor open did not settle"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
