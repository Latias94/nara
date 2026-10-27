//! Actual standalone C# proposals through the production, window-free Project Session.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_engine::project_host::{
    EditorProjectSession, EditorSourceToolCommand, EditorSourceToolDefinition,
};
use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue,
};
use nara_scene::{
    AuthoringPropertyEdit, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
    EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorEntityQuery,
    EditorEntityQueryLimits, EditorEntityQueryPosition, EditorPropertyTarget,
    EditorSourceToolAction, EditorSourceToolActionOutcome, EditorSourceToolActivationId,
    EditorSourceToolCommit, EditorSourceToolError, EditorSourceToolIntent,
    EditorSourceToolRequestId, EditorSourceToolView,
};

use super::{
    authoring_project::{
        Project, applied, assert_history, assert_value, close, replay, save, schema_bundle,
    },
    scene_runtime::{Definition, Runtime, id},
};

const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";
const TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
pub(super) fn approved_managed_host() -> nara_engine::project_host::EditorSourceToolManagedHost {
    use nara_engine::{
        fs::ContentDigest,
        project_host::{
            EditorSourceToolManagedHost, EditorSourceToolManagedHostDigests,
            open_explicit_product_project_root,
        },
    };
    let dotnet = PathBuf::from(std::env::var_os("NARA_SOURCE_TOOL_DOTNET").unwrap());
    let bootstrap = PathBuf::from(std::env::var_os("NARA_SOURCE_TOOL_BOOTSTRAP_DIR").unwrap());
    let digest = |path: PathBuf| ContentDigest::of_bytes(&std::fs::read(path).unwrap());
    EditorSourceToolManagedHost::new(
        open_explicit_product_project_root(dotnet.parent().unwrap()).unwrap(),
        open_explicit_product_project_root(&bootstrap).unwrap(),
        EditorSourceToolManagedHostDigests {
            launcher: digest(dotnet),
            bootstrap: digest(bootstrap.join("Nara.SourceToolHost.dll")),
            deps: digest(bootstrap.join("Nara.SourceToolHost.deps.json")),
            runtimeconfig: digest(bootstrap.join("Nara.SourceToolHost.runtimeconfig.json")),
        },
        &std::env::var("NARA_SOURCE_TOOL_FRAMEWORK").unwrap(),
        TIMEOUT,
    )
    .unwrap()
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
#[test]
fn csharp_installed_images_use_original_bytes_and_failed_update_preserves_the_session() {
    use super::local_release::installed_variant;
    let definition = definition();
    let host = approved_managed_host();
    let project = Project::new(&schema_bundle(&definition.owner), &original(), "json");
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let (directory, installation) = installed_variant("V1");
    let v1 = host.approve_installation(installation).unwrap();
    // No installation file is needed after approval. The actual child must execute captured
    // bytes even though none of those package paths still exists during probe or proposal.
    drop(directory);
    let view = probe(&mut editor, v1);
    assert!(view.attempt_error.is_none(), "{view:?}");
    let activation = active_id(&editor);
    attach(&mut editor, document);
    let basis = editor.document_query_basis(document).unwrap();
    let request = request_review(
        &mut editor,
        &basis,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        1,
    );
    let disk = std::fs::read(project.scene_path()).unwrap();
    let (directory, invalid) = installed_variant("INVALID");
    let invalid = host.approve_installation(invalid).unwrap();
    drop(directory);
    let view = probe(&mut editor, invalid);
    assert!(
        matches!(view.attempt_error, Some(EditorSourceToolError::Declaration)),
        "{view:?}"
    );
    assert_eq!(active_id(&editor), activation);
    assert_eq!(editor.source_tool_review().unwrap().request(), &request);
    assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
    accept_review(&mut editor, document, &request);
    assert_stats(&editor, document, "enemy", 101);
    let basis = editor.document_query_basis(document).unwrap();
    let old_review = request_review(
        &mut editor,
        &basis,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        40,
    );
    let (directory, v2) = installed_variant("V2");
    let v2 = host.approve_installation(v2).unwrap();
    drop(directory);
    let view = probe(&mut editor, v2);
    assert!(view.attempt_error.is_none(), "{view:?}");
    assert_ne!(active_id(&editor), activation);
    assert!(editor.source_tool_review().is_none());
    assert!(matches!(
        editor.accept_source_tool_review(&old_review),
        Err(EditorSourceToolError::StaleRequest)
    ));
    let request = request_review(
        &mut editor,
        &basis,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        1,
    );
    accept_review(&mut editor, document, &request);
    assert_stats(&editor, document, "enemy", 103);
    assert_history(&editor, document, 3, 0);
    replay(&mut editor, document, false);
    assert_stats(&editor, document, "enemy", 101);
    replay(&mut editor, document, true);
    assert_stats(&editor, document, "enemy", 103);
    save(&mut editor, document);
    close(&mut editor);
    let mut reopened = project.open();
    assert!(reopened.source_tool_view().active.is_none());
    let document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    assert_stats(&reopened, document, "enemy", 103);
    close(&mut reopened);
}

fn kind(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn original() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("enemy")).with_component(
            kind(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guard".into()),
            ),
        ),
        SceneEntityRecord::new(id("guide")).with_component(
            kind(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guide".into()),
            ),
        ),
    ])
}

fn tool_directory(variant: &str) -> PathBuf {
    std::env::var_os(format!("NARA_SOURCE_TOOL_{variant}_DIR"))
        .expect("Run python tests/fixtures/behavior-languages/run.py")
        .into()
}

fn definition() -> Arc<Definition> {
    let bytes = std::fs::read(tool_directory("V1").join("enemy-stats.catalog.json")).unwrap();
    let catalog = ComponentSchemaCatalog::from_json_bytes(&bytes).unwrap();
    Definition::from_catalog("trial.enemy-stats", catalog).unwrap()
}

fn tool_definition(
    definition: &Definition,
    variant: &str,
    fault: Option<&str>,
    timeout: Duration,
) -> EditorSourceToolDefinition {
    let directory = tool_directory(variant);
    let executable = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_DOTNET")
            .expect("Runner selects the absolute dotnet executable"),
    );
    assert!(executable.is_absolute());
    assert!(directory.is_absolute());
    let mut arguments = vec![directory.join("EnemyStatsTool.dll").into_os_string()];
    if let Some(fault) = fault {
        arguments.extend([OsString::from("--fault"), OsString::from(fault)]);
    }
    let command = EditorSourceToolCommand::new(executable, arguments, directory, timeout).unwrap();
    EditorSourceToolDefinition::new(command, definition.owner.clone(), kind(STATS)).unwrap()
}

fn parameters(bonus: i64, fault: Option<&str>) -> BTreeMap<String, ComponentValue> {
    let mut parameters = BTreeMap::from([("bonus".into(), ComponentValue::I64(bonus))]);
    if let Some(fault) = fault {
        parameters.insert("fault".into(), ComponentValue::String(fault.into()));
    }
    parameters
}

fn settle(editor: &mut EditorProjectSession) -> EditorSourceToolView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        let view = editor.source_tool_view();
        if !view.probing && !view.working && !view.retiring {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Source child or its physical IO custody did not return: {view:?}",
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn probe(
    editor: &mut EditorProjectSession,
    definition: EditorSourceToolDefinition,
) -> EditorSourceToolView {
    let attempt = editor.enable_source_tool(definition).unwrap();
    assert_eq!(editor.source_tool_view().attempt.as_ref(), Some(&attempt));
    let view = settle(editor);
    assert_eq!(view.attempt.as_ref(), Some(&attempt));
    view
}

fn enable(editor: &mut EditorProjectSession, definition: &Definition, fault: Option<&str>) {
    let view = probe(editor, tool_definition(definition, "V1", fault, TIMEOUT));
    assert!(
        view.active.is_some() && view.attempt_error.is_none() && view.request_error.is_none(),
        "{view:?}",
    );
}

fn active_id(editor: &EditorProjectSession) -> EditorSourceToolActivationId {
    editor.source_tool_view().active.unwrap().id().clone()
}

fn source_action(
    editor: &mut EditorProjectSession,
    action: EditorSourceToolAction,
) -> EditorSourceToolActionOutcome {
    let frame = editor.current_frame();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::SourceTool(action)],
        ))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    let [EditorActionOutcome::SourceTool(outcome)] = report.outcomes() else {
        panic!("Session lost the source-tool action outcome: {report:?}");
    };
    outcome.clone()
}

fn finish_request(
    editor: &mut EditorProjectSession,
    request: &EditorSourceToolRequestId,
) -> EditorSourceToolView {
    let view = settle(editor);
    assert_eq!(view.request.as_ref(), Some(request));
    view
}

fn request_review(
    editor: &mut EditorProjectSession,
    basis: &EditorDocumentQueryBasis,
    intent: EditorSourceToolIntent,
    bonus: i64,
) -> EditorSourceToolRequestId {
    let retained_attempt_failure = editor.source_tool_view().attempt_error.is_some();
    let activation = active_id(editor);
    let request = editor
        .request_source_tool(&activation, basis, intent, parameters(bonus, None))
        .unwrap();
    let view = finish_request(editor, &request);
    assert!(
        view.active.is_some() && view.review.is_some() && view.request_error.is_none(),
        "{view:?}",
    );
    assert_eq!(
        view.attempt_error.is_some(),
        retained_attempt_failure,
        "{view:?}"
    );
    assert_eq!(editor.source_tool_review().unwrap().request(), &request);
    request
}

fn accept_review(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    request: &EditorSourceToolRequestId,
) -> EditorSourceToolCommit {
    let (inverse, created) = {
        let review = editor.source_tool_review().unwrap();
        assert_eq!(review.request(), request);
        assert!(!review.title().is_empty());
        assert!(!review.patch().operations.is_empty());
        assert!(!review.inverse().operations.is_empty());
        (review.inverse().clone(), review.created_entity().cloned())
    };
    let committed = editor.accept_source_tool_review(request).unwrap();
    assert!(committed.report.applied, "{:?}", committed.report);
    assert_eq!(committed.report.document, Some(document));
    assert_eq!(committed.created_entity, created);
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&inverse),
    );
    assert!(editor.source_tool_review().is_none());
    editor.drive_editor_frame(Duration::ZERO);
    committed
}

fn attach(editor: &mut EditorProjectSession, document: EditorDocumentId) {
    let basis = editor.document_query_basis(document).unwrap();
    let request = request_review(
        editor,
        &basis,
        EditorSourceToolIntent::Attach {
            entity: id("enemy"),
        },
        0,
    );
    assert!(
        accept_review(editor, document, &request)
            .created_entity
            .is_none()
    );
    assert_stats(editor, document, "enemy", 100);
    assert_history(editor, document, 1, 0);
}

fn assert_stats(
    editor: &EditorProjectSession,
    document: EditorDocumentId,
    entity: &str,
    health: i64,
) {
    for (field, value) in [("health", health), ("armor", 10)] {
        assert_value(
            editor,
            document,
            entity,
            STATS,
            field,
            ComponentValue::I64(value),
        );
    }
}

fn entities(editor: &EditorProjectSession, document: EditorDocumentId) -> Vec<SceneEntityId> {
    let page = editor
        .query_document_entities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(
                editor.document_query_basis(document).unwrap(),
            ),
            limits: EditorEntityQueryLimits::new(16, 16).unwrap(),
        })
        .unwrap();
    assert!(page.next.is_none());
    page.entities
}

fn assert_unchanged(
    editor: &EditorProjectSession,
    project: &Project,
    basis: &EditorDocumentQueryBasis,
    history: (usize, usize),
    disk: &[u8],
) {
    assert_eq!(&editor.document_query_basis(basis.document).unwrap(), basis);
    assert_history(editor, basis.document, history.0, history.1);
    assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
}

fn rename(editor: &mut EditorProjectSession, document: EditorDocumentId, name: &str) {
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_authoring_property_edit(
            &basis,
            &[AuthoringPropertyEdit {
                entity: id("enemy"),
                component: kind(NAME),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new("value")),
                value: ComponentValue::String(name.into()),
            }],
        )
        .unwrap();
    applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    assert_ne!(
        editor.document_query_basis(document).unwrap().revision,
        basis.revision
    );
    assert_value(
        editor,
        document,
        "enemy",
        NAME,
        "value",
        ComponentValue::String(name.into()),
    );
}

#[test]
fn csharp_source_tool_review_history_save_reopen_and_public_rust_data_access() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let disk = std::fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        assert!(editor.source_tool_view().active.is_none());
        assert!(editor.source_tool_review().is_none());
        enable(&mut editor, &definition, None);

        let basis = editor.document_query_basis(document).unwrap();
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Attach {
                entity: id("enemy"),
            },
            0,
        );
        assert!(
            editor
                .source_tool_review()
                .unwrap()
                .created_entity()
                .is_none()
        );
        assert_unchanged(&editor, &project, &basis, (0, 0), &disk);
        assert!(
            accept_review(&mut editor, document, &request)
                .created_entity
                .is_none()
        );
        assert_stats(&editor, document, "enemy", 100);
        assert_history(&editor, document, 1, 0);

        let basis = editor.document_query_basis(document).unwrap();
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            25,
        );
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);
        accept_review(&mut editor, document, &request);
        assert_stats(&editor, document, "enemy", 125);
        assert_history(&editor, document, 2, 0);
        replay(&mut editor, document, false);
        assert_stats(&editor, document, "enemy", 100);
        assert_history(&editor, document, 1, 1);
        replay(&mut editor, document, true);
        assert_stats(&editor, document, "enemy", 125);
        assert_history(&editor, document, 2, 0);

        let basis = editor.document_query_basis(document).unwrap();
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Spawn {
                name: "Reinforcement".into(),
            },
            50,
        );
        let prepared_id = editor
            .source_tool_review()
            .unwrap()
            .created_entity()
            .unwrap()
            .clone();
        assert_unchanged(&editor, &project, &basis, (2, 0), &disk);
        assert_eq!(entities(&editor, document), vec![id("enemy"), id("guide")]);
        let committed = accept_review(&mut editor, document, &request);
        assert_eq!(committed.created_entity.as_ref(), Some(&prepared_id));
        assert_stats(&editor, document, prepared_id.as_str(), 150);
        assert_history(&editor, document, 3, 0);
        replay(&mut editor, document, false);
        assert_history(&editor, document, 2, 1);
        assert_eq!(entities(&editor, document), vec![id("enemy"), id("guide")]);
        assert_stats(&editor, document, "enemy", 125);
        replay(&mut editor, document, true);
        assert_history(&editor, document, 3, 0);
        let mut expected_ids = vec![id("enemy"), id("guide"), prepared_id.clone()];
        expected_ids.sort();
        assert_eq!(entities(&editor, document), expected_ids);
        assert_stats(&editor, document, prepared_id.as_str(), 150);
        for (entity, name) in [
            ("enemy", "Guard"),
            ("guide", "Guide"),
            (prepared_id.as_str(), "Reinforcement"),
        ] {
            assert_value(
                &editor,
                document,
                entity,
                NAME,
                "value",
                ComponentValue::String(name.into()),
            );
        }
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
        save(&mut editor, document);
        let saved = project.read();
        assert_eq!(saved.entities.len(), 3);
        assert_eq!(
            saved.roots,
            vec![id("enemy"), id("guide"), prepared_id.clone()]
        );
        for entity in &saved.entities {
            if let Some(stats) = entity.components.get(&kind(STATS)) {
                assert_eq!(stats.owner(), Some(definition.owner.owner_id()));
            }
        }
        for entity in &original().entities {
            let persisted = saved
                .entities
                .iter()
                .find(|saved| saved.id == entity.id)
                .unwrap();
            assert_eq!(
                persisted.components[&kind(NAME)],
                entity.components[&kind(NAME)]
            );
            if entity.id == id("guide") {
                assert_eq!(persisted, entity);
            }
        }
        close(&mut editor);
        let closed = editor.source_tool_view();
        assert!(closed.active.is_none() && !closed.probing && !closed.working && !closed.retiring);
        drop(editor);

        let mut reopened = project.open();
        let document = reopened
            .current_frame()
            .workspace()
            .active_document
            .unwrap();
        assert!(reopened.source_tool_view().active.is_none());
        assert!(reopened.source_tool_review().is_none());
        assert_history(&reopened, document, 0, 0);
        assert_stats(&reopened, document, "enemy", 125);
        assert_stats(&reopened, document, prepared_id.as_str(), 150);
        assert_value(
            &reopened,
            document,
            prepared_id.as_str(),
            NAME,
            "value",
            ComponentValue::String("Reinforcement".into()),
        );
        assert_eq!(project.read(), saved);
        let mut runtime = Runtime::materialize(Arc::clone(&definition), &saved, None).unwrap();
        for (entity, health) in [(id("enemy"), 125), (prepared_id, 150)] {
            let target = runtime.target(&entity);
            let record = runtime.read(&target, &kind(STATS)).unwrap().unwrap();
            assert_eq!(record.owner(), Some(definition.owner.owner_id()));
            assert_eq!(
                record.value.field("wire_health").unwrap(),
                &ComponentValue::I64(health)
            );
            assert_eq!(
                record.value.field("wire_armor").unwrap(),
                &ComponentValue::I64(10)
            );
        }
        runtime.retire().unwrap();
        close(&mut reopened);
    }
}

#[test]
fn csharp_source_tool_rejects_complete_invalid_proposals_without_a_valid_prefix() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        enable(&mut editor, &definition, None);
        let activation = active_id(&editor);
        attach(&mut editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let disk = std::fs::read(project.scene_path()).unwrap();
        for intent in [
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            EditorSourceToolIntent::Spawn {
                name: "Rejected".into(),
            },
        ] {
            for fault in ["bad_last_value", "too_many_writes", "throw"] {
                let request = editor
                    .request_source_tool(
                        &activation,
                        &basis,
                        intent.clone(),
                        parameters(30, Some(fault)),
                    )
                    .unwrap();
                let view = finish_request(&mut editor, &request);
                assert!(view.active.is_some() && view.review.is_none(), "{view:?}");
                assert!(view.attempt_error.is_none(), "{view:?}");
                assert!(
                    matches!(
                        (fault, view.request_error.as_ref()),
                        (
                            "bad_last_value",
                            Some(
                                EditorSourceToolError::Preparation(_)
                                    | EditorSourceToolError::Draft
                            )
                        ) | ("too_many_writes", Some(EditorSourceToolError::Limit))
                            | (
                                "throw",
                                Some(EditorSourceToolError::Process("nonzero-exit"))
                            )
                    ),
                    "{fault}: {view:?}",
                );
                assert!(editor.source_tool_review().is_none());
                assert!(matches!(
                    editor.accept_source_tool_review(&request),
                    Err(EditorSourceToolError::Busy)
                ));
                assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
                assert_stats(&editor, document, "enemy", 100);
                assert_eq!(entities(&editor, document), vec![id("enemy"), id("guide")]);
            }
        }
        let request = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(i64::MAX, None),
            )
            .unwrap();
        let view = finish_request(&mut editor, &request);
        assert!(view.attempt_error.is_none(), "{view:?}");
        assert!(matches!(
            view.request_error,
            Some(EditorSourceToolError::Process("nonzero-exit"))
        ));
        assert!(view.review.is_none());
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            1,
        );
        accept_review(&mut editor, document, &request);
        assert_stats(&editor, document, "enemy", 101);
        close(&mut editor);
    }
}

#[test]
fn csharp_source_tool_fences_newest_request_original_basis_disable_and_physical_close() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        enable(&mut editor, &definition, None);
        let activation = active_id(&editor);
        attach(&mut editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let disk = std::fs::read(project.scene_path()).unwrap();

        let older = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            2,
        );
        let newer = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(3, None),
            )
            .unwrap();
        assert_ne!(older, newer);
        assert!(matches!(
            editor.accept_source_tool_review(&older),
            Err(EditorSourceToolError::StaleRequest)
        ));
        let view = finish_request(&mut editor, &newer);
        assert!(
            view.review.is_some() && view.attempt_error.is_none() && view.request_error.is_none(),
            "{view:?}",
        );
        assert_eq!(editor.source_tool_review().unwrap().request(), &newer);

        let failed_read = editor.request_source_tool(
            &activation,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("guide"),
            },
            parameters(1, None),
        );
        assert!(matches!(
            failed_read,
            Err(EditorSourceToolError::Schema | EditorSourceToolError::QueryBasis(_))
        ));
        let failed = editor.source_tool_view();
        assert!(failed.request.is_some());
        assert_ne!(failed.request.as_ref(), Some(&newer));
        assert!(failed.review.is_none());
        assert!(
            failed.attempt_error.is_none() && failed.request_error.is_some(),
            "{failed:?}"
        );
        assert!(matches!(
            editor.accept_source_tool_review(&newer),
            Err(EditorSourceToolError::StaleRequest)
        ));
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);

        enable(&mut editor, &definition, Some("hold"));
        let activation = active_id(&editor);
        let pending = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(10, None),
            )
            .unwrap();
        assert!(editor.source_tool_view().working);
        assert!(
            editor
                .request_source_tool(
                    &activation,
                    &basis,
                    EditorSourceToolIntent::Edit {
                        entity: id("guide")
                    },
                    parameters(1, None),
                )
                .is_err()
        );
        assert!(matches!(
            editor.accept_source_tool_review(&pending),
            Err(EditorSourceToolError::StaleRequest)
        ));
        let failed = settle(&mut editor);
        assert!(
            failed.review.is_none()
                && failed.attempt_error.is_none()
                && failed.request_error.is_some(),
            "{failed:?}",
        );
        assert_ne!(failed.request.as_ref(), Some(&pending));
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);

        let pending = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(40, None),
            )
            .unwrap();
        assert!(editor.source_tool_view().working);
        rename(&mut editor, document, "Owner changed");
        let changed = editor.document_query_basis(document).unwrap();
        assert_ne!(changed.revision, basis.revision);
        let view = finish_request(&mut editor, &pending);
        assert!(view.attempt_error.is_none(), "{view:?}");
        assert!(matches!(
            view.request_error,
            Some(EditorSourceToolError::QueryBasis(
                EditorDocumentQueryRejection::StaleRevision
            ))
        ));
        assert!(view.review.is_none());
        assert_unchanged(&editor, &project, &changed, (2, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);
        assert_value(
            &editor,
            document,
            "enemy",
            NAME,
            "value",
            ComponentValue::String("Owner changed".into()),
        );

        let reviewed = request_review(
            &mut editor,
            &changed,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            50,
        );
        rename(&mut editor, document, "Owner changed again");
        assert!(matches!(
            editor.accept_source_tool_review(&reviewed),
            Err(EditorSourceToolError::QueryBasis(
                EditorDocumentQueryRejection::StaleRevision
            ))
        ));
        let basis = editor.document_query_basis(document).unwrap();
        assert_unchanged(&editor, &project, &basis, (3, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);

        let pending = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(60, None),
            )
            .unwrap();
        assert!(editor.source_tool_view().working);
        editor.disable_source_tool(&activation).unwrap();
        assert!(editor.source_tool_view().active.is_none());
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&pending),
            Err(EditorSourceToolError::Disabled)
        ));
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy")
                },
                parameters(1, None),
            ),
            Err(EditorSourceToolError::Disabled)
        ));
        let disabled = settle(&mut editor);
        assert!(disabled.active.is_none() && disabled.review.is_none());
        assert!(disabled.request.is_none() && disabled.request_error.is_none());
        assert!(disabled.attempt_error.is_none());
        assert_unchanged(&editor, &project, &basis, (3, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);

        enable(&mut editor, &definition, Some("hold"));
        let activation = active_id(&editor);
        let reviewed = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            70,
        );
        editor.disable_source_tool(&activation).unwrap();
        assert!(matches!(
            editor.accept_source_tool_review(&reviewed),
            Err(EditorSourceToolError::Disabled)
        ));
        assert!(settle(&mut editor).review.is_none());
        assert_unchanged(&editor, &project, &basis, (3, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);

        enable(&mut editor, &definition, Some("hold"));
        let activation = active_id(&editor);
        let pending = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Spawn {
                    name: "Late spawn".into(),
                },
                parameters(80, None),
            )
            .unwrap();
        assert!(editor.source_tool_view().working);
        close(&mut editor);
        let closed = editor.source_tool_view();
        assert!(closed.active.is_none() && !closed.probing && !closed.working && !closed.retiring);
        assert!(closed.request.is_none() && closed.request_error.is_none());
        assert!(closed.attempt_error.is_none());
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&pending),
            Err(EditorSourceToolError::Closing)
        ));
        assert!(matches!(
            editor.request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy")
                },
                parameters(1, None),
            ),
            Err(EditorSourceToolError::Closing)
        ));
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
        assert_eq!(project.read(), original());
    }
}

#[test]
fn csharp_source_tool_requires_the_exact_document_admission_and_selected_owner() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        enable(&mut editor, &definition, None);
        let activation = active_id(&editor);
        let basis = editor.document_query_basis(document).unwrap();
        let disk = std::fs::read(project.scene_path()).unwrap();

        let mut independently_opened = project.open();
        let other_document = independently_opened
            .current_frame()
            .workspace()
            .active_document
            .unwrap();
        let other = independently_opened
            .document_query_basis(other_document)
            .unwrap();
        assert_ne!(basis.schema, other.schema);
        assert_eq!(
            basis.schema.as_ref().unwrap().schema(&kind(STATS)),
            other.schema.as_ref().unwrap().schema(&kind(STATS)),
        );
        assert_eq!(
            basis
                .schema
                .as_ref()
                .unwrap()
                .component_schema_owner(&kind(STATS)),
            other
                .schema
                .as_ref()
                .unwrap()
                .component_schema_owner(&kind(STATS)),
        );
        close(&mut independently_opened);
        let mut missing = basis.clone();
        missing.schema = None;
        let mut equivalent = basis.clone();
        equivalent.schema = other.schema;
        for rejected in [missing, equivalent] {
            assert!(matches!(
                editor.request_source_tool(
                    &activation,
                    &rejected,
                    EditorSourceToolIntent::Attach {
                        entity: id("enemy")
                    },
                    parameters(1, None),
                ),
                Err(EditorSourceToolError::QueryBasis(
                    EditorDocumentQueryRejection::StaleSchema
                ))
            ));
            assert!(!editor.source_tool_view().working);
            assert!(editor.source_tool_review().is_none());
            assert_unchanged(&editor, &project, &basis, (0, 0), &disk);
        }
        close(&mut editor);

        let catalog = definition.owner.current().clone();
        let mut changed = catalog.clone();
        changed.components[0].authoring_initial_value = Some(ComponentValue::map([
            ("wire_health", ComponentValue::I64(999)),
            ("wire_armor", ComponentValue::I64(10)),
        ]));
        for (owner, catalog) in [
            ("trial.another-owner", catalog),
            ("trial.enemy-stats", changed),
        ] {
            let admitted = Definition::from_catalog(owner, catalog).unwrap();
            let project = Project::new(&schema_bundle(&admitted.owner), &original(), extension);
            let disk = std::fs::read(project.scene_path()).unwrap();
            let mut editor = project.open();
            let document = editor.current_frame().workspace().active_document.unwrap();
            enable(&mut editor, &definition, None);
            let activation = active_id(&editor);
            let basis = editor.document_query_basis(document).unwrap();
            assert!(matches!(
                editor.request_source_tool(
                    &activation,
                    &basis,
                    EditorSourceToolIntent::Attach {
                        entity: id("enemy")
                    },
                    parameters(1, None),
                ),
                Err(EditorSourceToolError::Schema)
            ));
            assert!(!editor.source_tool_view().working);
            assert!(editor.source_tool_review().is_none());
            assert_unchanged(&editor, &project, &basis, (0, 0), &disk);
            assert_eq!(project.read(), original());
            close(&mut editor);
        }
    }
}

#[test]
fn csharp_source_tool_failed_candidates_preserve_the_predecessor_and_v2_revokes_old_review() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        enable(&mut editor, &definition, None);
        let activation = active_id(&editor);
        attach(&mut editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let disk = std::fs::read(project.scene_path()).unwrap();
        let reviewed = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            1,
        );
        let mut previous_attempt = editor.source_tool_view().attempt;
        for (variant, fault, timeout, expected) in [
            ("INVALID", None, TIMEOUT, "declaration"),
            ("V1", Some("malformed"), TIMEOUT, "protocol"),
            ("V1", Some("stdout_limit"), TIMEOUT, "stdout-limit"),
            ("V1", Some("stderr_limit"), TIMEOUT, "stderr-limit"),
            ("V1", Some("nonzero"), TIMEOUT, "nonzero-exit"),
            ("V1", Some("hold"), Duration::from_millis(60), "timed-out"),
        ] {
            let view = probe(
                &mut editor,
                tool_definition(&definition, variant, fault, timeout),
            );
            assert_ne!(view.attempt, previous_attempt);
            previous_attempt = view.attempt.clone();
            assert!(view.active.is_some() && view.review.is_some(), "{view:?}");
            assert_eq!(view.active.as_ref().unwrap().id(), &activation);
            assert!(view.request_error.is_none(), "{view:?}");
            assert!(
                match (expected, view.attempt_error.as_ref()) {
                    ("declaration", Some(EditorSourceToolError::Declaration))
                    | ("protocol", Some(EditorSourceToolError::Protocol)) => true,
                    (expected, Some(EditorSourceToolError::Process(actual))) => expected == *actual,
                    _ => false,
                },
                "{variant}/{fault:?}: {view:?}",
            );
            assert_eq!(view.request.as_ref(), Some(&reviewed));
            assert_eq!(editor.source_tool_review().unwrap().request(), &reviewed);
            assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
            assert_stats(&editor, document, "enemy", 100);
        }
        accept_review(&mut editor, document, &reviewed);
        assert_stats(&editor, document, "enemy", 101);
        let basis = editor.document_query_basis(document).unwrap();
        let failed = editor
            .request_source_tool(
                &activation,
                &basis,
                EditorSourceToolIntent::Edit {
                    entity: id("enemy"),
                },
                parameters(1, Some("throw")),
            )
            .unwrap();
        let view = finish_request(&mut editor, &failed);
        assert!(matches!(
            view.attempt_error,
            Some(EditorSourceToolError::Process("timed-out"))
        ));
        assert!(matches!(
            view.request_error,
            Some(EditorSourceToolError::Process("nonzero-exit"))
        ));
        assert_unchanged(&editor, &project, &basis, (2, 0), &disk);
        assert_stats(&editor, document, "enemy", 101);
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            1,
        );
        let view = editor.source_tool_view();
        assert!(matches!(
            view.attempt_error,
            Some(EditorSourceToolError::Process("timed-out"))
        ));
        assert!(view.request_error.is_none());
        accept_review(&mut editor, document, &request);
        assert_stats(&editor, document, "enemy", 102);
        assert_history(&editor, document, 3, 0);

        let basis = editor.document_query_basis(document).unwrap();
        let old_review = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            40,
        );
        let view = probe(
            &mut editor,
            tool_definition(&definition, "V2", None, TIMEOUT),
        );
        assert!(
            view.active.is_some() && view.attempt_error.is_none() && view.request_error.is_none(),
            "{view:?}",
        );
        assert_ne!(view.attempt, previous_attempt);
        assert!(view.request.is_none() && view.review.is_none());
        assert!(editor.source_tool_review().is_none());
        assert!(matches!(
            editor.accept_source_tool_review(&old_review),
            Err(EditorSourceToolError::StaleRequest)
        ));
        assert_unchanged(&editor, &project, &basis, (3, 0), &disk);
        let request = request_review(
            &mut editor,
            &basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            1,
        );
        assert_ne!(request, old_review);
        accept_review(&mut editor, document, &request);
        assert_stats(&editor, document, "enemy", 104);
        assert_history(&editor, document, 4, 0);
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
        close(&mut editor);
    }
}

#[test]
fn csharp_source_tool_integer_facet_shares_one_candidate_and_rejects_deferred_activation() {
    let definition = definition();
    for extension in ["json", "ron"] {
        let project = Project::new(&schema_bundle(&definition.owner), &original(), extension);
        let disk = std::fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        enable(&mut editor, &definition, None);
        attach(&mut editor, document);
        let activation = active_id(&editor);
        let basis = editor.document_query_basis(document).unwrap();
        let target = EditorPropertyTarget {
            basis: basis.clone(),
            entity: id("enemy"),
            component: kind(STATS),
            property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
        };
        let observed = editor.current_frame();
        let active = observed.source_tool().active.as_ref().unwrap();
        assert_eq!(active.id(), &activation);
        assert_eq!(active.owner(), definition.owner.owner_id());
        assert_eq!(active.component(), &kind(STATS));
        let facet = active.integer_property().unwrap();
        assert_eq!(facet.field(), &ComponentFieldId::new("health"));
        assert_eq!(facet.parameter(), "bonus");
        assert_eq!(facet.label(), "Health bonus");
        assert_eq!(facet.initial(), 0);
        assert!(active.matches_integer_property(&target));
        let mut generic = target.clone();
        generic.property = ComponentPropertyPath::new(ComponentFieldId::new("armor"));
        assert!(!active.matches_integer_property(&generic));
        assert!(matches!(
            source_action(
                &mut editor,
                EditorSourceToolAction::IntegerProperty {
                    activation: activation.clone(),
                    target: generic,
                    value: 30,
                }
            ),
            EditorSourceToolActionOutcome::Requested(Err(EditorSourceToolError::PropertyFacet)),
        ));
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
        let deferred_property = EditorSourceToolAction::IntegerProperty {
            activation: activation.clone(),
            target: target.clone(),
            value: 90,
        };
        let deferred_request = EditorSourceToolAction::Request {
            activation: activation.clone(),
            basis: basis.clone(),
            intent: EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            parameters: parameters(90, None),
        };
        let deferred_disable = EditorSourceToolAction::Disable {
            activation: activation.clone(),
        };
        let EditorSourceToolActionOutcome::Requested(Ok(request)) = source_action(
            &mut editor,
            EditorSourceToolAction::IntegerProperty {
                activation: activation.clone(),
                target: target.clone(),
                value: 25,
            },
        ) else {
            panic!("The actual admitted health facet must produce a request");
        };
        let view = finish_request(&mut editor, &request);
        assert!(view.request_error.is_none(), "{view:?}");
        let inspector = editor.current_frame();
        let panel = editor.current_frame();
        assert!(Arc::ptr_eq(&inspector, &panel));
        assert_eq!(inspector.source_tool(), panel.source_tool());
        let summary = inspector.source_tool().review.as_ref().unwrap();
        assert_eq!(summary.request, request);
        let inspector_review = editor.source_tool_review().unwrap();
        let panel_review = editor.source_tool_review().unwrap();
        assert!(std::ptr::eq(inspector_review.patch(), panel_review.patch()));
        assert!(std::ptr::eq(
            inspector_review.inverse(),
            panel_review.inverse()
        ));
        assert_eq!(summary.operations, panel_review.patch().operations.len());
        assert_eq!(summary.title.as_ref(), panel_review.title());
        let inverse = panel_review.inverse().clone();
        assert_stats(&editor, document, "enemy", 100);
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);

        let successor = probe(
            &mut editor,
            tool_definition(&definition, "V2", None, TIMEOUT),
        );
        assert!(successor.attempt_error.is_none(), "{successor:?}");
        let next_activation = successor.active.as_ref().unwrap().id().clone();
        assert_ne!(next_activation, activation);
        assert!(matches!(
            source_action(
                &mut editor,
                EditorSourceToolAction::Accept {
                    request: request.clone()
                },
            ),
            EditorSourceToolActionOutcome::Accepted(Err(EditorSourceToolError::StaleRequest))
        ));
        for deferred in [deferred_property, deferred_request] {
            assert!(matches!(
                source_action(&mut editor, deferred),
                EditorSourceToolActionOutcome::Requested(Err(
                    EditorSourceToolError::StaleActivation
                ))
            ));
        }
        assert!(matches!(
            source_action(&mut editor, deferred_disable),
            EditorSourceToolActionOutcome::Disabled(Err(EditorSourceToolError::StaleActivation))
        ));
        assert_eq!(active_id(&editor), next_activation);
        assert_unchanged(&editor, &project, &basis, (1, 0), &disk);
        assert_stats(&editor, document, "enemy", 100);

        let EditorSourceToolActionOutcome::Requested(Ok(next_request)) = source_action(
            &mut editor,
            EditorSourceToolAction::IntegerProperty {
                activation: next_activation.clone(),
                target: target.clone(),
                value: 25,
            },
        ) else {
            panic!("The successor facet must remain usable");
        };
        let view = finish_request(&mut editor, &next_request);
        assert!(view.request_error.is_none(), "{view:?}");
        let successor_inverse = editor.source_tool_review().unwrap().inverse().clone();
        assert_eq!(successor_inverse, inverse);
        let accepted = source_action(
            &mut editor,
            EditorSourceToolAction::Accept {
                request: next_request.clone(),
            },
        );
        let EditorSourceToolActionOutcome::Accepted(Ok(committed)) = accepted else {
            panic!("The shared candidate must commit exactly once");
        };
        assert!(committed.report.applied);
        assert_eq!(committed.report.document, Some(document));
        assert!(committed.created_entity.is_none());
        assert_eq!(
            committed
                .report
                .patch_report
                .as_ref()
                .unwrap()
                .inverse
                .as_ref(),
            Some(&inverse)
        );
        assert_stats(&editor, document, "enemy", 150);
        assert_history(&editor, document, 2, 0);
        assert!(matches!(
            source_action(
                &mut editor,
                EditorSourceToolAction::Accept {
                    request: next_request
                },
            ),
            EditorSourceToolActionOutcome::Accepted(Err(EditorSourceToolError::Busy))
        ));
        replay(&mut editor, document, false);
        assert_stats(&editor, document, "enemy", 100);
        replay(&mut editor, document, true);
        assert_stats(&editor, document, "enemy", 150);
        assert!(matches!(
            source_action(
                &mut editor,
                EditorSourceToolAction::Disable {
                    activation: next_activation.clone(),
                }
            ),
            EditorSourceToolActionOutcome::Disabled(Ok(()))
        ));
        assert!(editor.current_frame().source_tool().active.is_none());
        assert!(matches!(
            source_action(
                &mut editor,
                EditorSourceToolAction::IntegerProperty {
                    activation: next_activation,
                    target,
                    value: 1,
                }
            ),
            EditorSourceToolActionOutcome::Requested(Err(EditorSourceToolError::Disabled))
        ));
        assert_eq!(std::fs::read(project.scene_path()).unwrap(), disk);
        close(&mut editor);
    }
}
