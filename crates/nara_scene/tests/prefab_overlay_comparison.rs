#![cfg(feature = "serde")]

// Private semantic comparison; no production overlay format or public API is added.
use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringPropertyEdit,
    AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneComponentRecord, SceneEntityId, SceneEntityRecord, ScenePatchReport, SourceDocumentKey,
    SourceDocumentKind,
};

#[derive(Debug, Clone, PartialEq)]
struct PropertyIntent {
    writes: Vec<AuthoringPropertyEdit>,
}

#[derive(Debug)]
struct PreparedProperties {
    source: SourceDocumentKey,
    base: PrefabDocument,
    schema: AuthoringSchemaBasis,
    intent: PropertyIntent,
    effective: Result<PrefabDocument, ScenePatchReport>,
}

impl PreparedProperties {
    fn prepare(
        source: &SourceDocumentKey,
        base: &PrefabAuthoringSession,
        intent: PropertyIntent,
    ) -> Self {
        let effective = base
            .prepare_authoring_property_edit(base.revision(), &intent.writes)
            .map(|prepared| prepared.document().clone());
        Self {
            source: source.clone(),
            base: base.document().clone(),
            schema: base.authoring_schema_basis().unwrap(),
            intent,
            effective,
        }
    }

    // Full equality is the small experiment's oracle, not a selected production closure cache.
    fn activate(
        self,
        source: &SourceDocumentKey,
        current_base: &PrefabAuthoringSession,
    ) -> Result<PrefabDocument, Box<Self>> {
        if self.source != *source
            || self.base != *current_base.document()
            || Some(&self.schema) != current_base.authoring_schema_basis().as_ref()
            || self.effective.is_err()
        {
            return Err(Box::new(self));
        }
        Ok(self.effective.unwrap())
    }
}

fn source(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "61ee0316-c40c-40ec-ad2e-c8e425d9dd3d".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn id() -> SceneEntityId {
    SceneEntityId::new("object").unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.OverlayPosition")
}

fn bundle(version: ComponentSchemaVersion) -> AuthoringSchemaBundle {
    let shape = ComponentValueShape::record_members(["x", "y"].map(|name| {
        ComponentShapeMember::new(format!("wire_{name}"), ComponentValueKind::U64)
            .with_id(ComponentFieldId::new(name))
    }));
    let schema = ComponentSchema::new(component(), "Position", version)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("position"),
            "Position",
            ComponentFieldPath::from_fields(["position"]),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overlay-comparison").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn document(version: ComponentSchemaVersion, x: u64, y: u64) -> PrefabDocument {
    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id()).with_component(
        component(),
        SceneComponentRecord::new(
            version,
            ComponentValue::map([(
                "position",
                ComponentValue::map([
                    ("wire_x", ComponentValue::U64(x)),
                    ("wire_y", ComponentValue::U64(y)),
                ]),
            )]),
        ),
    )])
}

fn open(document: &PrefabDocument, bundle: &AuthoringSchemaBundle) -> PrefabAuthoringSession {
    let adapters = bundle
        .owners()
        .iter()
        .map(|owner| {
            AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
        })
        .collect::<Vec<_>>();
    let published =
        PrefabDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/base.prefab.json").unwrap()),
                bundle,
                &adapters,
            )
            .unwrap()
            .publish();
    PrefabAuthoringSession::from_published_authoring_document(published)
}

fn write(member: &str, value: u64, version: ComponentSchemaVersion) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        entity: id(),
        component: component(),
        component_version: version,
        property: ComponentPropertyPath::new(ComponentFieldId::new("position"))
            .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(member))]),
        value: ComponentValue::U64(value),
    }
}

fn intent(x: u64) -> PropertyIntent {
    PropertyIntent {
        writes: vec![write("x", x, ComponentSchemaVersion::ONE)],
    }
}

fn change(base: &mut PrefabAuthoringSession, member: &str, value: u64) {
    let prepared = base
        .prepare_authoring_property_edit(
            base.revision(),
            &[write(member, value, ComponentSchemaVersion::ONE)],
        )
        .unwrap();
    assert!(base.commit_authoring_edit(prepared).applied);
}

#[test]
fn original_leaf_intent_inherits_a_changed_sibling_unlike_a_lowered_root_patch() {
    let bundle = bundle(ComponentSchemaVersion::ONE);
    let mut base = open(&document(ComponentSchemaVersion::ONE, 1, 2), &bundle);
    let intent = intent(10);
    let lowered = base
        .prepare_authoring_property_edit(base.revision(), &intent.writes)
        .unwrap()
        .patch()
        .clone();
    change(&mut base, "y", 20);

    let replay = base
        .prepare_authoring_edit(base.revision(), &lowered)
        .unwrap();
    assert_eq!(
        replay.document(),
        &document(ComponentSchemaVersion::ONE, 10, 2)
    );
    let prepared = PreparedProperties::prepare(&source("base"), &base, intent);
    assert_eq!(
        prepared.activate(&source("base"), &base).unwrap(),
        document(ComponentSchemaVersion::ONE, 10, 20)
    );
    assert_eq!(
        base.document(),
        &document(ComponentSchemaVersion::ONE, 1, 20)
    );
}

#[test]
fn an_explicit_equal_value_pin_is_not_removed_as_redundant() {
    let bundle = bundle(ComponentSchemaVersion::ONE);
    let mut base = open(&document(ComponentSchemaVersion::ONE, 1, 2), &bundle);
    let original = intent(1);
    let prepared = PreparedProperties::prepare(&source("base"), &base, original.clone());
    assert_eq!(prepared.intent, original);
    assert_eq!(prepared.effective.as_ref().unwrap(), base.document());
    change(&mut base, "x", 30);
    let stale = prepared.activate(&source("base"), &base).unwrap_err();
    assert_eq!(stale.intent, original);
    let fresh = PreparedProperties::prepare(&source("base"), &base, stale.intent);
    assert_eq!(
        fresh.activate(&source("base"), &base).unwrap(),
        document(ComponentSchemaVersion::ONE, 1, 2)
    );
}

#[test]
fn a_changed_base_rejects_the_old_candidate_but_not_fresh_preparation() {
    let bundle = bundle(ComponentSchemaVersion::ONE);
    let mut base = open(&document(ComponentSchemaVersion::ONE, 1, 2), &bundle);
    let prepared = PreparedProperties::prepare(&source("base"), &base, intent(10));
    change(&mut base, "y", 40);
    let rejected = prepared.activate(&source("base"), &base).unwrap_err();
    assert_eq!(rejected.intent, intent(10));
    let fresh = PreparedProperties::prepare(&source("base"), &base, rejected.intent);
    assert_eq!(
        fresh.activate(&source("base"), &base).unwrap(),
        document(ComponentSchemaVersion::ONE, 10, 40)
    );
}

#[test]
fn canonical_source_and_exact_schema_admission_are_not_just_value_equality() {
    let bundle = bundle(ComponentSchemaVersion::ONE);
    let original = document(ComponentSchemaVersion::ONE, 1, 2);
    let base = open(&original, &bundle);
    let prepared = PreparedProperties::prepare(&source("base"), &base, intent(10));
    let wrong_source = prepared
        .activate(&source("replacement"), &base)
        .unwrap_err();
    assert_eq!(wrong_source.intent, intent(10));
    let other_admission = open(&original, &bundle);
    assert!(
        wrong_source
            .activate(&source("base"), &other_admission)
            .is_err()
    );
    assert_eq!(base.document(), &original);
}

#[test]
fn current_target_identity_does_not_reinterpret_an_old_version_override_value() {
    let version = ComponentSchemaVersion(2);
    let bundle = bundle(version);
    let base = open(&document(version, 1, 2), &bundle);
    let prepared = PreparedProperties::prepare(&source("base"), &base, intent(10));
    assert!(prepared.effective.is_err());
    let conflict = prepared.activate(&source("base"), &base).unwrap_err();
    assert_eq!(conflict.intent, intent(10));
    assert_eq!(base.document(), &document(version, 1, 2));
    assert_eq!(base.history_status().undo_depth, 0);
}

#[test]
fn a_final_unresolvable_intent_preserves_all_input_without_partial_activation() {
    let bundle = bundle(ComponentSchemaVersion::ONE);
    let base = open(&document(ComponentSchemaVersion::ONE, 1, 2), &bundle);
    let revision = base.revision();
    let history = base.history_status();
    let mut original = intent(10);
    original
        .writes
        .push(write("removed", 50, ComponentSchemaVersion::ONE));
    let prepared = PreparedProperties::prepare(&source("base"), &base, original.clone());
    assert!(prepared.effective.is_err());
    let conflict = prepared.activate(&source("base"), &base).unwrap_err();
    assert_eq!(conflict.intent, original);
    assert_eq!(
        base.document(),
        &document(ComponentSchemaVersion::ONE, 1, 2)
    );
    assert_eq!(base.revision(), revision);
    assert_eq!(base.history_status(), history);
}
