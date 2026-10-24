#[cfg(feature = "serde")]
#[path = "support/prefab_source.rs"]
mod prefab_source;

#[cfg(feature = "serde")]
use nara_asset::AssetRef;
use nara_asset::{AssetPath, AssetRecord, AssetSourceKind, ProjectAssetDatabase, StableAssetId};
use nara_core::ItemLimit;
use nara_ecs::Component;
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};
use nara_scene::{
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchApplyLimits, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

#[derive(Component)]
struct ReferenceHolder;

#[derive(Component)]
struct BudgetHolder;

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn reference_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.test.FinalReferences")
}

fn links(target: &str) -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::EntityReference(
        EntityReference::SceneLocal {
            entity: scene_id(target),
        },
    )])
}

fn images(path: &str) -> ComponentValue {
    ComponentValue::map([(
        "icons",
        ComponentValue::List(vec![ComponentValue::map([
            ("kind", ComponentValue::String("path".to_owned())),
            ("value", ComponentValue::String(path.to_owned())),
        ])]),
    )])
}

fn reference_schema() -> ComponentSchema {
    let mut link_field = ComponentFieldSchema::required(
        ComponentFieldId::new("links"),
        "Links",
        ComponentFieldPath::from_fields(["links"]),
        ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::EntityRef,
        )),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    link_field.default_value = Some(links("old"));
    let mut image_field = ComponentFieldSchema::required(
        ComponentFieldId::new("images"),
        "Images",
        ComponentFieldPath::from_fields(["images"]),
        ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::AssetRef,
        ))),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef);
    image_field.default_value = Some(images("textures/default.png"));
    ComponentSchema::new(
        reference_id(),
        "Final references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([link_field, image_field])
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<ReferenceHolder, _, _>(
            reference_schema(),
            |value| {
                value.field("links")?;
                value.field("images")?;
                Ok(ReferenceHolder)
            },
            |_| Ok(reference_value("old", "textures/present.png")),
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn reference_value(target: &str, image: &str) -> ComponentValue {
    ComponentValue::map([("links", links(target)), ("images", images(image))])
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("source")).with_component(
            reference_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                reference_value("old", "textures/present.png"),
            ),
        ),
        SceneEntityRecord::new(scene_id("old")),
    ])
}

fn database(include_default: bool) -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            StableAssetId::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            AssetPath::new("textures/present.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    if include_default {
        database
            .insert(AssetRecord::new(
                StableAssetId::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
                AssetPath::new("textures/default.png").unwrap(),
                AssetSourceKind::Image,
            ))
            .unwrap();
    }
    database
}

fn set_field(field: &str, value: ComponentValue) -> ScenePatchOperation {
    ScenePatchOperation::SetField {
        entity: scene_id("source"),
        component: reference_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value,
    }
}

fn final_closure_patch() -> ScenePatchDocument {
    ScenePatchDocument::new([
        set_field("links", links("later")),
        set_field("images", images("textures/temporarily-missing.png")),
        set_field("images", images("textures/present.png")),
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(scene_id("later")),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        },
    ])
}

#[test]
fn nested_multi_operation_closure_commits_one_inverse_and_revision_across_native_entries() {
    let registry = registry();
    let database = database(false);
    let original = document();
    let patch = final_closure_patch();
    let mut direct = original.clone();
    let report = patch.apply_to_scene_with_asset_database(&mut direct, &registry, &database);
    assert!(report.applied, "{:?}", report.diagnostics);
    let inverse = report.inverse.unwrap();
    assert!(
        inverse
            .apply_to_scene_with_asset_database(&mut direct, &registry, &database)
            .applied
    );
    assert_eq!(direct, original);

    let mut scene = SceneAuthoringSession::new(original.clone());
    let before = scene.revision();
    let report = scene.apply_patch_with_asset_database(&patch, &registry, &database);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert!(report.inverse.is_some());
    assert_eq!(scene.revision().generation(), before.generation() + 1);
    assert_eq!(scene.history_status().undo_depth, 1);
    assert!(scene.undo_with_asset_database(&registry, &database).applied);
    assert_eq!(scene.document(), &original);
    assert!(scene.redo_with_asset_database(&registry, &database).applied);

    let original_prefab = PrefabDocument::new(original.roots.clone(), original.entities.clone());
    let mut prefab = PrefabAuthoringSession::new(original_prefab.clone());
    let before = prefab.revision();
    let report = prefab.apply_patch_with_asset_database(&patch, &registry, &database);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert!(report.inverse.is_some());
    assert_eq!(prefab.revision().generation(), before.generation() + 1);
    assert_eq!(prefab.history_status().undo_depth, 1);
    assert!(
        prefab
            .undo_with_asset_database(&registry, &database)
            .applied
    );
    assert_eq!(prefab.document(), &original_prefab);
    assert!(
        prefab
            .redo_with_asset_database(&registry, &database)
            .applied
    );
}

#[test]
fn final_nested_asset_or_local_dangling_rejects_without_history_publication() {
    let registry = registry();
    let database = database(false);
    for operation in [
        set_field("links", links("absent")),
        set_field("images", images("textures/absent.png")),
        ScenePatchOperation::RemoveField {
            entity: scene_id("source"),
            component: reference_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("images"),
        },
    ] {
        let patch = ScenePatchDocument::new([operation]);
        let mut scene = SceneAuthoringSession::new(document());
        let original = scene.document().clone();
        let revision = scene.revision();
        let history = scene.history_status();
        let report = scene.apply_patch_with_asset_database(&patch, &registry, &database);
        assert!(!report.applied, "final dangling closure was published");
        assert!(report.inverse.is_none());
        assert_eq!(scene.document(), &original);
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.history_status(), history);

        let original = PrefabDocument::new(original.roots, original.entities);
        let mut prefab = PrefabAuthoringSession::new(original.clone());
        let revision = prefab.revision();
        let history = prefab.history_status();
        let report = prefab.apply_patch_with_asset_database(&patch, &registry, &database);
        assert!(
            !report.applied,
            "final dangling Prefab closure was published"
        );
        assert!(report.inverse.is_none());
        assert_eq!(prefab.document(), &original);
        assert_eq!(prefab.revision(), revision);
        assert_eq!(prefab.history_status(), history);
    }
}

#[test]
fn effective_asset_defaults_require_catalogue_membership_only_when_database_is_supplied() {
    let registry = registry();
    let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: scene_id("source"),
        component: reference_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("images"),
    }]);
    let mut logical = document();
    assert!(patch.apply_to_scene(&mut logical, &registry).applied);
    assert!(
        logical
            .entities
            .iter()
            .find(|entity| entity.id == scene_id("source"))
            .unwrap()
            .components[&reference_id()]
            .value
            .get("images")
            .is_none()
    );
    let mut unresolved = document();
    let original = unresolved.clone();
    let rejected =
        patch.apply_to_scene_with_asset_database(&mut unresolved, &registry, &database(false));
    assert!(
        !rejected.applied,
        "an omitted default asset bypassed the selected catalogue"
    );
    assert!(rejected.inverse.is_none());
    assert_eq!(unresolved, original);
    let mut catalogued = document();
    let report =
        patch.apply_to_scene_with_asset_database(&mut catalogued, &registry, &database(true));
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(logical, catalogued);
}

fn budget_registry() -> ComponentRegistry {
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("values"),
        "Values",
        ComponentFieldPath::from_fields(["values"]),
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    field.default_value = Some(ComponentValue::List(vec![ComponentValue::U64(7); 32]));
    let schema = ComponentSchema::new(
        ComponentTypeId::new("nara.test.BudgetDefault"),
        "Budget default",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<BudgetHolder, _, _>(
            schema,
            |value| {
                value.field("values")?;
                Ok(BudgetHolder)
            },
            |_| {
                Ok(ComponentValue::map([(
                    "values",
                    ComponentValue::List(vec![ComponentValue::U64(7); 32]),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn budget_document() -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("source")).with_component(
        ComponentTypeId::new("nara.test.BudgetDefault"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::Map(Default::default()),
        ),
    )])
}

#[test]
fn public_patch_selected_value_limits_bound_default_interpretation() {
    let registry = budget_registry();
    let mut document = budget_document();
    let original = document.clone();
    let limits =
        ScenePatchApplyLimits::default().with_validation_value_nodes(ItemLimit::new(2).unwrap());
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(scene_id("later")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let report = patch.apply_to_scene_with_limits(&mut document, &registry, limits);
    assert!(
        !report.applied,
        "selected patch limits were replaced by a default interpretation policy"
    );
    assert!(report.inverse.is_none());
    assert_eq!(document, original);
    let exact =
        ScenePatchApplyLimits::default().with_validation_value_nodes(ItemLimit::new(34).unwrap());
    let report = patch.apply_to_scene_with_limits(&mut document, &registry, exact);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert!(report.inverse.is_some());
}

#[cfg(feature = "serde")]
#[test]
fn authoring_asset_database_file_entries_preserve_selected_canonical_value_limits() {
    use nara_scene::{PrefabDocumentCandidate, SceneDocumentCandidate, SceneFileLimits};
    let registry = budget_registry();
    let database = ProjectAssetDatabase::default();
    let limits = SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(1).unwrap());
    let scene = budget_document();
    let scene_json = scene.to_json_string().unwrap();
    let scene_candidate = || {
        SceneDocumentCandidate::decode_json_bytes_with_limits(scene_json.as_bytes(), limits)
            .unwrap()
    };
    assert!(
        SceneAuthoringSession::try_from_file_candidate_with_asset_database(
            scene_candidate(),
            &registry,
            &database,
        )
        .is_err()
    );
    let mut session = SceneAuthoringSession::new(SceneDocument::default());
    let revision = session.revision();
    assert!(
        session
            .try_replace_file_candidate_with_asset_database(scene_candidate(), &registry, &database)
            .is_err()
    );
    assert_eq!(session.document(), &SceneDocument::default());
    assert_eq!(session.revision(), revision);

    let prefab = PrefabDocument::new(scene.roots, scene.entities);
    let prefab_json = prefab.to_json_string().unwrap();
    let prefab_candidate = || {
        PrefabDocumentCandidate::decode_json_bytes_with_limits(prefab_json.as_bytes(), limits)
            .unwrap()
    };
    assert!(
        PrefabAuthoringSession::try_from_file_candidate_with_asset_database(
            prefab_candidate(),
            &registry,
            &database,
        )
        .is_err()
    );
    let mut session = PrefabAuthoringSession::new(PrefabDocument::default());
    let revision = session.revision();
    assert!(
        session
            .try_replace_file_candidate_with_asset_database(
                prefab_candidate(),
                &registry,
                &database
            )
            .is_err()
    );
    assert_eq!(session.document(), &PrefabDocument::default());
    assert_eq!(session.revision(), revision);
}

#[cfg(feature = "serde")]
#[test]
fn published_data_authoring_closes_known_local_edges_and_preserves_unknown_records() {
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaCatalog,
        PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        __private::AuthoringAdapterReceipt, AuthoringDocumentSource, PrefabDocumentCandidate,
        SceneDocumentCandidate,
    };
    use std::collections::BTreeSet;
    let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.final-references").unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![reference_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let unknown = ComponentTypeId::new("nara.future.Unknown");
    let mut document = document();
    document
        .entities
        .iter_mut()
        .find(|entity| entity.id == scene_id("source"))
        .unwrap()
        .components
        .get_mut(&reference_id())
        .unwrap()
        .owner = Some(owner);
    let preserved = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::String("retain me".to_owned()),
    );
    document
        .entities
        .iter_mut()
        .find(|entity| entity.id == scene_id("source"))
        .unwrap()
        .components
        .insert(unknown.clone(), preserved.clone());
    let publication =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/links.scene.json").unwrap()),
                &bundle,
                &[adapter.clone()],
            )
            .unwrap()
            .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(publication);
    let before = session.document().clone();
    let revision = session.revision();
    let report = session.apply_authoring_patch(&ScenePatchDocument::new([set_field(
        "images",
        images("textures/another-logical-image.png"),
    )]));
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(session.revision().generation(), revision.generation() + 1);
    assert_eq!(
        session
            .document()
            .entities
            .iter()
            .find(|entity| entity.id == scene_id("source"))
            .unwrap()
            .components[&unknown],
        preserved
    );
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &before);

    let mut dangling = before;
    dangling
        .entities
        .retain(|entity| entity.id != scene_id("old"));
    dangling.roots.retain(|entity| entity != &scene_id("old"));
    for omit_link in [false, true] {
        let mut candidate_document = dangling.clone();
        if omit_link {
            candidate_document
                .entities
                .iter_mut()
                .find(|entity| entity.id == scene_id("source"))
                .unwrap()
                .components
                .get_mut(&reference_id())
                .unwrap()
                .value
                .remove_path(&ComponentFieldPath::from_fields(["links"]))
                .unwrap();
        }
        let candidate = SceneDocumentCandidate::decode_json_bytes(
            candidate_document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap();
        assert!(
            candidate
                .admit_authoring(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/links.scene.json").unwrap()
                    ),
                    &bundle,
                    &[adapter.clone()],
                )
                .is_err(),
            "known nested local reference lost its target during Scene authoring publication, omitted={omit_link}"
        );
        let prefab = PrefabDocument::new(candidate_document.roots, candidate_document.entities);
        let candidate =
            PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
                .unwrap();
        assert!(
            candidate
                .admit_authoring(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/links.prefab.json").unwrap()
                    ),
                    &bundle,
                    &[adapter.clone()],
                )
                .is_err(),
            "known nested local reference lost its target during Prefab authoring publication, omitted={omit_link}"
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn prefab_anchor_does_not_bypass_direct_source_reference_closure() {
    use nara_ecs::World;
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaCatalog,
        PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        __private::AuthoringAdapterReceipt, AuthoringDocumentSource, InMemoryPrefabSourceResolver,
        PrefabInstance, SceneDocumentCandidate,
    };
    use std::collections::BTreeSet;

    let registry = registry();
    let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.final-references").unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner,
        ComponentSchemaCatalog {
            components: vec![reference_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let source = AssetRef::path("prefabs/independent.prefab.json").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        source.clone(),
        prefab_source::key("final-closure.independent"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("child"))]),
    );
    let mut anchor = SceneEntityRecord::new(scene_id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source,
        overrides: ScenePatchDocument::default(),
    });
    let mut source_document = document();
    source_document.roots.push(anchor.id.clone());
    source_document.entities.push(anchor);
    for dangling in [false, true] {
        let mut candidate_document = source_document.clone();
        if dangling {
            candidate_document
                .roots
                .retain(|entity| entity != &scene_id("old"));
            candidate_document
                .entities
                .retain(|entity| entity.id != scene_id("old"));
        }
        let encoded = candidate_document.to_json_string().unwrap();
        let generic = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/anchored.scene.json").unwrap()),
                &bundle,
                &[adapter.clone()],
            );
        assert_eq!(generic.is_err(), dangling);
        let native = SceneAuthoringSession::try_from_file_candidate(
            SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
            &registry,
        );
        assert_eq!(native.is_err(), dangling);
        let mut world = World::new();
        let before_entities = world.iter_entities().count();
        let expansion = (candidate_document).expand_prefabs(&registry, &resolver);
        assert_eq!(
            expansion.diagnostics.has_errors(),
            dangling,
            "{:?}",
            expansion.diagnostics
        );
        assert_eq!(expansion.candidate.is_none(), dangling);
        if dangling {
            assert_eq!(world.iter_entities().count(), before_entities);
        } else {
            let candidate = expansion
                .candidate
                .unwrap()
                .bind_source(scene_source_key("prefab-anchor-reference-closure/source"));
            let spawned = nara_scene::spawn_scene(
                &mut world,
                &registry,
                &candidate,
                &scene_occurrence("prefab-anchor-reference-closure/instance"),
                None,
            );
            assert!(
                !spawned.diagnostics.has_errors(),
                "{:?}",
                spawned.diagnostics
            );
            assert!(spawned.instance.is_some());
        }
    }
}

#[cfg(feature = "serde")]
fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}
#[cfg(feature = "serde")]
fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
