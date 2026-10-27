use std::convert::Infallible;

use nara_reflect::{ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog};

use super::*;
use crate::{
    PrefabAuthoringSession, ResolvedPrefabSource, SceneAuthoringSession, SourceDocumentKey,
    SourceDocumentKind,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.AssetBudget")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.asset-budget").unwrap()
}

fn asset_basis(field_count: usize, defaults: bool) -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Assets", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields((0..field_count).map(|index| {
            let name = format!("image{index:02}");
            let field = if defaults {
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new(&name),
                    &name,
                    ComponentFieldPath::from_fields([name.as_str()]),
                    nara_reflect::ComponentValueKind::AssetRef,
                    crate::patch::asset_ref_value(&AssetRef::path("textures/a.png").unwrap()),
                )
            } else {
                ComponentFieldSchema::optional(
                    ComponentFieldId::new(&name),
                    &name,
                    ComponentFieldPath::from_fields([name.as_str()]),
                    nara_reflect::ComponentValueKind::AssetRef,
                )
            };
            field.with_capabilities([
                ComponentCapability::Scene,
                ComponentCapability::Inspect,
                ComponentCapability::AssetRef,
            ])
        }));
    let schema_owner = AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    );
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&schema_owner, &Default::default()).unwrap();
    let bundle = AuthoringSchemaBundle::new([schema_owner]).unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

fn conversion_fixture() -> (SceneDocument, AuthoringSchemaBasis, ResolvedPrefabSource) {
    let basis = asset_basis(60, false);
    let empty = ComponentValue::Map(BTreeMap::new());
    assert_eq!(empty.cost().nodes(), 1);
    let value = ComponentValue::map((0..6).map(|index| {
        (
            format!("image{index:02}"),
            crate::patch::asset_ref_value(&AssetRef::path("textures/a.png").unwrap()),
        )
    }));
    assert_eq!(value.cost().nodes(), 19);
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/asset-budget.prefab.json").unwrap(),
        overrides: None,
    });
    // Fresh IDs have 22 Base64 characters. This existing ID sorts after every possible one.
    let existing =
        SceneEntityRecord::new(id(&"z".repeat(32))).with_component(component(), record(empty));
    let document = SceneDocument::from_ordered_roots([anchor, existing]);
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf")).with_component(component(), record(value)),
        ])),
        basis.clone(),
        SourceDocumentKey::named(
            "e758bd18-2f06-4736-9752-dcfe038c3289".parse().unwrap(),
            "asset-budget",
            SourceDocumentKind::Prefab,
        )
        .unwrap(),
    );
    (document, basis, resolved)
}

fn assert_scene_classification(actual: &SceneAuthoringSession, expected: &AuthoringDocumentState) {
    assert_eq!(
        actual.authoring_record_facts(),
        Some(expected.record_facts.as_slice())
    );
    assert_eq!(
        actual.unavailable_record_manifest(),
        Some(&expected.unavailable_manifest)
    );
    assert_eq!(actual.authoring_readiness(), Some(expected.readiness));
    assert_eq!(actual.authoring_diagnostics(), Some(&expected.diagnostics));
    assert_eq!(
        actual.authoring_schema_basis(),
        Some(expected.schema_basis())
    );
}

fn assert_prefab_classification(
    actual: &PrefabAuthoringSession,
    expected: &AuthoringDocumentState,
) {
    assert_eq!(
        actual.authoring_record_facts(),
        Some(expected.record_facts.as_slice())
    );
    assert_eq!(
        actual.unavailable_record_manifest(),
        Some(&expected.unavailable_manifest)
    );
    assert_eq!(actual.authoring_readiness(), Some(expected.readiness));
    assert_eq!(actual.authoring_diagnostics(), Some(&expected.diagnostics));
    assert_eq!(
        actual.authoring_schema_basis(),
        Some(expected.schema_basis())
    );
}

#[test]
fn scene_conversion_at_64_value_nodes_matches_full_classification_save_and_reopen() {
    let (document, basis, resolved) = conversion_fixture();
    let limits = SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(64).unwrap());
    let source =
        AuthoringDocumentSource::new(AssetRef::path("scenes/asset-budget.scene.json").unwrap());
    let published = SceneDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
    .admit_authoring_with_basis(source.clone(), &basis)
    .unwrap()
    .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap();
    assert_eq!(prepared.entities().len(), 1);
    assert!(prepared.entities()[0].entity() < &id(&"z".repeat(32)));
    let reviewed = prepared.document().clone();
    assert_eq!(
        reviewed.entities.last().unwrap(),
        document.entities.last().unwrap()
    );
    let full = classify_scene_document(
        &reviewed,
        source,
        AuthoringDocumentKind::Scene,
        Arc::clone(&basis.0),
        limits,
    )
    .unwrap();
    assert_eq!(full.schema_declared_asset_references().count(), 6);
    session
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert_eq!(session.document(), &reviewed);
    assert_scene_classification(&session, &full);

    crate::format::validate_scene_publication_shape(session.document(), limits).unwrap();
    for json in [true, false] {
        let candidate = if json {
            SceneDocumentCandidate::decode_json_bytes_with_limits(
                session.document().to_json_string().unwrap().as_bytes(),
                limits,
            )
        } else {
            SceneDocumentCandidate::decode_ron_bytes_with_limits(
                session.document().to_ron_string().unwrap().as_bytes(),
                limits,
            )
        }
        .unwrap();
        let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
        assert_eq!(reopened.document(), &reviewed);
        assert_scene_classification(&reopened, &full);
        assert_ne!(
            reopened.revision().source_id(),
            session.revision().source_id()
        );
    }
}

#[test]
fn prefab_conversion_at_64_value_nodes_matches_full_classification_save_and_reopen() {
    let (scene, basis, resolved) = conversion_fixture();
    let document = PrefabDocument::new(scene.roots, scene.entities);
    let limits = SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(64).unwrap());
    let source = AuthoringDocumentSource::new(
        AssetRef::path("prefabs/asset-budget-source.prefab.json").unwrap(),
    );
    let published = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
    .admit_authoring_with_basis(source.clone(), &basis)
    .unwrap()
    .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let prepared = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |_, _| Ok::<_, Infallible>(Some(resolved.clone())))
        .unwrap();
    assert_eq!(prepared.entities().len(), 1);
    assert!(prepared.entities()[0].entity() < &id(&"z".repeat(32)));
    let reviewed = prepared.document().clone();
    assert_eq!(
        reviewed.entities.last().unwrap(),
        document.entities.last().unwrap()
    );
    let full = classify_scene_document(
        &reviewed.instantiate(),
        source,
        AuthoringDocumentKind::Prefab,
        Arc::clone(&basis.0),
        limits,
    )
    .unwrap();
    assert_eq!(full.schema_declared_asset_references().count(), 6);
    session
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert_eq!(session.document(), &reviewed);
    assert_prefab_classification(&session, &full);

    crate::format::validate_prefab_publication_shape(session.document(), limits).unwrap();
    for json in [true, false] {
        let candidate = if json {
            PrefabDocumentCandidate::decode_json_bytes_with_limits(
                session.document().to_json_string().unwrap().as_bytes(),
                limits,
            )
        } else {
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                session.document().to_ron_string().unwrap().as_bytes(),
                limits,
            )
        }
        .unwrap();
        let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
        assert_eq!(reopened.document(), &reviewed);
        assert_prefab_classification(&reopened, &full);
        assert_ne!(
            reopened.revision().source_id(),
            session.revision().source_id()
        );
    }
}

#[test]
fn default_asset_references_still_enforce_aggregate_node_and_byte_quotas() {
    let basis = asset_basis(3, true);
    for (limits, admitted) in [
        (
            SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(16).unwrap()),
            5,
        ),
        (
            SceneFileLimits::default()
                .with_component_value_nodes(ItemLimit::new(64).unwrap())
                .with_component_value_bytes(ByteLimit::new(256).unwrap()),
            3,
        ),
    ] {
        for count in [admitted, admitted + 1] {
            let document = SceneDocument::from_ordered_roots((0..count).map(|index| {
                SceneEntityRecord::new(id(&format!("entity{index}")))
                    .with_component(component(), record(ComponentValue::Map(BTreeMap::new())))
            }));
            // Authored values fit; only their omitted default asset inventory crosses the quota.
            crate::format::validate_scene_value_budget(&document, limits).unwrap();
            let result = classify_scene_document(
                &document,
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/default-assets.scene.json").unwrap(),
                ),
                AuthoringDocumentKind::Scene,
                Arc::clone(&basis.0),
                limits,
            );
            if count == admitted {
                assert_eq!(
                    result.unwrap().schema_declared_asset_references().count(),
                    count * 3,
                );
            } else {
                assert!(result.unwrap_err().diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.authoring-asset-reference-budget-exceeded"
                }));
            }
        }
    }
}
