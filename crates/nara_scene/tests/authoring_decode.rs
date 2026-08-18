use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_ecs::World;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, EntityReference, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentReadiness, AuthoringDocumentSource, AuthoringRecordReadiness,
    AuthoringScalarEdit, AuthoringSessionAuthorityError, AuthoringSessionReplacementError,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabInstance,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFormatError, ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    UnavailableRecordManifestEntry, UnavailableValuePathSegment, spawn_prefab, spawn_scene,
};

const OWNER: &str = "nara.scene.hierarchy.components";

#[test]
fn available_scalar_edit_preserves_known_unbound_manifest_across_reopen() {
    let (bundle, adapter) = scene_bundle_and_name_adapter();
    let owner = persistent_owner(OWNER);
    let entity = scene_id("player");
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
        .with_component(
            component_id("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Player".to_owned()),
            )
            .with_owner(owner.clone()),
        )
        .with_component(
            component_id("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("visible".to_owned()),
            )
            .with_owner(owner),
        )]);
    let source = source("scenes/main.scene.json");
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source.clone(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();

    assert_eq!(published.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(
        published
            .record_facts()
            .iter()
            .map(|fact| (fact.component().as_str(), fact.readiness()))
            .collect::<Vec<_>>(),
        vec![
            ("nara.scene.Name", AuthoringRecordReadiness::Available),
            (
                "nara.scene.Visibility",
                AuthoringRecordReadiness::KnownUnbound,
            ),
        ]
    );
    assert_eq!(
        published.record_facts()[0]
            .adapter()
            .expect("Name is covered by the trusted Adapter")
            .identity()
            .as_str(),
        "nara.scene.first-party-authoring-adapter"
    );
    let manifest = published.unavailable_manifest().clone();
    assert_eq!(manifest.record_count(), 1);
    assert_eq!(manifest.patch_target_count(), 0);
    let diagnostics = published.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), "scene.authoring-known-unbound");
    assert_eq!(
        diagnostic.severity(),
        nara_diagnostic::DiagnosticSeverity::Warning
    );

    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let before_revision = session.revision();
    let edit = AuthoringScalarEdit::new(
        entity,
        component_id("nara.scene.Name"),
        ComponentFieldId::new("value"),
        ComponentValue::String("Renamed".to_owned()),
    );
    let report = session.apply_authoring_scalar_edit(edit);

    assert!(report.applied, "{:#?}", report.diagnostics);
    assert_ne!(session.revision(), before_revision);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Renamed".to_owned())
    );

    let registry = ComponentRegistry::new();
    let undo = session.undo(&registry);
    assert!(undo.applied, "{:#?}", undo.diagnostics);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Player".to_owned())
    );

    let redo = session.redo(&registry);
    assert!(redo.applied, "{:#?}", redo.diagnostics);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Renamed".to_owned())
    );

    let before_document = session.document().clone();
    let before_revision = session.revision();
    let before_history = session.history_status();
    let replacement = SceneDocument::default();
    assert_eq!(
        session.replace_document(replacement),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    let native_candidate = SceneDocumentCandidate::decode_json_bytes(
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    assert!(matches!(
        session.try_replace_file_candidate(native_candidate, &registry),
        Err(AuthoringSessionReplacementError::Authority(
            AuthoringSessionAuthorityError::PublishedAuthoringDocument
        ))
    ));
    let mut world = World::new();
    let world_entities_before_sync = world.iter_entities().count();
    assert_eq!(
        session.sync_world(&mut world, &registry),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    assert_eq!(world.iter_entities().count(), world_entities_before_sync);
    let raw_patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("player"),
        component: component_id("nara.scene.Name"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("Bypass".to_owned()),
    }]);
    let raw_report = session.apply_patch(&raw_patch, &registry);
    assert!(!raw_report.applied);
    assert!(raw_report.diagnostics.has_errors());
    assert_eq!(session.document(), &before_document);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.history_status(), before_history);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    let reopened_candidate = SceneDocumentCandidate::decode_json_bytes(
        session.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let reopened = session
        .try_reopen_authoring_candidate(reopened_candidate)
        .unwrap();
    assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(reopened.document(), session.document());
}

#[test]
fn prefab_unknown_records_and_target_barriers_round_trip_semantically() {
    let unknown = component_id("nara.project.MissingBehavior");
    let anchor = scene_id("anchor");
    let added = scene_id("added");
    let overrides = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(added.clone()).with_component(
                unknown.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
                )
                .with_owner(persistent_owner("nara.project.behavior")),
            ),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        },
        ScenePatchOperation::SetField {
            entity: added,
            component: unknown.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("enabled"),
            value: ComponentValue::Bool(false),
        },
    ]);
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord {
        id: anchor,
        children: Vec::new(),
        components: BTreeMap::from([(
            unknown,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("speed", ComponentValue::I64(3))]),
            )
            .with_owner(persistent_owner("nara.project.behavior")),
        )]),
        prefab: Some(PrefabInstance {
            source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
            overrides,
        }),
    }]);
    let source = source("prefabs/missing.prefab.json");
    let bundle =
        AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
    let encoded = prefab.to_json_string().unwrap();
    let published = PrefabDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .admit_authoring(source, &bundle, &[])
        .unwrap()
        .publish();

    assert_eq!(published.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(published.unavailable_manifest().record_count(), 2);
    assert_eq!(published.unavailable_manifest().patch_target_count(), 1);
    assert!(
        published
            .record_facts()
            .iter()
            .all(|fact| { fact.readiness() == AuthoringRecordReadiness::UnknownSchema })
    );

    let manifest = published.unavailable_manifest().clone();
    let document = published.document().clone();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let session_revision = session.revision();
    assert_eq!(
        session.replace_document(PrefabDocument::default()),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), session_revision);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    let reopened = PrefabDocumentCandidate::decode_json_bytes(
        session.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let reopened = session.try_reopen_authoring_candidate(reopened).unwrap();
    assert_eq!(
        reopened.unavailable_record_manifest(),
        session.unavailable_record_manifest()
    );
}

#[test]
fn strict_runtime_rejects_degraded_scene_and_prefab_before_world_mutation() {
    let unknown = component_id("nara.project.MissingBehavior");
    let owner = persistent_owner("nara.project.behavior");
    let entity = SceneEntityRecord::new(scene_id("root")).with_component(
        unknown,
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
        )
        .with_owner(owner),
    );
    let scene = SceneDocument::from_ordered_roots([entity.clone()]);
    let prefab = PrefabDocument::from_ordered_roots([entity]);
    let bundle =
        AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
    let scene =
        SceneDocumentCandidate::decode_json_bytes(scene.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/degraded.scene.json"), &bundle, &[])
            .unwrap()
            .publish();
    let prefab =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("prefabs/degraded.prefab.json"), &bundle, &[])
            .unwrap()
            .publish();
    assert_eq!(scene.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(prefab.readiness(), AuthoringDocumentReadiness::Degraded);

    let mut registry = ComponentRegistry::new();
    let provider =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    nara_reflect::__private::register_or_validate_resolved_schema_provider(&mut registry, provider)
        .unwrap();
    registry.freeze().unwrap();
    let mut world = World::new();
    let before = world.iter_entities().count();

    let scene_report = spawn_scene(&mut world, &registry, scene.document());
    assert!(scene_report.diagnostics.has_errors());
    assert!(scene_report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before);

    let prefab_report = spawn_prefab(&mut world, &registry, prefab.document());
    assert!(prefab_report.diagnostics.has_errors());
    assert!(prefab_report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before);

    let mismatched_owner =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("wrong-owner"))
            .with_component(
                component_id("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("Wrong owner".to_owned()),
                )
                .with_owner(persistent_owner("nara.project.spoofed-scene-owner")),
            )]);
    let owner_report = spawn_scene(&mut world, &registry, &mismatched_owner);
    assert!(owner_report.instance.is_none());
    assert!(
        owner_report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.component-owner-mismatch" })
    );
    assert_eq!(world.iter_entities().count(), before);
}

#[test]
fn older_and_future_records_classify_without_native_migration() {
    let component = component_id("nara.project.Versioned");
    let owner = persistent_owner("nara.project.versioned");
    let schema = ComponentSchema::new(component.clone(), "Versioned", ComponentSchemaVersion(2))
        .with_capabilities([ComponentCapability::Scene]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("old")).with_component(
            component.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null)
                .with_owner(owner.clone()),
        ),
        SceneEntityRecord::new(scene_id("future")).with_component(
            component,
            SceneComponentRecord::new(ComponentSchemaVersion(3), ComponentValue::Null)
                .with_owner(owner),
        ),
    ]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/versioned.scene.json"), &bundle, &[])
            .unwrap()
            .publish();

    assert_eq!(
        published
            .record_facts()
            .iter()
            .map(|fact| fact.readiness())
            .collect::<Vec<_>>(),
        vec![
            AuthoringRecordReadiness::FutureSchema,
            AuthoringRecordReadiness::Unmigratable,
        ]
    );
}

#[test]
fn unavailable_manifest_retains_entity_and_schema_declared_asset_references() {
    let component = component_id("nara.project.Link");
    let owner = persistent_owner("nara.project.link");
    let target = scene_id("target");
    let asset = AssetRef::path("textures/icon.png").unwrap();
    let schema = ComponentSchema::new(component.clone(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
            )
            .with_capability(ComponentCapability::AssetRef),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("source")).with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone(),
                        }),
                    ),
                    (
                        "image",
                        ComponentValue::map([
                            ("kind", ComponentValue::String("path".to_owned())),
                            (
                                "value",
                                ComponentValue::String("textures/icon.png".to_owned()),
                            ),
                        ]),
                    ),
                ]),
            )
            .with_owner(owner),
        ),
        SceneEntityRecord::new(target),
    ]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/links.scene.json"), &bundle, &[])
            .unwrap()
            .publish();

    let UnavailableRecordManifestEntry::Record(record) =
        &published.unavailable_manifest().entries()[0]
    else {
        panic!("the link record must be retained as an unavailable record")
    };
    assert_eq!(record.entity_references().len(), 1);
    assert_eq!(
        record.entity_references()[0].path(),
        &[UnavailableValuePathSegment::Field("target".to_owned())]
    );
    assert_eq!(
        record.entity_references()[0].reference(),
        &EntityReference::SceneLocal {
            entity: scene_id("target")
        }
    );
    assert_eq!(record.schema_declared_asset_references().len(), 1);
    assert_eq!(
        record.schema_declared_asset_references()[0].asset_ref(),
        &asset
    );
}

#[test]
fn unknown_component_value_tag_rejects_before_authoring_candidate() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("entity"))
        .with_component(
            component_id("nara.project.UnknownValue"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
    let mut encoded =
        serde_json::from_str::<serde_json::Value>(&document.to_json_string().unwrap()).unwrap();
    encoded["payload"]["entities"][0]["components"]["nara.project.UnknownValue"]["value"] =
        serde_json::json!({ "type": "future_value", "value": 7 });

    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes(&serde_json::to_vec(&encoded).unwrap()),
        Err(SceneFormatError::Payload { .. })
    ));
}

fn scene_bundle_and_name_adapter() -> (
    AuthoringSchemaBundle,
    nara_scene::__private::AuthoringAdapterReceipt,
) {
    let resolved =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        persistent_owner(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str()),
        resolved.current().catalog().clone(),
        resolved
            .predecessor()
            .map(|record| record.catalog().clone()),
    )])
    .unwrap();
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
        resolved.contribution_receipt(),
        "nara.scene.first-party-authoring-adapter",
        1,
        1,
        [component_id("nara.scene.Name")],
    )
    .unwrap();
    (bundle, adapter)
}

fn source(path: &str) -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path(path).unwrap())
}

fn persistent_owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn component_id(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
