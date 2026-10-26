use super::*;
use nara_reflect::{
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
};
use nara_scene::{SceneComponentRecord, SceneEntityId};

#[test]
fn exact_instance_accounting_keeps_equal_snapshots_and_record_owner_payloads() {
    let owner = PersistentComponentSchemaOwnerId::try_new("test.snapshot-owner").unwrap();
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::String("full retained record".repeat(64)),
    );
    let declaration = |record| PrefabInstance {
        source: AssetRef::path("retained/base.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: SceneEntityId::new("nested").unwrap(),
            component: ComponentTypeId::new("test.Retained"),
            value: record,
        }]),
    };
    let unclaimed = declaration(record.clone());
    let claimed = declaration(record.with_owner(owner.clone()));
    assert_eq!(
        prefab_instance_retained_bytes(&claimed).unwrap()
            - prefab_instance_retained_bytes(&unclaimed).unwrap(),
        owner.as_str().len(),
    );
    let patch = |expected, value| {
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected,
            value,
        }])
    };
    let empty = patch_retained_bytes(&patch(None, None)).unwrap();
    let prior = patch_retained_bytes(&patch(Some(claimed.clone()), None)).unwrap() - empty;
    let next = patch_retained_bytes(&patch(None, Some(claimed.clone()))).unwrap() - empty;
    assert!(prior > "full retained record".len() * 64);
    assert_eq!(prior, next);
    assert_eq!(
        patch_retained_bytes(&patch(Some(claimed.clone()), Some(claimed))).unwrap() - empty,
        prior + next,
    );
}
