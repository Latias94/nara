#![cfg(feature = "serde")]

use nara_identity::{
    AssetSourcePath, AuthoredEntityReference, AuthoredReferenceTarget, EntityReference,
    PrefabEntityOrigin, PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey,
    SourceDocumentKind, StableAssetId,
};
use nara_reflect::ComponentValue;

#[test]
fn authored_value_ron_keeps_nested_discriminants_when_content_precedes_its_tag() {
    let universe = "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap();
    for expected_base in [
        SourceDocumentKey::named(universe, "base", SourceDocumentKind::Prefab).unwrap(),
        SourceDocumentKey::project_location(
            universe,
            AssetSourcePath::new("prefabs/base.ron").unwrap(),
            SourceDocumentKind::Prefab,
        ),
        SourceDocumentKey::stable_asset(
            StableAssetId::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            SourceDocumentKind::Prefab,
        ),
    ] {
        let reference = EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("anchor").unwrap()),
                    expected_base,
                }],
                target: AuthoredReferenceTarget::Input(ReferenceInputId::new("target").unwrap()),
            },
        };
        let payload = ron::to_string(&reference).unwrap();
        let expected = ComponentValue::EntityReference(reference);
        for wire in [
            format!("(type:\"entity_ref\",value:{payload})"),
            format!("(value:{payload},type:\"entity_ref\")"),
        ] {
            assert_eq!(
                ron::from_str::<ComponentValue>(&wire).unwrap(),
                expected,
                "{wire}"
            );
        }
        let wire = ron::to_string(&expected).unwrap();
        assert_eq!(
            ron::from_str::<ComponentValue>(&format!("(value:[{wire}],type:\"list\")")).unwrap(),
            ComponentValue::List(vec![expected.clone()])
        );
        let json = serde_json::to_string(&expected).unwrap();
        assert_eq!(
            serde_json::from_str::<ComponentValue>(&json).unwrap(),
            expected
        );
    }
}
