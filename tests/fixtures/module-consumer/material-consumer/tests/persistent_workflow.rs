use std::{collections::BTreeMap, fs, time::Duration};

use nara::{
    asset::AssetRef,
    fs::ContentDigest,
    material::{Material2dRecord, Material2dSource},
    project_host::ProjectContentMemberKind,
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{ScenePatchDocument, ScenePatchOperation},
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentMutation, EditorPersistenceAction,
        EditorPersistenceRequestResult, EditorPersistenceResult,
    },
};

#[path = "persistent_workflow/editor.rs"]
mod editor;
#[path = "persistent_workflow/project.rs"]
mod project;
#[path = "persistent_workflow/runtime.rs"]
mod runtime;

use project::{
    CARRIER, MATERIAL_PATH, Project, STARTUP, actor, indices_product, material_product,
    palette_product, parameters, recipe,
};

const REJECTED_SCENES: [&str; 4] = [
    "wrong-declaration.scene.json",
    "wrong-parameter.scene.json",
    "wrong-material-product.scene.json",
    "wrong-image-product.scene.json",
];

fn parameter_edit(value: ComponentValue) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: actor(),
        component: ComponentTypeId::new(CARRIER),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("parameters"),
        value,
    }])
}

#[test]
fn public_palette_edits_undo_redo_save_reopen_check_and_runs_its_material_only_texture() {
    let project = Project::new();
    let initial_bytes = fs::read(project.root().join(STARTUP)).unwrap();
    let initial_record = project
        .record(0.0, BTreeMap::new())
        .to_component_value()
        .unwrap();
    let edited_record = project
        .record(2.0, BTreeMap::new())
        .to_component_value()
        .unwrap();
    assert_eq!(project.initial.entities.len(), 1);
    assert_eq!(project.initial.entities[0].components.len(), 2);
    assert!(
        Material2dRecord::from_component_value(&initial_record)
            .unwrap()
            .texture_overrides()
            .is_empty(),
    );
    let source = Material2dSource::decode(
        &fs::read(project.root().join("assets").join(MATERIAL_PATH)).unwrap(),
    )
    .unwrap();
    assert_eq!(source.declaration(), &project.declaration);
    assert_eq!(source.default_parameters(), &parameters(0.0));
    assert_eq!(
        source.textures(),
        &BTreeMap::from([("palette".to_owned(), AssetRef::Product(palette_product()))]),
    );

    let mut editor = editor::open(&project);
    let document = editor.current_frame().workspace().active_document.unwrap();
    let edit = parameter_edit(edited_record.field("parameters").unwrap().clone());
    let initial_inverse = parameter_edit(initial_record.field("parameters").unwrap().clone());
    let basis = editor.document_query_basis(document).unwrap();
    let prepared = editor.prepare_authoring_edit(&basis, &edit).unwrap();
    assert_eq!(prepared.inverse(), &initial_inverse);
    editor::applied(editor.commit_authoring_edit(prepared));
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor::history(&editor, document), (1, 0));
    assert_eq!(
        fs::read(project.root().join(STARTUP)).unwrap(),
        initial_bytes
    );

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
        editor::applied(editor::dispatch(
            &mut editor,
            EditorAction::Mutate(mutation),
        ));
        let basis = editor.document_query_basis(document).unwrap();
        let observed = editor.prepare_authoring_edit(&basis, &edit).unwrap();
        assert_eq!(
            observed.inverse(),
            if redo { &edit } else { &initial_inverse },
        );
        drop(observed);
        assert_eq!(
            editor::history(&editor, document),
            if redo { (1, 0) } else { (0, 1) },
        );
        assert_eq!(
            fs::read(project.root().join(STARTUP)).unwrap(),
            initial_bytes
        );
    }
    assert!(
        editor
            .active_authoring_unavailable_record_manifest()
            .unwrap()
            .entries()
            .is_empty(),
    );
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    assert!(matches!(
        editor::dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision,
            }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. }),
    ));
    editor::wait(&mut editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
            if saved == document && revision == base_revision
    ));
    let expected = project.document(edited_record);
    let saved_bytes = fs::read(project.root().join(STARTUP)).unwrap();
    assert_ne!(saved_bytes, initial_bytes);
    assert_eq!(saved_bytes, expected.to_json_string().unwrap().as_bytes());
    assert_eq!(project.read_scene(), expected);
    let attempt = editor.current_frame().persistence().attempt().unwrap();
    editor::dispatch(
        &mut editor,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    let target = editor.current_frame().reopen_target();
    assert!(matches!(
        editor::dispatch(
            &mut editor,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
        ),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. }),
    ));
    editor::wait(&mut editor, |editor| {
        editor.current_frame().persistence().result().is_some()
    });
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. }),
    ));
    let reopened = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(editor::history(&editor, reopened), (0, 0));
    let basis = editor.document_query_basis(reopened).unwrap();
    let observed = editor.prepare_authoring_edit(&basis, &edit).unwrap();
    assert_eq!(observed.inverse(), &edit);
    drop(observed);
    assert_eq!(fs::read(project.root().join(STARTUP)).unwrap(), saved_bytes);
    assert_eq!(project.read_scene(), expected);
    editor::close(&mut editor);
    drop(editor);

    let delivery = check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap();
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["startup.scene.json"],
    );
    assert_eq!(
        delivery
            .members()
            .iter()
            .map(|member| member.path().as_str())
            .collect::<Vec<_>>(),
        [
            "assets/indices.png",
            "assets/indices.png.meta",
            "assets/palette.material2d.json",
            "assets/palette.material2d.json.meta",
            "assets/palette.png",
            "assets/palette.png.meta",
            "assets/palette.wgsl",
            "nara.toml",
            "scenes/startup.scene.json",
            "schema/authoring.bundle.json",
        ],
    );
    for member in delivery.members() {
        let bytes = fs::read(project.root().join(member.path().as_str())).unwrap();
        assert_eq!(member.digest(), ContentDigest::of_bytes(&bytes));
        if member.path().as_str() == "assets/palette.wgsl" {
            assert_eq!(member.kind(), ProjectContentMemberKind::AssetSource);
        }
    }
    let runtime = runtime::play(&project);
    assert_eq!(
        runtime.binding.generation().declaration(),
        &project.declaration,
    );
    assert_eq!(runtime.material.parameters(), &parameters(2.0));
    assert_eq!(runtime.binding.parameters(), &parameters(2.0));
    assert_eq!(runtime.defaults, parameters(0.0));
    assert_eq!(
        &runtime.binding.uniform_bytes()[..4],
        &2.0_f32.to_le_bytes()
    );
}

fn fields(value: &mut ComponentValue) -> &mut BTreeMap<String, ComponentValue> {
    let ComponentValue::Map(fields) = value else {
        panic!("the public durable material record must be a map");
    };
    fields
}

#[test]
fn source_collection_shader_and_input_snapshots_keep_original_content_budget_alive() {
    use nara::{
        fs::RelativePath,
        material::Material2dDomain,
        project_host::{
            ProjectContentBudgetKind, ProjectContentLoader, ingest_project_manifest,
            resolve_product_recipe,
        },
    };
    let project = Project::new();
    let capability = project.capability();
    let manifest = capability
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
    let loader = ProjectContentLoader::new(capability).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let collection = snapshot
        .domain_assets()
        .collection::<Material2dDomain>()
        .unwrap();
    let shader = collection
        .get(&AssetRef::Product(material_product()))
        .unwrap()
        .generation()
        .shader()
        .clone();
    let input = collection.iter().next().unwrap().generation().inputs()[0].clone();
    drop(snapshot);
    let held = loader.budget_snapshot();
    assert_eq!(held.active_generations(), 1);
    assert!(held.active(ProjectContentBudgetKind::RetainedBytes) > 0);
    assert!(shader.source().contains("material_fragment"));
    assert_eq!(input.source(), project::SHADER);
    drop(shader);
    assert_eq!(loader.budget_snapshot().active_generations(), 1);
    drop(input);
    let released = loader.budget_snapshot();
    assert_eq!(released.active_generations(), 0);
    assert_eq!(released.active_reservations(), 0);
    assert_eq!(released.active(ProjectContentBudgetKind::AggregateBytes), 0);
}

#[test]
fn product_rejects_wrong_declaration_parameter_and_product_kinds_without_touching_incumbent() {
    let project = Project::new();
    let original = project
        .record(0.0, BTreeMap::new())
        .to_component_value()
        .unwrap();
    let mut wrong_declaration = original.clone();
    fields(&mut wrong_declaration).insert(
        "declaration_id".to_owned(),
        ComponentValue::String("consumer.other-palette".to_owned()),
    );
    let mut wrong_parameter = original.clone();
    let parameters = fields(fields(&mut wrong_parameter).get_mut("parameters").unwrap());
    assert!(
        fields(parameters.get_mut("f32").unwrap())
            .remove("palette.offset")
            .is_some()
    );
    fields(parameters.get_mut("u32").unwrap())
        .insert("palette.offset".to_owned(), ComponentValue::U64(2));
    let mut wrong_material = original;
    fields(&mut wrong_material).insert(
        "material".to_owned(),
        nara::reflect::encode_asset_reference_value(&AssetRef::Product(indices_product())),
    );
    let wrong_image = project
        .record(
            0.0,
            BTreeMap::from([("palette".to_owned(), AssetRef::Product(material_product()))]),
        )
        .to_component_value()
        .unwrap();
    let invalid = [
        wrong_declaration,
        wrong_parameter,
        wrong_material,
        wrong_image,
    ];
    for (path, value) in REJECTED_SCENES.iter().zip(invalid) {
        Material2dRecord::from_component_value(&value).unwrap();
        let document = project.document(value);
        project.write_scene(STARTUP, &document);
        let error = check_project(ProjectCheckRequest::new(project.capability(), recipe()))
            .expect_err(path);
        assert!(
            error.content_error().unwrap().diagnostics().has_errors(),
            "{error:?}",
        );
        project.write_scene(&format!("scenes/{path}"), &document);
    }
    project.write_scene(STARTUP, &project.initial);
    project.declare_scenes(&REJECTED_SCENES);
    runtime::reject_replacements(&project);
}
