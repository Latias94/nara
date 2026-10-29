//! A separate workspace consumes the same nominal conversion through direct and root exports.

#[path = "support/nested_schema.rs"]
#[allow(
    dead_code,
    reason = "Other test binaries consume the remaining shared schema helpers."
)]
mod nested_schema;

use std::{convert::Infallible, fs, sync::Arc, thread};

use nara_asset::AssetRef;
use nara_engine::scene::{
    PrefabAuthoringSession, PrefabConversionRequest, PreparedPrefabConversion,
};
use nara_reflect::{ComponentTypeId, ComponentValue, EntityReference};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringSchemaBasis,
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PreparedScenePrefabConversion,
    ResolvedPrefabSource, SceneAuthoringSession, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let bundle = nested_schema::bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn document() -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor")).with_children([id("ordinary")]);
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/relationships.prefab.ron").unwrap(),
        overrides: None,
    });
    let mut unrelated = SceneEntityRecord::new(id("unrelated"));
    unrelated.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/missing.prefab.ron").unwrap(),
        overrides: None,
    });
    SceneDocument::new(
        [id("anchor"), id("unrelated")],
        [anchor, unrelated, SceneEntityRecord::new(id("ordinary"))],
    )
}

fn source(basis: &AuthoringSchemaBasis, request: &AssetRef) -> ResolvedPrefabSource {
    assert_eq!(
        request,
        &AssetRef::path("prefabs/relationships.prefab.ron").unwrap()
    );
    let document = nested_schema::document();
    ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::new(document.roots, document.entities)),
        basis.clone(),
        SourceDocumentKey::named(
            "46806b15-ef25-4da2-9983-32078fd5b5a7".parse().unwrap(),
            "consumer.relationships",
            SourceDocumentKind::Prefab,
        )
        .unwrap(),
    )
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public PrefabConversionError is intentionally preserved across the worker boundary."
)]
fn independent_scene_request_runs_off_owner_and_preserves_remapped_data_through_disk_reopen() {
    let basis = basis();
    let original = document();
    let published = SceneDocumentCandidate::decode_json_str(&original.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/consumer.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let request = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap();
    let loader_basis = basis.clone();
    let prepared: PreparedScenePrefabConversion = thread::spawn(move || {
        request.prepare_with_loader(None, |request, _| {
            Ok::<_, Infallible>(Some(source(&loader_basis, request)))
        })
    })
    .join()
    .unwrap()
    .unwrap();
    assert_eq!(session.document(), &original);
    assert_eq!(prepared.entities().len(), 2);
    assert_eq!(prepared.effective_asset_references().len(), 1);
    let mapped = |old| {
        prepared
            .entities()
            .iter()
            .find(|mapping| mapping.provenance().local_entity_id() == &id(old))
            .unwrap()
            .entity()
            .clone()
    };
    let actor = mapped("actor");
    let friend = mapped("friend");
    let reviewed = prepared.document().clone();
    let payload = &reviewed
        .entities
        .iter()
        .find(|record| record.id == actor)
        .unwrap()
        .components[&ComponentTypeId::new(nested_schema::COMPONENT_ID)];
    assert_eq!(
        payload.value.get("targets"),
        Some(&ComponentValue::map([(
            "friends",
            ComponentValue::List(vec![
                ComponentValue::EntityReference(EntityReference::SceneLocal { entity: friend }),
                ComponentValue::Null,
            ]),
        )]))
    );
    let accepted = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| {
            Ok::<_, Infallible>(Some(source(&basis, request)))
        })
        .unwrap();
    assert!(accepted.report().applied);
    assert_eq!(session.document(), &reviewed);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &reviewed);

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("converted.scene.ron");
    fs::write(&path, reviewed.to_ron_string().unwrap()).unwrap();
    let reopened = SceneDocumentCandidate::decode_ron_str(&fs::read_to_string(path).unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/consumer.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    assert_eq!(reopened.document(), &reviewed);
}

#[test]
fn renamed_root_prefab_request_commits_the_reviewed_nominal_candidate() {
    let basis = basis();
    let original = document();
    let original = PrefabDocument::new(original.roots, original.entities);
    let published = PrefabDocumentCandidate::decode_ron_str(&original.to_ron_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/consumer.prefab.ron").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let request: PrefabConversionRequest = session
        .request_complete_prefab_conversion(session.revision(), &id("anchor"), Default::default())
        .unwrap();
    let prepared: PreparedPrefabConversion = request
        .prepare_with_loader(None, |request, _| {
            Ok::<_, Infallible>(Some(source(&basis, request)))
        })
        .unwrap();
    let reviewed = prepared.document().clone();
    let accepted = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| {
            Ok::<_, Infallible>(Some(source(&basis, request)))
        })
        .unwrap();
    assert!(accepted.report().applied);
    assert_eq!(session.document(), &reviewed);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &reviewed);
}
