//! Stored defaults retain relocation obligations even when explicit values hide them.

use nara_asset::{AssetSourceKind, AssetSourcePath};
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::{
        AuthoringAdapterReceipt, PrefabExtractionDestination, PrefabExtractionRequest,
        PrefabExtractionWork, PreparedPrefabExtraction,
    },
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    SceneComponentRecord, SceneEntityPlacement, SceneEntityRecord, ScenePlacement,
    ScenePlacementParent,
};

const P: usize = 0;
const R: usize = 1;

#[test]
fn independent_disk_property_payload_can_remain_unavailable_without_exempting_physical_targets() {
    let basis = basis();
    let mut work = PrefabExtractionWork::new(Default::default()).unwrap();
    work.admit_capture_storage(Some(24 * 1024)).unwrap();
    let p = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("k")),
    ]));
    let u = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("u")),
    ]));
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(PrefabInstance::forest(path("u")));
    let working_r = Arc::new(PrefabDocument::from_ordered_roots([host.clone()]));
    let mut overlay = crate::PrefabOverlay::new(key("u"));
    overlay
        .components
        .push(crate::PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("u")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims: vec![crate::PrefabPropertyClaim {
                path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new("retired")),
                intent: crate::PrefabPropertyIntent::Set(ComponentValue::EntityReference(local(
                    "host",
                ))),
            }],
        });
    host.prefab.as_mut().unwrap().overrides = Some(overlay);
    let disk = [
        captured("p", Arc::clone(&p), &basis),
        captured(
            "r",
            Arc::new(PrefabDocument::from_ordered_roots([host])),
            &basis,
        ),
        captured("u", Arc::clone(&u), &basis),
    ];
    let binding = |name| PrefabExtractionBinding {
        reference: path(name),
        source: key(name),
    };
    let environment = PrefabExtractionEnvironment {
        name: "independent-unavailable-property".into(),
        sources: vec![0, 1, 2],
        before: vec![binding("p"), binding("u")],
        after: vec![binding("p"), binding("u"), binding("q")],
        assets_before: None,
        assets_after: None,
    };
    let mut prepared = PrefabExtractionRequest::new(
        vec![
            captured("p", p, &basis),
            captured("r", working_r, &basis),
            captured("u", u, &basis),
        ],
        vec![environment],
        0,
        basis,
        key("p"),
        id("k"),
        PrefabExtractionDestination {
            key: key("q"),
            reference: path("q"),
            metadata: AssetRecord::new(
                "67cae562-2cfc-4f6e-a7e2-19e2087cd2cc".parse().unwrap(),
                AssetSourcePath::new("prefabs/masked-default/q.ron").unwrap(),
                AssetSourceKind::Prefab,
            ),
        },
    )
    .prepare(work)
    .unwrap();
    let saved = prepared
        .prepare_saved_generation(&disk, &[true, false, false])
        .unwrap();
    assert!(std::ptr::eq(prefab(&saved.sources()[R]), prefab(&disk[R])));
    prepared.require_saved_generation(&saved).unwrap();
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn path(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/masked-default/{name}.ron")).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "94cb577c-8916-453c-8e91-9ef46f3327ad".parse().unwrap(),
        AssetSourcePath::new(format!("prefabs/masked-default/{name}.ron")).unwrap(),
        SourceDocumentKind::Prefab,
    )
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.SavedGuardHiddenDefault")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.saved-guard-hidden-default").unwrap()
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn routed(entity: &str) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("host")),
                expected_base: key("p"),
            }],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id(entity))),
        },
    }
}

fn basis() -> AuthoringSchemaBasis {
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
        ComponentValue::EntityReference(local("observer")),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let schema = ComponentSchema::new(component(), "Hidden default", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    assert!(adapter.contribution().is_none());
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn guard_document(hidden_default: EntityReference) -> PrefabDocument {
    let mut host = SceneEntityRecord::new(id("host"));
    host.prefab = Some(PrefabInstance::forest(path("p")));
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(local("observer")))]),
    )
    .with_owner(owner())
    .with_contextual_default(
        ComponentFieldId::new("target"),
        ComponentValue::EntityReference(hidden_default),
    );
    PrefabDocument::from_ordered_roots([
        host,
        SceneEntityRecord::new(id("observer")).with_component(component(), record),
    ])
}

fn captured(
    name: &str,
    document: Arc<PrefabDocument>,
    basis: &AuthoringSchemaBasis,
) -> CapturedExtractionSource {
    CapturedExtractionSource::Prefab {
        source: ResolvedPrefabSource::bound_authoring_source(document, basis.clone(), key(name)),
    }
}

fn prefab(source: &CapturedExtractionSource) -> &PrefabDocument {
    match source {
        CapturedExtractionSource::Prefab { source } => source.document(),
        CapturedExtractionSource::Scene { .. } => panic!("this fixture has no Scene consumers"),
    }
}

fn fixture(
    hidden_default: EntityReference,
) -> (PreparedPrefabExtraction, [CapturedExtractionSource; 2]) {
    let basis = basis();
    let mut work = PrefabExtractionWork::new(Default::default()).unwrap();
    work.admit_capture_storage(Some(16 * 1024)).unwrap();
    let p = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
        ],
        [
            SceneEntityRecord::new(id("k")),
            SceneEntityRecord::new(id("b")),
        ],
    ));
    let disk = [
        captured("p", Arc::clone(&p), &basis),
        captured("r", Arc::new(guard_document(hidden_default)), &basis),
    ];
    let sources = vec![
        captured("p", p, &basis),
        captured("r", Arc::new(guard_document(local("observer"))), &basis),
    ];
    let binding = || PrefabExtractionBinding {
        reference: path("p"),
        source: key("p"),
    };
    let environment = PrefabExtractionEnvironment {
        name: "prefabs-without-scenes".into(),
        sources: vec![P, R],
        before: vec![binding()],
        after: vec![
            binding(),
            PrefabExtractionBinding {
                reference: path("q"),
                source: key("q"),
            },
        ],
        assets_before: None,
        assets_after: None,
    };
    let request = PrefabExtractionRequest::new(
        sources,
        vec![environment],
        0,
        basis,
        key("p"),
        id("k"),
        PrefabExtractionDestination {
            key: key("q"),
            reference: path("q"),
            metadata: AssetRecord::new(
                "1e2cc7a8-dfce-4c26-950f-ec1d3f18295b".parse().unwrap(),
                AssetSourcePath::new("prefabs/masked-default/q.ron").unwrap(),
                AssetSourceKind::Prefab,
            ),
        },
    );
    (request.prepare(work).unwrap(), disk)
}

#[test]
fn masked_default_guard_refuses_and_explicit_current_save_retries_the_same_preparation() {
    let (mut prepared, disk) = fixture(routed("b"));
    assert_eq!(prepared.source_changed(P), Some(true));
    assert_eq!(prepared.source_changed(R), Some(false));
    let originals = prepared.original_sources().as_ptr();
    let outputs = prepared.sources().as_ptr();
    let original_p = prefab(&prepared.original_sources()[P]) as *const PrefabDocument;
    let working_r = prefab(&prepared.original_sources()[R]) as *const PrefabDocument;
    let disk_r = prefab(&disk[R]) as *const PrefabDocument;
    let q = prepared.extracted().document() as *const PrefabDocument;
    assert_ne!(disk_r, working_r);
    assert!(
        !prefab(&prepared.sources()[P])
            .entities
            .iter()
            .any(|record| record.id == id("b"))
    );
    assert!(
        prepared
            .extracted()
            .document()
            .entities
            .iter()
            .any(|record| record.id == id("b"))
    );
    let before = prepared.usage();

    let failure = prepared
        .prepare_saved_generation(&disk, &[true, false])
        .unwrap_err();
    assert_eq!(failure.environment_index(), Some(0));
    assert_eq!(failure.source_index(), Some(R));
    assert!(failure.compilation_failure().is_none());
    assert!(failure.report().diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.prefab-extraction-saved-guard-relocation"
    }));
    let refused = prepared.usage();
    assert!(refused.work_nodes > before.work_nodes);
    assert!(refused.work_bytes > before.work_bytes);
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    assert_eq!(prepared.sources().as_ptr(), outputs);
    assert_eq!(prepared.extracted().document() as *const _, q);

    // R has no source replacement: selecting its captured working output is SaveCurrent.
    let proof = prepared
        .prepare_saved_generation(&disk, &[true, true])
        .unwrap();
    assert_eq!(proof.published(), [true, true]);
    assert_eq!(prefab(&proof.sources()[R]) as *const _, working_r);
    assert_eq!(proof.extracted().document() as *const _, q);
    prepared.require_saved_generation(&proof).unwrap();
    assert!(prepared.usage().work_nodes > refused.work_nodes);
    assert!(prepared.usage().work_bytes > refused.work_bytes);
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    assert_eq!(prepared.sources().as_ptr(), outputs);
    assert_eq!(
        prefab(&prepared.original_sources()[P]) as *const _,
        original_p
    );
    assert_eq!(prefab(&disk[R]) as *const _, disk_r);
    assert_eq!(prefab(&disk[R]), &guard_document(routed("b")));
    assert_eq!(
        prefab(&prepared.original_sources()[R]),
        &guard_document(local("observer"))
    );
}

#[test]
fn masked_default_to_retained_root_preserves_the_affected_original_disk_guard() {
    let (mut prepared, disk) = fixture(routed("k"));
    assert_eq!(prepared.source_changed(R), Some(false));
    let originals = prepared.original_sources().as_ptr();
    let outputs = prepared.sources().as_ptr();
    let disk_r = prefab(&disk[R]) as *const PrefabDocument;
    let working_r = prefab(&prepared.original_sources()[R]) as *const PrefabDocument;
    let q = prepared.extracted().document() as *const PrefabDocument;
    assert_ne!(disk_r, working_r);
    let before = prepared.usage();

    let proof = prepared
        .prepare_saved_generation(&disk, &[true, false])
        .unwrap();
    assert_eq!(proof.published(), [true, false]);
    assert_eq!(prefab(&proof.sources()[R]) as *const _, disk_r);
    assert_eq!(proof.extracted().document() as *const _, q);
    prepared.require_saved_generation(&proof).unwrap();
    assert!(prepared.usage().work_nodes > before.work_nodes);
    assert!(prepared.usage().work_bytes > before.work_bytes);
    assert_eq!(prepared.original_sources().as_ptr(), originals);
    assert_eq!(prepared.sources().as_ptr(), outputs);
    assert_eq!(prefab(&disk[R]), &guard_document(routed("k")));
    assert_eq!(
        prefab(&prepared.original_sources()[R]),
        &guard_document(local("observer"))
    );
}
