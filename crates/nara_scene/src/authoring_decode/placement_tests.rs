use super::*;
use crate::{PrefabEntityOrigin, PrefabInstanceHop, ScenePlacement, ScenePlacementParent};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn source(unknown: bool) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance::forest(
        AssetRef::path("unopened.prefab.json").unwrap(),
    ));
    let mut child = SceneEntityRecord::new(id("local"));
    if unknown {
        child.components.insert(
            ComponentTypeId::new("project.Unavailable"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("retain the complete source record".into()),
            ),
        );
    }
    let inherited = PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
            expected_base: crate::tests::named_prefab_source_key("unopened"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("inherited")),
    };
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("anchor")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("local")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("anchor"),
                ))),
            ),
            SceneEntityPlacement::new(
                inherited,
                ScenePlacement::before(
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(id("anchor"))),
                    PrefabEntityTarget::source(id("local")),
                ),
            ),
        ],
        [anchor, child],
    )
}

fn classify(document: &SceneDocument) -> AuthoringDocumentState {
    classify_scene_document(
        document,
        AuthoringDocumentSource::new(AssetRef::path("source.scene.json").unwrap()),
        AuthoringDocumentKind::Scene,
        Arc::clone(&basis().0),
        SceneFileLimits::default(),
    )
    .unwrap()
}

#[test]
fn unavailable_manifest_preserves_routed_pins_independently_of_the_local_location() {
    let document = source(true);
    let state = classify(&document);
    assert_eq!(state.readiness, AuthoringDocumentReadiness::Degraded);
    assert_eq!(
        state.unavailable_manifest.source_placements(),
        document.placements
    );
    let mut changed = document.clone();
    changed.placements[2].placement = ScenePlacement::after(
        ScenePlacementParent::Entity(PrefabEntityTarget::source(id("anchor"))),
        PrefabEntityTarget::source(id("local")),
    );
    let changed_state = classify(&changed);
    assert_eq!(
        state.unavailable_manifest.entries(),
        changed_state.unavailable_manifest.entries()
    );
    assert_ne!(
        state.unavailable_manifest,
        changed_state.unavailable_manifest
    );
    for candidate in [
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap(),
        SceneDocumentCandidate::decode_ron_str(&document.to_ron_string().unwrap()).unwrap(),
    ] {
        let reopened = candidate
            .admit_authoring_with_basis(
                state.source.clone(),
                &AuthoringSchemaBasis(Arc::clone(&state.basis)),
            )
            .unwrap();
        assert_eq!(
            reopened.state.unavailable_manifest,
            state.unavailable_manifest
        );
    }
}

#[test]
fn source_classification_and_copy_do_not_demand_projected_targets_or_instance_inputs() {
    let document = source(false);
    assert_eq!(
        classify(&document).readiness,
        AuthoringDocumentReadiness::Complete
    );
    let mut prefab = PrefabDocument::new(document.placements, document.entities);
    prefab
        .inputs
        .insert(nara_identity::ReferenceInputId::new("unbound").unwrap());
    let basis = basis();
    let source = AuthoringDocumentSource::new(AssetRef::path("source.prefab.json").unwrap());
    let admitted =
        PrefabDocumentCandidate::from_source_document(prefab.clone(), SceneFileLimits::default())
            .unwrap()
            .admit_authoring_with_basis(source.clone(), &basis)
            .unwrap();
    assert_eq!(admitted.document, prefab);
    assert_eq!(
        admitted.state.readiness,
        AuthoringDocumentReadiness::Complete
    );
    assert!(
        basis
            .admit_prefab_copy_source(
                &prefab,
                source.as_asset_ref(),
                SceneFileLimits::default(),
                |_, _| Ok::<_, std::convert::Infallible>(()),
            )
            .is_ok()
    );
}

#[test]
fn classification_admits_the_complete_source_layout_before_its_manifest_copy() {
    let document = source(true);
    let mut refused = false;
    let mut admission = |stage, bytes: Option<usize>| {
        if stage == "classification-source-placements" {
            assert_eq!(
                bytes,
                retention::source_placements_bytes(&document.placements)
            );
            refused = true;
            Err(AuthoringDocumentPublicationError::new(
                DiagnosticReport::default(),
            ))
        } else {
            Ok(())
        }
    };
    let result = classify_scene_document_impl(
        &document,
        AuthoringDocumentSource::new(AssetRef::path("source.scene.json").unwrap()),
        AuthoringDocumentKind::Scene,
        Arc::clone(&basis().0),
        SceneFileLimits::default(),
        Some(&mut admission),
    );
    assert!(result.is_err());
    assert!(refused);
}

#[test]
fn ordered_root_construction_preserves_declaration_order_when_records_are_canonicalized() {
    let mut document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("z")),
        SceneEntityRecord::new(id("a")),
    ]);
    assert_eq!(document.entities[0].id, id("a"));
    assert_eq!(
        document.placements,
        [
            SceneEntityPlacement::root(id("z")),
            SceneEntityPlacement::root(id("a")),
        ]
    );
    let placements = document.placements.clone();
    document.canonicalize();
    assert_eq!(document.placements, placements);
}
