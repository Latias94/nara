use super::*;
use crate::{ScenePlacement, ScenePlacementOwner};
use nara_asset::AssetRef;
use nara_core::SerdeShapeError;
use nara_identity::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
};

// Preserve the complete pre-optimization validator as an independent acceptance oracle.
fn reference<T: SceneFilePayload>(
    payload: &T,
    limits: SceneFileLimits,
) -> Result<(), SceneFormatError> {
    let envelope = canonical_envelope(payload, limits)?;
    validate_json_publication_shape(&envelope, limits, false)?;
    validate_ron_publication_shape(&envelope, limits, false)
}

fn equivalent<T: SceneFilePayload>(payload: &T, limits: SceneFileLimits) {
    let expected = reference(payload, limits).map_err(|error| error.to_string());
    let actual = validate_publication_shape(payload, limits).map_err(|error| error.to_string());
    assert_eq!(actual, expected, "limits: {limits:?}");
}

fn certified<T: SceneFilePayload>(payload: &T, limits: SceneFileLimits) -> bool {
    let envelope = canonical_envelope(payload, limits).unwrap();
    envelope
        .payload()
        .entity_document()
        .is_some_and(|(roots, entities)| {
            publication_shape::certifies_entity_document(&envelope, roots, entities, limits.shape())
        })
}

#[test]
fn json_format_error_accessors_expose_only_safe_parser_facts() {
    let syntax =
        SceneDocumentCandidate::decode_json_bytes(b"{\"private-document-sentinel\":").unwrap_err();
    assert!(syntax.is_syntax_error());
    assert!(matches!(syntax.parser_location(), Some((1, column)) if column > 1));
    assert!(syntax.shape_error().is_none());

    let limits = SceneFileLimits::default().with_shape(SerdeShapeLimits::new(
        nara_core::DepthLimit::new(64).unwrap(),
        nara_core::ItemLimit::new(100).unwrap(),
        nara_core::ItemLimit::new(100).unwrap(),
        nara_core::ByteLimit::new(8).unwrap(),
        nara_core::ByteLimit::new(64).unwrap(),
    ));
    let shaped = SceneDocumentCandidate::decode_json_bytes_with_limits(
        br#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"test","version":"0.1.0"},"payload":{"roots":[],"entities":[],"private-document-sentinel":"xxxxxxxxx"}}"#,
        limits,
    ).unwrap_err();
    assert!(!shaped.is_syntax_error());
    assert!(matches!(
        shaped.shape_error(),
        Some(SerdeShapeError::StringLimitExceeded { .. })
    ));
    assert!(!format!("{shaped}").contains("private-document-sentinel"));
}

#[test]
fn ron_syntax_accessor_exposes_location_without_parser_text() {
    let syntax = SceneDocumentCandidate::decode_ron_bytes(
        br#"(kind: "scene", private_document_sentinel: "unterminated)"#,
    )
    .unwrap_err();

    assert!(syntax.is_syntax_error());
    assert!(matches!(syntax.parser_location(), Some((1, column)) if column > 1));
    assert!(syntax.shape_error().is_none());
}

#[test]
fn publication_shape_certificate_matches_full_oracle_for_supported_wire_values() {
    let values = [
        ComponentValue::Null,
        ComponentValue::Bool(true),
        ComponentValue::Bool(false),
        ComponentValue::I64(i64::MIN),
        ComponentValue::U64(u64::MAX),
        ComponentValue::f64(-0.0).unwrap(),
        ComponentValue::f64(f64::MAX).unwrap(),
        ComponentValue::f64(f64::MIN).unwrap(),
        ComponentValue::f64(f64::MIN_POSITIVE).unwrap(),
        ComponentValue::f64(f64::from_bits(1)).unwrap(),
        ComponentValue::String("quotes\" slash\\ newline\n tab\t NUL\0 中文🦀\u{2028}".into()),
        ComponentValue::List(Vec::new()),
        ComponentValue::Map(BTreeMap::new()),
        ComponentValue::map([
            ("", ComponentValue::Null),
            (
                "\"\\\n\0🦀",
                ComponentValue::List(vec![
                    ComponentValue::U64(1),
                    ComponentValue::String("雪".into()),
                ]),
            ),
        ]),
    ];
    for value in values {
        for declared_owner in [false, true] {
            let scene = document(value.clone(), declared_owner);
            let prefab = PrefabDocument::new(scene.roots.clone(), scene.entities.clone());
            assert!(certified(&scene, SceneFileLimits::default()), "{value:?}");
            assert!(certified(&prefab, SceneFileLimits::default()), "{value:?}");
            equivalent(&scene, SceneFileLimits::default());
            equivalent(&prefab, SceneFileLimits::default());
        }
    }
}

#[test]
fn publication_shape_certificate_preserves_each_shape_and_exact_encoding_boundary() {
    let scene = document(
        ComponentValue::map([
            ("key\"\\\n🦀", ComponentValue::String("text\0\t雪".into())),
            (
                "list",
                ComponentValue::List(vec![ComponentValue::Null, ComponentValue::Bool(true)]),
            ),
        ]),
        true,
    );
    let prefab = PrefabDocument::new(scene.roots.clone(), scene.entities.clone());
    shape_boundaries(&scene);
    shape_boundaries(&prefab);
}

#[test]
fn publication_shape_certificate_proves_reference_forms_against_the_complete_oracle() {
    let local = ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: id("child"),
    });
    let persistent = |namespace: &str| {
        ComponentValue::EntityReference(EntityReference::Persistent {
            entity: PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new(namespace).unwrap(),
                PersistentRuntimeId::parse_str("0123456789ABCDEF0123456789ABCDEF").unwrap(),
            ),
        })
    };
    for value in [
        local.clone(),
        persistent("game.rooms"),
        persistent(&"n".repeat(256)),
        ComponentValue::map([
            (
                "nested",
                ComponentValue::List(vec![
                    local,
                    ComponentValue::map([("reference", persistent("game.other"))]),
                ]),
            ),
            ("preserved", ComponentValue::String("quoted\"\\ 雪".into())),
        ]),
    ] {
        for declared_owner in [false, true] {
            let scene = document(value.clone(), declared_owner);
            let prefab = PrefabDocument::new(scene.roots.clone(), scene.entities.clone());
            assert!(certified(&scene, SceneFileLimits::default()), "{value:?}");
            assert!(certified(&prefab, SceneFileLimits::default()), "{value:?}");
            shape_boundaries(&scene);
            shape_boundaries(&prefab);
        }
    }
}

fn shape_boundaries<T: SceneFilePayload>(payload: &T) {
    let base = SceneFileLimits::default();
    let shape = base.shape();
    for maximum in 1..=80 {
        let limits = base.with_shape(SerdeShapeLimits::new(
            DepthLimit::new(maximum).unwrap(),
            shape.nodes(),
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
        equivalent(payload, limits);
    }
    for maximum in 1..=256 {
        let limits = base.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            ItemLimit::new(maximum).unwrap(),
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
        equivalent(payload, limits);
    }
    for maximum in 1..=8 {
        let limits = base.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            ItemLimit::new(maximum).unwrap(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
        equivalent(payload, limits);
    }
    for maximum in (1..=40).chain([255, 256, 257]) {
        let limits = base.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            ByteLimit::new(maximum).unwrap(),
            shape.total_string_bytes(),
        ));
        equivalent(payload, limits);
    }
    for maximum in 1..=768 {
        let limits = base.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            shape.string_bytes(),
            ByteLimit::new(maximum).unwrap(),
        ));
        equivalent(payload, limits);
    }
    encoded_boundaries(payload);
}

#[test]
fn publication_shape_certificate_falls_back_without_rejecting_valid_unsupported_values() {
    let mut embedded = document(ComponentValue::Null, false);
    embedded.entities[0].prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: Some(crate::PrefabOverlay {
            components: vec![crate::PrefabComponentIntent::Properties {
                target: crate::PrefabEntityTarget::source(id("nested")),
                component: ComponentTypeId::new("test.Value"),
                version: ComponentSchemaVersion::ONE,
                claims: vec![crate::PrefabPropertyClaim {
                    path: nara_reflect::ComponentPropertyPath::new(
                        nara_reflect::ComponentFieldId::new("value"),
                    ),
                    intent: crate::PrefabPropertyIntent::Set(ComponentValue::String(
                        "override".into(),
                    )),
                    bindings: vec![],
                }],
            }],
            ..crate::PrefabOverlay::new(crate::tests::named_prefab_source_key(
                "publication-shape-base",
            ))
        }),
    });
    let prefab = PrefabDocument::new(embedded.roots.clone(), embedded.entities.clone());
    assert!(!certified(&embedded, SceneFileLimits::default()));
    assert!(!certified(&prefab, SceneFileLimits::default()));
    assert!(reference(&embedded, SceneFileLimits::default()).is_ok());
    assert!(reference(&prefab, SceneFileLimits::default()).is_ok());
    equivalent(&embedded, SceneFileLimits::default());
    equivalent(&prefab, SceneFileLimits::default());
    encoded_boundaries(&embedded);
    encoded_boundaries(&prefab);
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("new")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    assert!(!certified(&patch, SceneFileLimits::default()));
    assert!(reference(&patch, SceneFileLimits::default()).is_ok());
    encoded_boundaries(&patch);
}

#[test]
fn publication_shape_certificate_depth_cutoff_is_fallback_not_a_new_limit() {
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("game.rooms").unwrap(),
            PersistentRuntimeId::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap(),
        ),
    });
    for (mut value, layers, expected_certificate) in [
        (ComponentValue::Null, 13, false),
        (
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("child"),
            }),
            12,
            false,
        ),
        (persistent.clone(), 11, true),
        (persistent, 12, false),
    ] {
        for _ in 0..layers {
            value = ComponentValue::List(vec![value]);
        }
        let scene = document(value, false);
        let prefab = PrefabDocument::new(scene.roots.clone(), scene.entities.clone());
        let base = SceneFileLimits::default();
        let shape = base.shape();
        let limits = base.with_shape(SerdeShapeLimits::new(
            DepthLimit::new(100).unwrap(),
            shape.nodes(),
            shape.container_items(),
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
        assert_eq!(certified(&scene, limits), expected_certificate);
        assert_eq!(certified(&prefab, limits), expected_certificate);
        assert!(
            reference(&scene, limits).is_ok(),
            "{:?}",
            reference(&scene, limits)
        );
        assert!(
            reference(&prefab, limits).is_ok(),
            "{:?}",
            reference(&prefab, limits)
        );
        equivalent(&scene, limits);
        equivalent(&prefab, limits);
    }
}

fn encoded_boundaries<T: SceneFilePayload>(payload: &T) {
    let envelope = canonical_envelope(payload, SceneFileLimits::default()).unwrap();
    let json = serde_json::to_vec_pretty(&envelope).unwrap();
    let ron = ron::ser::to_string_pretty(&envelope, canonical_ron_pretty_config()).unwrap();
    for maximum in [
        json.len() - 1,
        json.len(),
        json.len() + 1,
        ron.len() - 1,
        ron.len(),
        ron.len() + 1,
    ] {
        equivalent(
            payload,
            SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(maximum).unwrap()),
        );
    }
    let limits = SceneFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(json.len().max(ron.len())).unwrap());
    assert!(validate_publication_shape(payload, limits).is_ok());
}

fn document(value: ComponentValue, declared_owner: bool) -> SceneDocument {
    let mut record = SceneComponentRecord::new(ComponentSchemaVersion::ONE, value);
    if declared_owner {
        record.owner = Some(PersistentComponentSchemaOwnerId::try_new("test.owner").unwrap());
    }
    // Deliberately unsorted records: the existing canonical envelope remains the encoding owner.
    SceneDocument {
        roots: vec![id("parent")],
        entities: vec![
            SceneEntityRecord::new(id("parent"))
                .with_children([id("child")])
                .with_component(ComponentTypeId::new("test.Value"), record),
            SceneEntityRecord::new(id("child")),
        ],
    }
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
