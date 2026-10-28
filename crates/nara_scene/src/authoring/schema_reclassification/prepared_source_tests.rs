use super::*;
use crate::{
    __private::{
        AuthoringAdapterReceipt, PreparedPrefabSchemaSource, PreparedSceneSchemaSource,
        borrow_prepared_prefab_schema_source, borrow_prepared_scene_schema_source,
    },
    AuthoringDocumentReadiness, AuthoringDocumentSource, PrefabDocumentCandidate,
    SceneComponentRecord, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFileLimits, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};
use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};

fn previous() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn successor() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.PreparedSource"),
        "Prepared source",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.prepared-source").unwrap(),
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(SceneEntityId::new("root").unwrap())
        .with_component(
            ComponentTypeId::new("test.PreparedSource"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::U64(7))]),
            ),
        )])
}

fn file_limits() -> SceneFileLimits {
    SceneFileLimits::default().with_entities(ItemLimit::new(17).unwrap())
}

fn scene(basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            document().to_json_string().unwrap().as_bytes(),
            file_limits(),
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(
                AssetRef::path("scenes/prepared-source.scene.json").unwrap(),
            ),
            basis,
        )
        .unwrap()
        .publish(),
    )
}

fn prefab(basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let document = document();
    let document = PrefabDocument::new(document.roots, document.entities);
    PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            document.to_json_string().unwrap().as_bytes(),
            file_limits(),
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(
                AssetRef::path("prefabs/prepared-source.prefab.json").unwrap(),
            ),
            basis,
        )
        .unwrap()
        .publish(),
    )
}

// Exercise the same acceptance contract through each nominal type and its hidden export.
macro_rules! each_source {
    ($basis:expr, |$session:ident, $borrow:ident| $body:block) => {{
        {
            #[allow(unused_mut)]
            let mut $session = scene($basis);
            let $borrow = borrow_prepared_scene_schema_source;
            $body
        }
        {
            #[allow(unused_mut)]
            let mut $session = prefab($basis);
            let $borrow = borrow_prepared_prefab_schema_source;
            $body
        }
    }};
}

#[test]
fn validated_views_share_original_arcs_and_borrow_only_successor_classification() {
    let previous = previous();
    let successor = successor();
    let equal_successor = self::successor();
    assert_eq!(
        successor.schema(&ComponentTypeId::new("test.PreparedSource")),
        equal_successor.schema(&ComponentTypeId::new("test.PreparedSource"))
    );
    assert_ne!(successor, equal_successor);
    each_source!(&previous, |session, borrow| {
        let source = session.shared_document();
        let source_count = Arc::strong_count(&source);
        let revision = session.revision();
        let history = session.history_status();
        let mut work = SchemaReclassificationWork::default();
        let prepared = session
            .prepare_schema_reclassification(&successor, &mut work)
            .unwrap();
        let usage = work.usage();
        assert_eq!(Arc::strong_count(&source), source_count + 1);
        let view = borrow(&session, &prepared).unwrap();
        assert_eq!(Arc::strong_count(&source), source_count + 1);
        assert!(std::ptr::eq(view.document(), source.as_ref()));
        let shared = view.shared_document();
        assert!(Arc::ptr_eq(&source, &shared));
        assert_eq!(Arc::strong_count(&source), source_count + 2);
        assert_eq!(view.schema_basis(), successor);
        assert_ne!(view.schema_basis(), equal_successor);
        assert_eq!(view.base_revision(), revision);
        assert_eq!(view.file_limits(), file_limits());
        assert_eq!(Some(view.source()), session.authoring_source());
        assert_eq!(view.readiness(), AuthoringDocumentReadiness::Complete);
        assert_eq!(
            view.unavailable_record_manifest(),
            prepared.unavailable_record_manifest()
        );
        assert_eq!(view.diagnostics(), prepared.diagnostics());
        assert_eq!(view.unavailable_record_manifest().total_count(), 0);
        assert_eq!(
            session.unavailable_record_manifest().unwrap().total_count(),
            1
        );
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
        assert_eq!(work.usage(), usage);
    });
}

#[test]
fn views_and_commit_bind_reject_the_same_edited_revision() {
    let previous = previous();
    let successor = successor();
    each_source!(&previous, |session, borrow| {
        let prepared = session
            .prepare_schema_reclassification(&successor, &mut SchemaReclassificationWork::default())
            .unwrap();
        let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(SceneEntityId::new("newer").unwrap()),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]);
        assert!(session.apply_authoring_patch(&patch).applied);
        let source = session.shared_document();
        let rejected_view = borrow(&session, &prepared).unwrap_err();
        let rejected_bind = session.bind_schema_reclassification(prepared).unwrap_err();
        assert_eq!(rejected_view.diagnostics, rejected_bind.diagnostics);
        assert!(Arc::ptr_eq(&source, &session.shared_document()));
        assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
    });
}

#[test]
fn views_reject_foreign_sources_even_with_equal_contents_and_the_same_bases() {
    let previous = previous();
    let successor = successor();
    macro_rules! check {
        ($factory:ident, $borrow:ident) => {{
            let first = $factory(&previous);
            let mut foreign = $factory(&previous);
            assert_eq!(first.document(), foreign.document());
            assert_eq!(first.authoring_source(), foreign.authoring_source());
            assert_eq!(
                first.authoring_schema_basis(),
                foreign.authoring_schema_basis()
            );
            let prepared = first
                .prepare_schema_reclassification(
                    &successor,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap();
            let rejected_view = $borrow(&foreign, &prepared).unwrap_err();
            let rejected_bind = foreign.bind_schema_reclassification(prepared).unwrap_err();
            assert_eq!(rejected_view.diagnostics, rejected_bind.diagnostics);
        }};
    }
    check!(scene, borrow_prepared_scene_schema_source);
    check!(prefab, borrow_prepared_prefab_schema_source);
}

#[test]
fn views_reject_reclassified_sources_without_a_revision_or_document_change() {
    let previous = previous();
    let successor = successor();
    each_source!(&previous, |session, borrow| {
        let stale = session
            .prepare_schema_reclassification(&successor, &mut SchemaReclassificationWork::default())
            .unwrap();
        let accepted = session
            .prepare_schema_reclassification(&successor, &mut SchemaReclassificationWork::default())
            .unwrap();
        let revision = session.revision();
        let source = session.shared_document();
        session.commit_schema_reclassification(accepted).unwrap();
        assert_eq!(session.revision(), revision);
        assert!(Arc::ptr_eq(&source, &session.shared_document()));
        let rejected_view = borrow(&session, &stale).unwrap_err();
        let rejected_bind = session.bind_schema_reclassification(stale).unwrap_err();
        assert_eq!(rejected_view.diagnostics, rejected_bind.diagnostics);
        assert_eq!(session.authoring_schema_basis(), Some(successor.clone()));
    });
}

#[test]
fn independently_readmitted_equal_previous_basis_invalidates_prepared_source_reads() {
    let previous = previous();
    let equal_previous = self::previous();
    assert_ne!(previous, equal_previous);
    let successor = successor();
    each_source!(&previous, |session, borrow| {
        let stale = session
            .prepare_schema_reclassification(&successor, &mut SchemaReclassificationWork::default())
            .unwrap();
        let replacement = session
            .prepare_schema_reclassification(
                &equal_previous,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        let revision = session.revision();
        session.commit_schema_reclassification(replacement).unwrap();
        assert_eq!(session.revision(), revision);
        assert!(borrow(&session, &stale).is_err());
        assert!(session.bind_schema_reclassification(stale).is_err());
        assert_eq!(
            session.authoring_schema_basis(),
            Some(equal_previous.clone())
        );
    });
}

#[test]
fn hidden_exports_preserve_both_nominal_view_signatures() {
    // These signatures also check that Scene and Prefab source handles remain nominal.
    let _: for<'a> fn(
        &'a SceneAuthoringSession,
        &'a PreparedSceneSchemaReclassification,
    ) -> Result<PreparedSceneSchemaSource<'a>, ScenePatchReport> =
        borrow_prepared_scene_schema_source;
    let _: for<'a> fn(
        &'a PrefabAuthoringSession,
        &'a PreparedPrefabSchemaReclassification,
    ) -> Result<PreparedPrefabSchemaSource<'a>, ScenePatchReport> =
        borrow_prepared_prefab_schema_source;
}
