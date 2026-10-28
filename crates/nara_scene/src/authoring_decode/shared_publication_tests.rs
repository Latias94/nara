use super::*;
use crate::{
    PrefabAuthoringSession, SceneAuthoringHistoryLimits, SceneAuthoringSession,
    SchemaReclassificationWork,
};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
    ComponentValueKind,
};

const BASE: &str = "test.shared.Base";
const RECOVERED: &str = "test.shared.Recovered";

fn owner(name: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(name).unwrap()
}

fn basis(recover: bool) -> AuthoringSchemaBasis {
    let mut declarations = vec![("test.shared.base", BASE)];
    if recover {
        declarations.push(("test.shared.recovered", RECOVERED));
    }
    let bundle = AuthoringSchemaBundle::new(declarations.into_iter().map(|(name, component)| {
        let schema = ComponentSchema::new(
            ComponentTypeId::new(component),
            "Shared value",
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
        AuthoringSchemaBundleOwner::new(
            owner(name),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
    }))
    .unwrap();
    let adapters: Vec<_> = bundle
        .owners()
        .iter()
        .map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, &BTreeSet::new()).unwrap())
        .collect();
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

fn entities() -> [SceneEntityRecord; 1] {
    let record = |name, value| {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::U64(value))]),
        )
        .with_owner(owner(name))
    };
    [SceneEntityRecord::new(SceneEntityId::new("root").unwrap())
        .with_component(ComponentTypeId::new(BASE), record("test.shared.base", 7))
        .with_component(
            ComponentTypeId::new(RECOVERED),
            record("test.shared.recovered", 11),
        )]
}

fn scene_candidate(basis: &AuthoringSchemaBasis) -> SceneAuthoringDocumentCandidate {
    SceneDocumentCandidate::decode_json_str(
        &SceneDocument::from_ordered_roots(entities())
            .to_json_string()
            .unwrap(),
    )
    .unwrap()
    .admit_authoring_with_basis(
        AuthoringDocumentSource::new(AssetRef::path("scenes/shared.scene.json").unwrap()),
        basis,
    )
    .unwrap()
}

fn prefab_candidate(basis: &AuthoringSchemaBasis) -> PrefabAuthoringDocumentCandidate {
    PrefabDocumentCandidate::decode_json_str(
        &PrefabDocument::from_ordered_roots(entities())
            .to_json_string()
            .unwrap(),
    )
    .unwrap()
    .admit_authoring_with_basis(
        AuthoringDocumentSource::new(AssetRef::path("prefabs/shared.prefab.json").unwrap()),
        basis,
    )
    .unwrap()
}

fn edit(component: &str, value: u64) -> AuthoringFieldEdit {
    AuthoringFieldEdit::single(
        SceneEntityId::new("root").unwrap(),
        ComponentTypeId::new(component),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("value"),
        ComponentValue::U64(value),
    )
    .unwrap()
}

// Exercise both nominal source APIs with exactly the same ownership assertions.
macro_rules! each_source {
    ($basis:expr, |$candidate:ident, $fork:ident, $session:ident| $body:block) => {{
        {
            let $candidate = scene_candidate($basis);
            let $fork = fork_published_authoring_scene_document;
            type $session = SceneAuthoringSession;
            $body
        }
        {
            let $candidate = prefab_candidate($basis);
            let $fork = fork_published_authoring_prefab_document;
            type $session = PrefabAuthoringSession;
            $body
        }
    }};
}

#[test]
fn publication_fork_and_sessions_transfer_exact_shared_handles() {
    let basis = basis(false);
    each_source!(&basis, |candidate, fork, Session| {
        let entities = candidate.document.entities.as_ptr();
        let facts = candidate.state.record_facts.as_ptr();
        let published = candidate.publish();
        assert_eq!(published.document.entities.as_ptr(), entities);
        assert_eq!(published.state.record_facts.as_ptr(), facts);
        assert_eq!(Arc::strong_count(&published.document), 1);
        assert_eq!(Arc::strong_count(&published.state), 1);

        let document = Arc::clone(&published.document);
        let state = Arc::clone(&published.state);
        let forked = fork(&published);
        assert!(Arc::ptr_eq(&forked.document, &document));
        assert!(Arc::ptr_eq(&forked.state, &state));
        let (transferred_document, transferred_state) = forked.into_parts();
        assert!(Arc::ptr_eq(&transferred_document, &document));
        assert!(Arc::ptr_eq(&transferred_state, &state));
        assert_eq!(Arc::strong_count(&state), 3);
        drop((transferred_document, transferred_state));

        let first = Session::from_published_authoring_document(fork(&published));
        let limits = SceneAuthoringHistoryLimits::new(8, 16, 128, 4096, 16, 128, 4096).unwrap();
        let second =
            Session::from_published_authoring_document_with_history_limits(published, limits);
        assert!(Arc::ptr_eq(&first.shared_document(), &document));
        assert!(Arc::ptr_eq(&second.shared_document(), &document));
        // Each Session retains the original classification Arc, including its source member.
        assert_eq!(Arc::strong_count(&state), 3);
        assert!(std::ptr::eq(
            first.authoring_source().unwrap(),
            &state.source
        ));
        assert!(std::ptr::eq(
            second.authoring_source().unwrap(),
            &state.source
        ));
        assert_ne!(first.revision().source_id(), second.revision().source_id());
        assert!(!first.owns_content_state(second.content_state()));
        assert!(!second.owns_content_state(first.content_state()));
        assert_eq!(
            first.history_limits(),
            SceneAuthoringHistoryLimits::default()
        );
        assert_eq!(second.history_limits(), limits);
        assert_eq!(first.history_status().undo_depth, 0);
        assert_eq!(first.history_status().redo_depth, 0);
        assert_eq!(second.history_status().undo_depth, 0);
        assert_eq!(second.history_status().redo_depth, 0);
    });
}

#[test]
fn editing_shared_sources_preserves_snapshots_and_independent_history() {
    let basis = basis(false);
    each_source!(&basis, |candidate, fork, Session| {
        let published = candidate.publish();
        let document = Arc::clone(&published.document);
        let state = Arc::clone(&published.state);
        let original_facts = published.record_facts().to_vec();
        let mut first = Session::from_published_authoring_document(fork(&published));
        let mut sibling = Session::from_published_authoring_document(fork(&published));
        let sibling_revision = sibling.revision();
        let sibling_content = sibling.content_state();
        let sibling_history = sibling.history_status();

        assert!(first.apply_authoring_field_edit(edit(BASE, 42)).applied);
        assert!(!Arc::ptr_eq(&first.shared_document(), &document));
        assert!(!std::ptr::eq(
            first.authoring_source().unwrap(),
            &state.source
        ));
        assert_eq!(first.history_status().undo_depth, 1);
        assert_eq!(published.document(), document.as_ref());
        assert_eq!(published.record_facts(), original_facts);
        assert!(Arc::ptr_eq(&published.state, &state));
        assert!(Arc::ptr_eq(&sibling.shared_document(), &document));
        assert!(std::ptr::eq(
            sibling.authoring_source().unwrap(),
            &state.source
        ));
        assert_eq!(sibling.document(), document.as_ref());
        assert_eq!(sibling.revision(), sibling_revision);
        assert_eq!(sibling.content_state(), sibling_content);
        assert_eq!(sibling.history_status(), sibling_history);
        assert_eq!(sibling.authoring_record_facts().unwrap(), original_facts);
        assert_eq!(
            sibling.unavailable_record_manifest(),
            Some(published.unavailable_manifest())
        );

        assert!(first.undo_authoring().applied);
        assert_eq!(first.document(), document.as_ref());
        assert_eq!(first.history_status().redo_depth, 1);
        assert_eq!(sibling.history_status(), sibling_history);
        assert!(first.redo_authoring().applied);
        let first_document = first.shared_document();
        let first_revision = first.revision();
        let first_history = first.history_status();
        assert!(sibling.apply_authoring_field_edit(edit(BASE, 29)).applied);
        assert_ne!(sibling.document(), first.document());
        assert_ne!(sibling.document(), published.document());
        assert!(Arc::ptr_eq(&first.shared_document(), &first_document));
        assert_eq!(first.revision(), first_revision);
        assert_eq!(first.history_status(), first_history);
        assert!(sibling.undo_authoring().applied);
        assert_eq!(sibling.document(), document.as_ref());
        assert_eq!(first.revision(), first_revision);
        assert_eq!(published.document(), document.as_ref());
        assert_eq!(published.record_facts(), original_facts);
    });
}

#[test]
fn reclassifying_one_owner_preserves_the_published_predecessor_and_sibling() {
    let previous = basis(false);
    let successor = basis(true);
    each_source!(&previous, |candidate, fork, Session| {
        let published = candidate.publish();
        let document = Arc::clone(&published.document);
        let state = Arc::clone(&published.state);
        let original_facts = published.record_facts().to_vec();
        let mut first = Session::from_published_authoring_document(fork(&published));
        let sibling = Session::from_published_authoring_document(fork(&published));
        let revision = first.revision();
        let content = first.content_state();
        let history = first.history_status();
        let sibling_revision = sibling.revision();
        let sibling_history = sibling.history_status();
        let prepared = first
            .prepare_schema_reclassification(&successor, &mut SchemaReclassificationWork::default())
            .unwrap();
        assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Complete);
        assert!(std::ptr::eq(
            first.authoring_source().unwrap(),
            &state.source
        ));
        first.commit_schema_reclassification(prepared).unwrap();

        assert!(Arc::ptr_eq(&first.shared_document(), &document));
        assert!(!std::ptr::eq(
            first.authoring_source().unwrap(),
            &state.source
        ));
        assert_eq!(first.authoring_schema_basis(), Some(successor.clone()));
        assert_eq!(
            first.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Complete)
        );
        assert_eq!(first.revision(), revision);
        assert_eq!(first.content_state(), content);
        assert_eq!(first.history_status(), history);
        assert_eq!(published.schema_basis(), previous);
        assert_eq!(published.readiness(), AuthoringDocumentReadiness::Degraded);
        assert_eq!(published.record_facts(), original_facts);
        assert!(Arc::ptr_eq(&published.state, &state));
        assert!(Arc::ptr_eq(&sibling.shared_document(), &document));
        assert!(std::ptr::eq(
            sibling.authoring_source().unwrap(),
            &state.source
        ));
        assert_eq!(sibling.authoring_schema_basis(), Some(previous.clone()));
        assert_eq!(sibling.revision(), sibling_revision);
        assert_eq!(sibling.history_status(), sibling_history);
        assert_eq!(sibling.authoring_record_facts().unwrap(), original_facts);
        assert_eq!(
            sibling.unavailable_record_manifest(),
            Some(published.unavailable_manifest())
        );

        assert!(
            first
                .apply_authoring_field_edit(edit(RECOVERED, 99))
                .applied
        );
        assert_eq!(published.document(), document.as_ref());
        assert_eq!(sibling.document(), document.as_ref());
        assert_eq!(published.record_facts(), original_facts);
        let reopened_owner = Session::from_published_authoring_document(fork(&published));
        assert!(Arc::ptr_eq(&reopened_owner.shared_document(), &document));
        assert!(std::ptr::eq(
            reopened_owner.authoring_source().unwrap(),
            &state.source
        ));
        assert_eq!(
            reopened_owner.authoring_schema_basis(),
            Some(previous.clone())
        );
        assert_eq!(
            reopened_owner.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        assert_ne!(
            reopened_owner.revision().source_id(),
            sibling_revision.source_id()
        );
    });
}
