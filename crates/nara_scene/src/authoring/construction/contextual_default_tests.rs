//! Public source construction must retain hidden lexical defaults without changing authored data.

use std::sync::Arc;

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, EntityReference,
    PersistentComponentSchemaOwnerId,
};

use super::admit_template_payloads;
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringPropertyValue,
    AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringHistoryLimits, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchApplyLimits,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ContextualConstruction")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.contextual-construction").unwrap()
}

fn field() -> ComponentFieldId {
    ComponentFieldId::new("target-id")
}

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn target() -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: id("socket"),
    })
}

fn initial() -> ComponentValue {
    ComponentValue::map([
        ("target", ComponentValue::Null),
        ("count", ComponentValue::U64(7)),
    ])
}

fn expected_record() -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, initial())
        .with_owner(owner())
        .with_contextual_default(field(), target())
}

fn basis(default: ComponentValue) -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        component(),
        "Contextual construction",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::optional_with_default(
            field(),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
            default,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("count"),
            "Count",
            ComponentFieldPath::from_fields(["count"]),
            ComponentValueKind::U64,
            ComponentValue::U64(42),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
    .with_authoring_initial_value(initial());
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
            .expect("Optional<EntityRef> must qualify for the engine's data Adapter");
    assert!(adapter.contribution().is_none());
    assert_eq!(adapter.components().collect::<Vec<_>>(), [&component()]);
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("host")),
        SceneEntityRecord::new(id("socket")),
    ])
}

fn scene(
    basis: &AuthoringSchemaBasis,
    limits: SceneAuthoringHistoryLimits,
) -> SceneAuthoringSession {
    let encoded = document().to_json_string().unwrap();
    let published = SceneDocumentCandidate::decode_json_str(&encoded)
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/contextual.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(published, limits)
}

fn prefab(
    basis: &AuthoringSchemaBasis,
    limits: SceneAuthoringHistoryLimits,
) -> PrefabAuthoringSession {
    let document = document();
    let document = PrefabDocument::new(document.roots, document.entities);
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/contextual.prefab.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    PrefabAuthoringSession::from_published_authoring_document_with_history_limits(published, limits)
}

macro_rules! each_source {
    ($basis:expr, $limits:expr, |$session:ident| $body:block) => {{
        {
            let mut $session = scene($basis, $limits);
            $body
        }
        {
            let mut $session = prefab($basis, $limits);
            $body
        }
    }};
}

fn read_budget() -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(ItemLimit::new(64).unwrap(), ByteLimit::new(1024).unwrap())
}

#[test]
fn ordinary_add_and_new_entity_templates_preserve_hidden_defaults_absence_and_exact_history() {
    let basis = basis(target());
    for new_entity in [false, true] {
        each_source!(&basis, SceneAuthoringHistoryLimits::default(), |session| {
            let before = session.shared_document();
            let source = session.authoring_source().cloned();
            let template = session.authoring_initial_component(&component()).unwrap();
            assert_eq!(template, expected_record());
            assert_eq!(template.contextual_defaults.len(), 1);
            let created = if new_entity {
                let mut draft = session
                    .begin_authoring_construction(session.revision())
                    .unwrap();
                let local = draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .unwrap();
                draft.add_component(&local, component()).unwrap();
                let prepared = session.prepare_authoring_construction(&draft).unwrap();
                let created = prepared.entity(&local).unwrap().clone();
                assert!(matches!(
                    &prepared.patch().operations[1],
                    ScenePatchOperation::AddComponent { value, .. } if value == &template
                ));
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                session.commit_authoring_construction(prepared).unwrap();
                created
            } else {
                let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                    entity: id("host"),
                    component: component(),
                    value: template,
                }]);
                let prepared = session
                    .prepare_authoring_edit(session.revision(), &patch)
                    .unwrap();
                assert!(session.commit_authoring_edit(prepared).applied);
                id("host")
            };
            let after = session.document().clone();
            let property = ComponentPropertyPath::new(field());
            assert_eq!(
                session
                    .authoring_entity_view(&created)
                    .unwrap()
                    .component(&component())
                    .unwrap()
                    .property_value(&property, &mut read_budget())
                    .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::Null),
            );
            assert_eq!(session.history_status().undo_depth, 1);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), before.as_ref());
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &after);

            // Removing authored presence exposes the retained lexical default, not the initial
            // Null. Undo must recover that exact presence, while Redo must retain its absence.
            let remove = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
                entity: created.clone(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                field: field(),
            }]);
            let prepared = session
                .prepare_authoring_edit(session.revision(), &remove)
                .unwrap();
            assert!(session.commit_authoring_edit(prepared).applied);
            let absent = session.document().clone();
            let record = &absent
                .entities
                .iter()
                .find(|entity| entity.id == created)
                .unwrap()
                .components[&component()];
            assert!(record.value.get("target").is_none());
            assert_eq!(
                record.contextual_defaults,
                expected_record().contextual_defaults,
            );
            assert_eq!(
                session
                    .authoring_entity_view(&created)
                    .unwrap()
                    .component(&component())
                    .unwrap()
                    .property_value(&property, &mut read_budget())
                    .unwrap(),
                AuthoringPropertyValue::Defaulted(&target()),
            );
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &after);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &absent);
            assert_eq!(session.authoring_source(), source.as_ref());
            assert_eq!(session.authoring_schema_basis(), Some(basis.clone()));
            assert_eq!(
                basis
                    .schema(&component())
                    .unwrap()
                    .authoring_initial_value(),
                Some(&initial()),
            );
        });
    }
}

fn history_limits(work: usize, nodes: usize, bytes: usize) -> SceneAuthoringHistoryLimits {
    SceneAuthoringHistoryLimits::new(8, 4096, 4096, 1_048_576, work, nodes, bytes).unwrap()
}

fn expected_patch(with_write: bool) -> ScenePatchDocument {
    let mut patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: id("host"),
        component: component(),
        value: expected_record(),
    }]);
    if with_write {
        patch.operations.push(ScenePatchOperation::SetField {
            entity: id("host"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: field(),
            value: target(),
        });
    }
    patch
}

#[test]
fn public_construction_admits_complete_hidden_default_work_nodes_and_bytes_exactly() {
    let basis = basis(target());
    for with_write in [false, true] {
        let expected = expected_patch(with_write);
        let cost = crate::patch::scene_patch_cost(&expected);
        assert_eq!(cost.operation_count(), if with_write { 3 } else { 2 });
        assert_eq!(
            cost.value_cost().nodes(),
            initial().cost().nodes()
                + target().cost().nodes()
                + if with_write {
                    target().cost().nodes()
                } else {
                    0
                },
        );
        let exact = (
            cost.operation_count(),
            cost.value_cost().nodes(),
            cost.logical_bytes(),
        );
        // The AddComponent retains 59 locator/owner bytes, 19 initial-value bytes and
        // 9 field-ID + 6 default-target bytes. A SetField adds 46 bytes and one value node.
        assert_eq!(exact, if with_write { (3, 5, 139) } else { (2, 4, 93) });
        for (work, nodes, bytes, accepted) in [
            (exact.0, exact.1, exact.2, true),
            (exact.0 - 1, exact.1, exact.2, false),
            (exact.0, exact.1 - 1, exact.2, false),
            (exact.0, exact.1, exact.2 - 1, false),
        ] {
            each_source!(&basis, history_limits(work, nodes, bytes), |session| {
                let before = session.shared_document();
                let revision = session.revision();
                let content = session.content_state();
                let history = session.history_status();
                let usage = session.history_usage();
                let mut draft = session.begin_authoring_construction(revision).unwrap();
                draft.add_component(id("host"), component()).unwrap();
                if with_write {
                    draft
                        .set_property(
                            id("host"),
                            component(),
                            ComponentPropertyPath::new(field()),
                            target(),
                        )
                        .unwrap();
                }
                let result = session.prepare_authoring_construction(&draft);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert_eq!(session.revision(), revision);
                assert_eq!(session.content_state(), content);
                assert_eq!(session.history_status(), history);
                assert_eq!(session.history_usage(), usage);
                if accepted {
                    let prepared = result.unwrap();
                    assert_eq!(prepared.patch(), &expected);
                    let after = prepared.document().clone();
                    session.commit_authoring_construction(prepared).unwrap();
                    assert_eq!(session.history_usage().operations, exact.0);
                    assert_eq!(session.history_usage().value_nodes, exact.1);
                    assert_eq!(session.history_usage().logical_bytes, exact.2);
                    assert!(session.undo_authoring().applied);
                    assert_eq!(session.document(), before.as_ref());
                    assert!(session.redo_authoring().applied);
                    assert_eq!(session.document(), &after);
                } else {
                    let refusal = result.unwrap_err();
                    assert!(!refusal.applied);
                    assert!(refusal.inverse.is_none());
                    assert!(refusal.diagnostics.iter().any(|entry| {
                        entry.code().as_str() == "scene.authoring-construction-template-copy-limit"
                    }));
                }
            });
        }
    }
}

#[test]
fn borrowed_template_gate_matches_replay_cost_including_hidden_default_declarations() {
    let basis = basis(target());
    let session = scene(&basis, SceneAuthoringHistoryLimits::default());
    let template = session
        .authority
        .published()
        .unwrap()
        .initial_component_template(&component())
        .unwrap();
    let entity = id("host");
    let cost = crate::patch::scene_patch_cost(&expected_patch(false));
    let exact = ScenePatchApplyLimits::default()
        .with_validation_work(ItemLimit::new(cost.operation_count()).unwrap())
        .with_validation_value_nodes(ItemLimit::new(cost.value_cost().nodes()).unwrap())
        .with_validation_value_bytes(ByteLimit::new(cost.logical_bytes()).unwrap());
    for (replay, accepted) in [
        (exact, true),
        (
            exact.with_validation_work(ItemLimit::new(cost.operation_count() - 1).unwrap()),
            false,
        ),
        (
            exact.with_validation_value_nodes(
                ItemLimit::new(cost.value_cost().nodes() - 1).unwrap(),
            ),
            false,
        ),
        (
            exact.with_validation_value_bytes(ByteLimit::new(cost.logical_bytes() - 1).unwrap()),
            false,
        ),
    ] {
        assert_eq!(
            admit_template_payloads(
                [],
                [(&entity, template)],
                SceneAuthoringHistoryLimits::default(),
                replay,
            )
            .is_ok(),
            accepted,
        );
    }
}

#[test]
fn context_free_defaults_stay_in_schema_and_raw_records_receive_no_implicit_allowance() {
    let context_free = basis(ComponentValue::Null);
    each_source!(
        &context_free,
        SceneAuthoringHistoryLimits::default(),
        |session| {
            let template = session.authoring_initial_component(&component()).unwrap();
            assert!(template.contextual_defaults.is_empty());
            assert_eq!(template.value, initial());
            let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                entity: id("host"),
                component: component(),
                value: template,
            }]);
            assert!(session.apply_authoring_patch(&patch).applied);
        }
    );

    let contextual = basis(target());
    each_source!(
        &contextual,
        SceneAuthoringHistoryLimits::default(),
        |session| {
            let before = session.shared_document();
            let revision = session.revision();
            let history = session.history_status();
            let raw = SceneComponentRecord::new(ComponentSchemaVersion::ONE, initial())
                .with_owner(owner());
            assert!(raw.contextual_defaults.is_empty());
            let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                entity: id("host"),
                component: component(),
                value: raw,
            }]);
            let report = session.apply_authoring_patch(&patch);
            assert!(!report.applied);
            assert!(report.inverse.is_none());
            assert!(report.diagnostics.iter().any(|entry| {
                entry.code().as_str() == "scene.authoring-contextual-default-invalid"
            }));
            assert!(Arc::ptr_eq(&before, &session.shared_document()));
            assert_eq!(session.revision(), revision);
            assert_eq!(session.history_status(), history);
        }
    );
}
