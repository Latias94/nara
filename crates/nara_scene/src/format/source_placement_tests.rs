use super::*;
use crate::{
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceBinding, PrefabInstanceHop,
    ScenePlacement, ScenePlacementSlot,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn json(payload: &str) -> String {
    format!(
        r#"{{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{{"name":"test","version":"0.1.0"}},"payload":{payload}}}"#,
    )
}

fn ron(payload: &str) -> String {
    format!(
        r#"(kind:"scene",format_version:1,engine_min_version:"0.1.0",generator:(name:"test",version:"0.1.0"),payload:{payload})"#,
    )
}

#[test]
fn both_source_wires_require_placements_and_reject_removed_hierarchy_fields() {
    for payload in [
        r#"{"entities":[]}"#,
        r#"{"roots":[],"entities":[]}"#,
        r#"{"placements":[],"roots":[],"entities":[]}"#,
        r#"{"placements":[],"entities":[{"id":"root","children":[]}]}"#,
        r#"{"placements":[],"entities":[{"id":"root","parent":null}]}"#,
    ] {
        let scene = json(payload);
        let prefab = scene.replacen(r#""kind":"scene""#, r#""kind":"prefab""#, 1);
        assert!(SceneDocumentCandidate::decode_json_str(&scene).is_err());
        assert!(PrefabDocumentCandidate::decode_json_str(&prefab).is_err());
    }
    for payload in [
        "(entities:[])",
        "(roots:[],entities:[])",
        "(placements:[],roots:[],entities:[])",
        "(placements:[],entities:[(id:\"root\",children:[])])",
        "(placements:[],entities:[(id:\"root\",parent:None)])",
    ] {
        let scene = ron(payload);
        let prefab = scene.replacen("kind:\"scene\"", "kind:\"prefab\"", 1);
        assert!(SceneDocumentCandidate::decode_ron_str(&scene).is_err());
        assert!(PrefabDocumentCandidate::decode_ron_str(&prefab).is_err());
    }
    assert!(
        SceneDocumentCandidate::decode_json_str(&json(r#"{"placements":[],"entities":[]}"#,))
            .is_ok()
    );
    assert!(SceneDocumentCandidate::decode_ron_str(&ron("(placements:[],entities:[])")).is_ok());
}

#[test]
fn instance_binding_is_required_and_round_trips_both_explicit_modes() {
    assert!(
        serde_json::from_str::<PrefabInstance>(
            r#"{"source":{"kind":"path","value":"missing.prefab.json"}}"#,
        )
        .is_err()
    );
    assert!(
        ron::from_str::<PrefabInstance>(r#"(source:(kind:"path",value:"missing.prefab.json"))"#,)
            .is_err()
    );
    for binding in [
        PrefabInstanceBinding::Forest,
        PrefabInstanceBinding::Root {
            base: crate::tests::named_prefab_source_key("root-binding"),
            root: id("specified-root"),
        },
    ] {
        let instance = PrefabInstance {
            source: nara_asset::AssetRef::path("missing.prefab.json").unwrap(),
            binding,
            arguments: None,
            overrides: None,
        };
        let encoded = serde_json::to_string(&instance).unwrap();
        assert_eq!(
            serde_json::from_str::<PrefabInstance>(&encoded).unwrap(),
            instance
        );
        let encoded = ron::to_string(&instance).unwrap();
        assert_eq!(ron::from_str::<PrefabInstance>(&encoded).unwrap(), instance);
    }
}

fn routed_source() -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance::forest(
        nara_asset::AssetRef::path("missing.prefab.json").unwrap(),
    ));
    let routed = |entity| PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
            expected_base: crate::tests::named_prefab_source_key("unopened"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    };
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("anchor")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("local")),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(id("anchor"))),
                    routed("d2"),
                ),
            ),
            SceneEntityPlacement::new(
                routed("d1"),
                ScenePlacement::first(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("anchor"),
                ))),
            ),
        ],
        [anchor, SceneEntityRecord::new(id("local"))],
    )
}

#[test]
fn raw_decode_retains_routed_intent_without_opening_or_certifying_its_targets() {
    let source = routed_source();
    for candidate in [
        SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap()).unwrap(),
        SceneDocumentCandidate::decode_ron_str(&source.to_ron_string().unwrap()).unwrap(),
    ] {
        assert_eq!(candidate.document, source);
    }
    let mut prefab = PrefabDocument::new(source.placements.clone(), source.entities.clone());
    prefab
        .inputs
        .insert(nara_identity::ReferenceInputId::new("external").unwrap());
    for candidate in [
        PrefabDocumentCandidate::decode_json_str(&prefab.to_json_string().unwrap()).unwrap(),
        PrefabDocumentCandidate::decode_ron_str(&prefab.to_ron_string().unwrap()).unwrap(),
    ] {
        assert_eq!(candidate.into_composition_source().unwrap(), prefab);
    }
    let source = json(r#"{"placements":[],"entities":[{"id":"unplaced"}]}"#).replacen(
        r#""kind":"scene""#,
        r#""kind":"prefab""#,
        1,
    );
    assert!(
        PrefabDocumentCandidate::decode_json_str(&source)
            .unwrap()
            .into_composition_source()
            .is_err()
    );
}

#[test]
fn placement_metadata_uses_inclusive_aggregate_file_budgets() {
    let source = routed_source();
    let mut counts = SceneFileCounts::default();
    counts
        .observe_entities(
            &source.placements,
            &source.entities,
            SceneFileLimits::default(),
        )
        .unwrap();
    let nodes = counts.overlay_nodes + counts.component_value_cost.nodes();
    let bytes = counts.structural_logical_bytes + counts.component_value_cost.logical_bytes();
    let exact = SceneFileLimits::default()
        .with_placements(ItemLimit::new(counts.placements).unwrap())
        .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
        .with_component_value_bytes(ByteLimit::new(bytes).unwrap());
    assert!(source.validate_budget(exact).is_ok());
    for (limits, kind) in [
        (
            exact.with_placements(ItemLimit::new(counts.placements - 1).unwrap()),
            SceneFileBudgetKind::Placements,
        ),
        (
            exact.with_component_value_nodes(ItemLimit::new(nodes - 1).unwrap()),
            SceneFileBudgetKind::ComponentValueNodes,
        ),
        (
            exact.with_component_value_bytes(ByteLimit::new(bytes - 1).unwrap()),
            SceneFileBudgetKind::ComponentValueBytes,
        ),
    ] {
        assert_eq!(source.validate_budget(limits).unwrap_err().kind, kind);
        assert!(matches!(
            SceneDocumentCandidate::decode_json_bytes_with_limits(
                source.to_json_string().unwrap().as_bytes(), limits,
            ),
            Err(SceneFormatError::Budget(error)) if error.kind == kind
        ));
        assert!(matches!(
            SceneDocumentCandidate::decode_ron_bytes_with_limits(
                source.to_ron_string().unwrap().as_bytes(), limits,
            ),
            Err(SceneFormatError::Budget(error)) if error.kind == kind
        ));
    }
}

#[test]
fn placement_patch_snapshots_share_one_file_allowance() {
    let source = routed_source();
    let mut value = source.placements.clone();
    value[1].placement.slot = ScenePlacementSlot::Last;
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPlacements {
        expected: source.placements,
        value,
    }]);
    let exact = SceneFileLimits::default().with_placements(ItemLimit::new(6).unwrap());
    assert!(patch.validate_budget(exact).is_ok());
    let short = exact.with_placements(ItemLimit::new(5).unwrap());
    assert_eq!(
        patch.validate_budget(short).unwrap_err().kind,
        SceneFileBudgetKind::Placements
    );
}
