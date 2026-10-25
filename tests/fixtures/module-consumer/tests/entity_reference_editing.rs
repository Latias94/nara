//! An independent component goes through ordinary editing, persistence, Check and a product run.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_engine::{
    ProductRecipe, SchemaContribution,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    ecs::{Component, Query, ResMut, Resource},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    identity::EntityReference,
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession, HeadlessProductRun, HeadlessRunIntent, HeadlessRunOutcome,
    },
    project_workflow::{ProjectCheckErrorKind, ProjectCheckRequest, check_project},
    reflect::{
        ComponentFieldId, ComponentRegistry, ComponentRegistryError, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponent,
        PersistentComponentProvider,
    },
    scene::{
        AuthoringDocumentReadiness, AuthoringFieldEdit, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntityRecord, UnavailableRecordManifest,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceRequestResult,
        EditorPersistenceResult, SceneInspectorCommand, SceneInspectorComponentView,
        SceneInspectorFieldState,
    },
};

const COMPONENT: &str = "consumer.entity-reference.Link";
const REFERENCES: &str = "consumer.entity-reference.ReferenceFields";
const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.entity-reference.schema");

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.entity-reference.Link",
    version = 1,
    alias = "Link",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Link {
    #[nara(
        id = "target",
        alias = "Target",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    target: EntityReference,
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
struct LinkOutcome(Option<EntityReference>);

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq, Eq)]
#[nara(
    id = "consumer.entity-reference.ReferenceFields",
    version = 1,
    alias = "Reference fields",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct ReferenceFields {
    #[nara(
        id = "nested",
        alias = "Nested",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    nested: BTreeMap<String, Vec<Option<EntityReference>>>,
    #[nara(
        id = "optional",
        alias = "Optional",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    optional: Option<EntityReference>,
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
struct ReferenceOutcome(Option<ReferenceFields>);

nara_engine::persistent_components! { mod types = [Link]; }

fn reference_schema() -> ComponentSchema {
    let mut schema = ReferenceFields::persistent_component_schema();
    for field in &mut schema.fields {
        field.default_value = Some(match field.id.as_str() {
            "nested" => nested_reference("target"),
            "optional" => reference("target"),
            _ => unreachable!(),
        });
    }
    schema
}

// The data bundle and native codec must share the same explicit decode defaults.
fn initial_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    let mut catalog = types::initial_catalog()?;
    catalog.components.push(reference_schema());
    Ok(catalog)
}

fn register(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    types::register(registry)?;
    registry.register_persistent_component_with_codec::<ReferenceFields, _, _>(
        reference_schema(),
        ReferenceFields::__decode_persistent_component,
        ReferenceFields::__encode_persistent_component,
    )?;
    Ok(())
}

const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.entity-reference"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.entity-reference.native", 1),
    initial_catalog,
    register,
);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("consumer.entity-reference"),
    PluginCategory::Runtime,
)
.requires_plugins(&[nara_engine::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
.provides_schema(&[PROVIDER_ID]);

#[derive(Default)]
struct LinkPlugin;
impl Plugin for LinkPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<LinkOutcome>()?.add_systems(
            CoreStage::Update,
            |links: Query<&Link>, mut outcome: ResMut<LinkOutcome>| {
                outcome.0 = links.iter().next().map(|link| link.target.clone());
            },
        )?;
        app.init_resource::<ReferenceOutcome>()?.add_systems(
            CoreStage::Update,
            |fields: Query<&ReferenceFields>, mut outcome: ResMut<ReferenceOutcome>| {
                outcome.0 = fields.iter().next().cloned();
            },
        )?;
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<LinkPlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
}
fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn reference(value: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(value) })
}
fn nested_reference(value: &str) -> ComponentValue {
    ComponentValue::map([(
        "friends",
        ComponentValue::List(vec![reference(value), ComponentValue::Null]),
    )])
}

#[test]
fn source_local_reference_edit_save_reopen_and_strict_check_share_the_public_workflow() {
    let project = Project::new();
    let mut host = EditorProjectOpenHost::new();
    let mut open = host.begin(project.capability(), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut editor = loop {
        match host.advance(open) {
            EditorProjectOpenProgress::Opened(editor) => break editor,
            EditorProjectOpenProgress::Pending(next) => open = next,
            EditorProjectOpenProgress::Rejected(failure) => panic!("{:?}", failure.diagnostics()),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    let document = editor.current_frame().workspace().active_document.unwrap();
    select_source(&mut editor);
    let baseline = editor
        .current_frame()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert!(!edit(&mut editor, "missing"));
    assert_eq!(
        editor.current_frame().workspace().document(document),
        Some(&baseline)
    );
    assert!(edit(&mut editor, "other"));
    assert_eq!(target(&editor), reference("other"));
    for (undo, expected) in [(true, "target"), (false, "other")] {
        let frame = editor.current_frame();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let mutation = if undo {
            EditorDocumentMutation::Undo {
                document,
                base_revision: scene.revision,
            }
        } else {
            EditorDocumentMutation::Redo {
                document,
                base_revision: scene.revision,
            }
        };
        assert!(
            matches!(dispatch(&mut editor, EditorAction::Mutate(mutation)), EditorActionOutcome::Workspace(report) if report.applied)
        );
        assert_eq!(target(&editor), reference(expected));
    }
    let revision = editor
        .current_frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .revision;
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision
            })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    settle(&mut editor);
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap();
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        &mut editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    assert!(edit(&mut editor, "target"));
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    settle(&mut editor);
    select_source(&mut editor);
    assert_eq!(self::target(&editor), reference("other"));
    assert!(
        !editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let report = HeadlessProductRun::new(
        &project.root,
        HeadlessRunIntent::frames(std::num::NonZeroU32::new(1).unwrap(), Duration::ZERO)
            .with_recipe(recipe()),
        Vec::new(),
    )
    .run()
    .unwrap();
    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(LinkOutcome(Some(EntityReference::SceneLocal {
            entity: id("other")
        },))),
        "{report:#?}"
    );
}

#[test]
fn nested_local_references_preserve_unavailable_records_through_history_save_and_reopen() {
    for degraded in [false, true] {
        let project = Project::with_reference_fields(degraded);
        let mut editor = open_project(&project);
        let manifest = editor
            .active_authoring_unavailable_record_manifest()
            .unwrap()
            .clone();
        assert_eq!(manifest.total_count(), usize::from(degraded));
        assert_reference_field(
            &editor,
            "nested",
            nested_reference("target"),
            SceneInspectorFieldState::Present,
            &manifest,
        );
        let document = editor.current_frame().workspace().active_document.unwrap();
        let baseline = editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .clone();
        for field in ["nested", "optional"] {
            let before = reference_fields(&editor);
            let missing = if field == "nested" {
                nested_reference("missing")
            } else {
                reference("missing")
            };
            let rejected = set_reference_field(&mut editor, field, missing);
            assert!(
                matches!(rejected, EditorActionOutcome::Workspace(report) if !report.applied && report.diagnostics.iter().any(|diagnostic| diagnostic.code().as_str() == "scene.authoring-local-reference-invalid"))
            );
            assert_eq!(
                editor.current_frame().workspace().document(document),
                Some(&baseline)
            );
            assert_eq!(reference_fields(&editor), before);
            assert_eq!(
                editor.active_authoring_unavailable_record_manifest(),
                Some(&manifest)
            );
        }

        for (reset, expected, state) in [
            (false, "other", SceneInspectorFieldState::Present),
            (true, "target", SceneInspectorFieldState::Defaulted),
        ] {
            let before = reference_fields(&editor);
            let accepted = if reset {
                remove_reference_field(&mut editor, "nested")
            } else {
                set_reference_field(&mut editor, "nested", nested_reference(expected))
            };
            assert!(matches!(accepted, EditorActionOutcome::Workspace(report) if report.applied));
            assert_reference_field(
                &editor,
                "nested",
                nested_reference(expected),
                state.clone(),
                &manifest,
            );
            reference_history(&mut editor, before, &manifest);
            save_and_reopen(&mut editor);
            assert_reference_field(
                &editor,
                "nested",
                nested_reference(expected),
                state,
                &manifest,
            );
            assert_reference_run(&project, degraded, expected, Some("target"));
        }
        close_editor(&mut editor);
    }
}

#[test]
fn optional_reference_null_and_remove_field_reset_survive_history_save_and_reopen() {
    for degraded in [false, true] {
        let project = Project::with_reference_fields(degraded);
        let mut editor = open_project(&project);
        let manifest = editor
            .active_authoring_unavailable_record_manifest()
            .unwrap()
            .clone();
        assert_eq!(manifest.total_count(), usize::from(degraded));
        assert_reference_field(
            &editor,
            "optional",
            reference("target"),
            SceneInspectorFieldState::Present,
            &manifest,
        );
        let before = reference_fields(&editor);
        let accepted = set_reference_field(&mut editor, "optional", ComponentValue::Null);
        assert!(matches!(accepted, EditorActionOutcome::Workspace(report) if report.applied));
        assert_reference_field(
            &editor,
            "optional",
            ComponentValue::Null,
            SceneInspectorFieldState::Present,
            &manifest,
        );
        reference_history(&mut editor, before, &manifest);
        save_and_reopen(&mut editor);
        assert_reference_field(
            &editor,
            "optional",
            ComponentValue::Null,
            SceneInspectorFieldState::Present,
            &manifest,
        );
        assert_reference_run(&project, degraded, "target", None);

        let before = reference_fields(&editor);
        let reset = remove_reference_field(&mut editor, "optional");
        assert!(matches!(reset, EditorActionOutcome::Workspace(report) if report.applied));
        assert_reference_field(
            &editor,
            "optional",
            reference("target"),
            SceneInspectorFieldState::Defaulted,
            &manifest,
        );
        reference_history(&mut editor, before, &manifest);
        save_and_reopen(&mut editor);
        assert_reference_field(
            &editor,
            "optional",
            reference("target"),
            SceneInspectorFieldState::Defaulted,
            &manifest,
        );
        close_editor(&mut editor);
        assert_reference_run(&project, degraded, "target", Some("target"));
    }
}

fn open_project(project: &Project) -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new();
    let mut open = host.begin(project.capability(), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(open) {
            EditorProjectOpenProgress::Opened(mut editor) => {
                select_source(&mut editor);
                return editor;
            }
            EditorProjectOpenProgress::Pending(next) => open = next,
            EditorProjectOpenProgress::Rejected(failure) => panic!("{:?}", failure.diagnostics()),
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn set_reference_field(
    editor: &mut EditorProjectSession,
    field: &str,
    value: ComponentValue,
) -> EditorActionOutcome {
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let action = EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
        document: scene.document,
        base_revision: scene.revision,
        edit: AuthoringFieldEdit::single(
            id("source"),
            ComponentTypeId::new(REFERENCES),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new(field),
            value,
        )
        .unwrap(),
    });
    dispatch(editor, action)
}

fn remove_reference_field(editor: &mut EditorProjectSession, field: &str) -> EditorActionOutcome {
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    // AuthoringFieldEdit only writes values; removal uses the ordinary Inspector command.
    let action = EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        document: scene.document,
        base_revision: scene.revision,
        command: SceneInspectorCommand::RemoveField {
            entity: id("source"),
            component: ComponentTypeId::new(REFERENCES),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new(field),
        },
    });
    dispatch(editor, action)
}

fn reference_fields(editor: &EditorProjectSession) -> SceneInspectorComponentView {
    editor
        .current_frame()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == REFERENCES)
        .unwrap()
        .clone()
}

fn assert_reference_field(
    editor: &EditorProjectSession,
    field: &str,
    value: ComponentValue,
    state: SceneInspectorFieldState,
    manifest: &UnavailableRecordManifest,
) {
    let fields = reference_fields(editor);
    let actual = fields
        .fields
        .iter()
        .find(|candidate| candidate.id.as_str() == field)
        .unwrap();
    assert_eq!(actual.value.as_ref(), Some(&value));
    assert_eq!(actual.state, state);
    assert_eq!(
        editor.active_authoring_unavailable_record_manifest(),
        Some(manifest)
    );
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    assert_eq!(
        frame
            .workspace()
            .document(document)
            .unwrap()
            .authoring_readiness,
        Some(if manifest.total_count() == 0 {
            AuthoringDocumentReadiness::Complete
        } else {
            AuthoringDocumentReadiness::Degraded
        })
    );
}

fn reference_history(
    editor: &mut EditorProjectSession,
    before: SceneInspectorComponentView,
    manifest: &UnavailableRecordManifest,
) {
    let after = reference_fields(editor);
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let history = frame.workspace().document(document).unwrap().history;
    for (undo, expected) in [(true, before), (false, after)] {
        let revision = editor
            .current_frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision;
        let mutation = if undo {
            EditorDocumentMutation::Undo {
                document,
                base_revision: revision,
            }
        } else {
            EditorDocumentMutation::Redo {
                document,
                base_revision: revision,
            }
        };
        assert!(
            matches!(dispatch(editor, EditorAction::Mutate(mutation)), EditorActionOutcome::Workspace(report) if report.applied)
        );
        assert_eq!(reference_fields(editor), expected);
        assert_eq!(
            editor.active_authoring_unavailable_record_manifest(),
            Some(manifest)
        );
        let actual = editor
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history;
        assert_eq!(actual.undo_depth, history.undo_depth - usize::from(undo));
        assert_eq!(actual.redo_depth, history.redo_depth + usize::from(undo));
    }
}

fn save_and_reopen(editor: &mut EditorProjectSession) {
    let manifest = editor
        .active_authoring_unavailable_record_manifest()
        .unwrap()
        .clone();
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let document = scene.document;
    let save = EditorAction::Persistence(EditorPersistenceAction::Save {
        document,
        base_revision: scene.revision,
    });
    assert!(matches!(
        dispatch(editor, save),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    settle(editor);
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    assert_eq!(
        editor.active_authoring_unavailable_record_manifest(),
        Some(&manifest)
    );
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        dispatch(
            editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target })
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    settle(editor);
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    let attempt = editor
        .current_frame()
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    select_source(editor);
    let frame = editor.current_frame();
    let reopened = frame.workspace().document(document).unwrap();
    assert!(!reopened.dirty);
    assert_eq!(reopened.history.undo_depth, 0);
    assert_eq!(reopened.history.redo_depth, 0);
    assert_eq!(
        editor.active_authoring_unavailable_record_manifest(),
        Some(&manifest)
    );
}

fn close_editor(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

fn assert_reference_run(project: &Project, degraded: bool, nested: &str, optional: Option<&str>) {
    let check = check_project(ProjectCheckRequest::new(project.capability(), recipe()));
    if degraded {
        let error = check.unwrap_err();
        assert_eq!(error.kind(), ProjectCheckErrorKind::Content);
        assert!(
            error
                .content_error()
                .unwrap()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.unknown-component"),
            "{error:?}"
        );
    } else {
        check.unwrap();
    }
    let report = HeadlessProductRun::new(
        &project.root,
        HeadlessRunIntent::frames(std::num::NonZeroU32::new(1).unwrap(), Duration::ZERO)
            .with_recipe(recipe()),
        Vec::new(),
    )
    .run()
    .unwrap();
    if degraded {
        assert_eq!(
            report.outcome(),
            &HeadlessRunOutcome::<ReferenceOutcome>::Failed,
            "{report:#?}"
        );
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.unknown-component"),
            "{report:#?}"
        );
    } else {
        assert_eq!(
            report.outcome(),
            &HeadlessRunOutcome::Completed(ReferenceOutcome(Some(ReferenceFields {
                nested: BTreeMap::from([(
                    "friends".to_owned(),
                    vec![
                        Some(EntityReference::SceneLocal { entity: id(nested) }),
                        None
                    ]
                )]),
                optional: optional.map(|target| EntityReference::SceneLocal { entity: id(target) }),
            }))),
            "{report:#?}"
        );
    }
}

fn edit(editor: &mut EditorProjectSession, value: &str) -> bool {
    let frame = editor.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let action = EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
        document: scene.document,
        base_revision: scene.revision,
        edit: AuthoringFieldEdit::single(
            id("source"),
            ComponentTypeId::new(COMPONENT),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("target"),
            reference(value),
        )
        .unwrap(),
    });
    matches!(dispatch(editor, action), EditorActionOutcome::Workspace(report) if report.applied)
}
fn target(editor: &EditorProjectSession) -> ComponentValue {
    let frame = editor.current_frame();
    frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|field| field.id.as_str() == "target")
        .unwrap()
        .value
        .clone()
        .unwrap()
}
fn select_source(editor: &mut EditorProjectSession) {
    let document = editor.current_frame().workspace().active_document.unwrap();
    dispatch(
        editor,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document,
            entity: Some(id("source")),
        }),
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
fn settle(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.current_frame().persistence().result().is_none() {
        assert!(Instant::now() < deadline);
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct Project {
    root: PathBuf,
}
impl Project {
    fn with_reference_fields(degraded: bool) -> Self {
        let project = Self::new();
        let fields = ReferenceFields {
            nested: BTreeMap::from([(
                "friends".to_owned(),
                vec![
                    Some(EntityReference::SceneLocal {
                        entity: id("target"),
                    }),
                    None,
                ],
            )]),
            optional: Some(EntityReference::SceneLocal {
                entity: id("target"),
            }),
        };
        let mut entities = vec![
            SceneEntityRecord::new(id("source")).with_component(
                ComponentTypeId::new(REFERENCES),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    fields.__encode_persistent_component().unwrap(),
                ),
            ),
            SceneEntityRecord::new(id("target")),
            SceneEntityRecord::new(id("other")),
        ];
        if degraded {
            entities.push(SceneEntityRecord::new(id("unrelated")).with_component(
                ComponentTypeId::new("consumer.unavailable.Future"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(9),
                    ComponentValue::map([
                        (
                            "opaque",
                            ComponentValue::String("preserve verbatim".to_owned()),
                        ),
                        (
                            "links",
                            ComponentValue::List(vec![reference("target"), ComponentValue::Null]),
                        ),
                    ]),
                ),
            ));
        }
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(entities)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-entity-ref-{}-{}",
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
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = 'ebf75b1d-134b-5966-bc8b-b7ca14e8b578'\nnative_document_identity = 'project_location'\nname = \"Entity reference consumer\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("source")).with_component(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    Link {
                        target: EntityReference::SceneLocal {
                            entity: id("target"),
                        },
                    }
                    .__encode_persistent_component()
                    .unwrap(),
                ),
            ),
            SceneEntityRecord::new(id("target")),
            SceneEntityRecord::new(id("other")),
        ]);
        fs::write(
            root.join("scenes/startup.scene.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        let project = Self { root };
        let capability = project.capability();
        let manifest = capability
            .open_file(&nara_engine::fs::RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate =
            nara_engine::project_host::ingest_project_manifest(&manifest, None).unwrap();
        let plan = nara_engine::project_host::resolve_product_recipe(&candidate, recipe()).unwrap();
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
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let directory = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            directory,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
