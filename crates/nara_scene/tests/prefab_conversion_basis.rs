#![cfg(feature = "serde")]

//! Conversion-basis probes through the existing authoring compiler, without a conversion API.

#[path = "support/prefab_source.rs"]
mod prefab_source;

use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_identity::EntityReference;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, AuthoringPrefabExpansionReport, AuthoringPrefabOmission,
        AuthoringPrefabOmissionReason, AuthoringPreviewOmissionLocation,
        AuthoringPreviewOmissionReason, try_expand_authoring_prefabs_with_loader,
    },
    AuthoringDocumentSource, AuthoringRecordReadiness, AuthoringSchemaBasis, ExpandedSceneDocument,
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PrefabProjectionStep,
    PublishedAuthoringPrefabDocument, ResolvedPrefabSource, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/conversion/{name}.prefab.json")).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ConversionBasis")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.conversion-basis").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Conversion basis", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
        ]);
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn record(target: SceneEntityId) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("amount", ComponentValue::U64(7)),
            (
                "target",
                ComponentValue::EntityReference(EntityReference::SceneLocal { entity: target }),
            ),
            (
                "image",
                ComponentValue::map([
                    ("kind", ComponentValue::String("path".into())),
                    ("value", ComponentValue::String("textures/icon.png".into())),
                ]),
            ),
        ]),
    )
    .with_owner(owner())
}

fn prefab(record: SceneComponentRecord) -> PrefabDocument {
    PrefabDocument::new(
        [id("root")],
        [
            SceneEntityRecord::new(id("root"))
                .with_children([id("target")])
                .with_component(component(), record),
            SceneEntityRecord::new(id("target")),
        ],
    )
}

fn publish_prefab(
    document: &PrefabDocument,
    basis: &AuthoringSchemaBasis,
) -> Arc<PublishedAuthoringPrefabDocument> {
    Arc::new(
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(AuthoringDocumentSource::new(source("base")), basis)
            .unwrap()
            .publish(),
    )
}

fn anchor(entity: &str, base: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(entity));
    record.prefab = Some(PrefabInstance {
        source: source(base),
        overrides: None,
    });
    record
}

fn open_scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/conversion.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document(published)
}

fn compile(
    document: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    published: &Arc<PublishedAuthoringPrefabDocument>,
    requested: &mut Vec<AssetRef>,
) -> AuthoringPrefabExpansionReport {
    try_expand_authoring_prefabs_with_loader(
        document,
        basis,
        Default::default(),
        None,
        |request, _| {
            requested.push(request.clone());
            Ok::<_, Infallible>((request == &source("base")).then(|| {
                ResolvedPrefabSource::bound_authoring(
                    Arc::clone(published),
                    prefab_source::key("conversion.base"),
                )
            }))
        },
    )
    .unwrap()
}

fn projected(candidate: &ExpandedSceneDocument, local: &str) -> SceneEntityId {
    let expected = [PrefabProjectionStep::SourceEntity {
        source_document: source("base"),
        canonical_source: prefab_source::key("conversion.base"),
        entity: id(local),
    }];
    let mut matches = candidate.provenance().iter().filter(|(_, origin)| {
        origin.projection_path().is_some_and(|path| {
            path.root_anchor() == &id("selected") && path.steps() == expected.as_slice()
        })
    });
    let (projected, origin) = matches
        .next()
        .expect("selected source entity must be projected");
    assert!(
        matches.next().is_none(),
        "selected projection must be unique"
    );
    assert_eq!(origin.local_entity_id(), &id(local));
    projected.clone()
}

fn entity<'a>(document: &'a SceneDocument, entity: &SceneEntityId) -> &'a SceneEntityRecord {
    document
        .entities
        .iter()
        .find(|record| &record.id == entity)
        .expect("projected entity must exist")
}

#[test]
fn native_preview_loses_available_component_data_and_declared_references() {
    let basis = basis();
    let prefab = prefab(record(id("target")));
    let published = publish_prefab(&prefab, &basis);
    let fact = &published.record_facts()[0];
    assert_eq!(published.schema_basis(), basis);
    assert_eq!(fact.component(), &component());
    assert_eq!(fact.readiness(), AuthoringRecordReadiness::Available);
    assert!(fact.adapter().unwrap().contribution().is_none());
    assert!(!fact.adapter().unwrap().supports_native_preview());
    assert_eq!(published.unavailable_manifest().record_count(), 0);
    let document = SceneDocument::from_ordered_roots([anchor("selected", "base")]);
    let session = open_scene(&document, &basis);
    let revision = session.revision();
    let content = session.content_state();
    let history = session.history_status();
    let mut requested = Vec::new();

    let report = compile(session.document(), &basis, &published, &mut requested);

    assert_eq!(requested, [source("base")]);
    assert!(!report.budget_exceeded());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let candidate = report
        .candidate
        .as_ref()
        .expect("semantic compilation must succeed");
    let root = projected(candidate, "root");
    let target = projected(candidate, "target");
    assert_ne!(target, id("target"));
    let root_record = entity(candidate.document(), &root);
    assert_eq!(root_record.components.len(), 1);
    assert_eq!(root_record.components[&component()], record(target.clone()));
    assert_eq!(root_record.children, [target]);
    let references = report.effective_asset_references();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].field_id(), &ComponentFieldId::new("image"));
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::from_fields(["image"])
    );
    assert_eq!(
        references[0].asset_ref(),
        &AssetRef::path("textures/icon.png").unwrap()
    );
    assert_eq!(
        report.omissions,
        [AuthoringPrefabOmission {
            entity: root.clone(),
            component: component(),
            reason: AuthoringPrefabOmissionReason::NativeBindingUnavailable,
        }]
    );
    let provenance = candidate.provenance().clone();

    let (preview, omissions) = report.into_native_preview().unwrap();

    assert_eq!(preview.provenance(), &provenance);
    assert!(entity(preview.document(), &root).components.is_empty());
    assert_eq!(omissions.len(), 1);
    assert_eq!(
        omissions[0].reason(),
        AuthoringPreviewOmissionReason::NativeBindingUnavailable
    );
    assert_eq!(
        omissions[0].location(),
        &AuthoringPreviewOmissionLocation::ProjectedEntityComponent {
            entity: root.clone(),
            provenance: provenance[&root].clone(),
            component: component(),
        }
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.history_status(), history);
    assert_eq!(session.authoring_schema_basis(), Some(basis));
    assert_eq!(published.document(), &prefab);
}

#[test]
fn unavailable_selected_record_is_an_omission_even_before_native_preview_filtering() {
    let basis = basis();
    let mut unavailable = record(id("target"));
    unavailable.version = ComponentSchemaVersion(2);
    let prefab = prefab(unavailable);
    let published = publish_prefab(&prefab, &basis);
    assert_eq!(
        published.record_facts()[0].readiness(),
        AuthoringRecordReadiness::FutureSchema
    );
    assert_eq!(published.unavailable_manifest().record_count(), 1);
    let manifest = published.unavailable_manifest().clone();
    let document = SceneDocument::from_ordered_roots([anchor("selected", "base")]);
    let session = open_scene(&document, &basis);
    let revision = session.revision();
    let history = session.history_status();
    let mut requested = Vec::new();

    let report = compile(session.document(), &basis, &published, &mut requested);

    assert_eq!(requested, [source("base")]);
    assert!(!report.budget_exceeded());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let candidate = report
        .candidate
        .as_ref()
        .expect("unavailable data is reported separately");
    let root = projected(candidate, "root");
    assert!(entity(candidate.document(), &root).components.is_empty());
    assert!(report.effective_asset_references().is_empty());
    assert_eq!(
        report.omissions,
        [AuthoringPrefabOmission {
            entity: root.clone(),
            component: component(),
            reason: AuthoringPrefabOmissionReason::RecordUnavailable(
                AuthoringRecordReadiness::FutureSchema,
            ),
        }]
    );
    let provenance = candidate.provenance().clone();

    let (preview, omissions) = report.into_native_preview().unwrap();

    assert_eq!(preview.provenance(), &provenance);
    assert!(entity(preview.document(), &root).components.is_empty());
    assert_eq!(omissions.len(), 1);
    assert_eq!(
        omissions[0].reason(),
        AuthoringPreviewOmissionReason::RecordUnavailable(AuthoringRecordReadiness::FutureSchema)
    );
    assert_eq!(
        omissions[0].location(),
        &AuthoringPreviewOmissionLocation::ProjectedEntityComponent {
            entity: root.clone(),
            provenance: provenance[&root].clone(),
            component: component(),
        }
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert_eq!(published.document(), &prefab);
    assert_eq!(published.unavailable_manifest(), &manifest);
}

#[test]
fn unrelated_missing_dependency_blocks_whole_document_compilation_with_a_healthy_selected_anchor() {
    let basis = basis();
    let prefab = prefab(record(id("target")));
    let published = publish_prefab(&prefab, &basis);
    let selected_document = SceneDocument::from_ordered_roots([anchor("selected", "base")]);
    let selected = open_scene(&selected_document, &basis);
    let mut requested = Vec::new();
    let healthy = compile(selected.document(), &basis, &published, &mut requested);
    assert_eq!(requested, [source("base")]);
    assert!(
        !healthy.diagnostics.has_errors(),
        "{:?}",
        healthy.diagnostics
    );
    let candidate = healthy
        .candidate
        .as_ref()
        .expect("selected anchor must be healthy");
    let root = projected(candidate, "root");
    let target = projected(candidate, "target");
    assert_eq!(
        entity(candidate.document(), &root).components[&component()],
        record(target)
    );

    // Whole-document failure is expected; conversion selection needs its own scope decision.
    let document = SceneDocument::from_ordered_roots([
        anchor("selected", "base"),
        anchor("unrelated", "missing"),
    ]);
    let session = open_scene(&document, &basis);
    let revision = session.revision();
    let history = session.history_status();
    requested.clear();

    let report = compile(session.document(), &basis, &published, &mut requested);

    assert_eq!(requested, [source("base"), source("missing")]);
    assert!(!report.budget_exceeded());
    assert!(report.candidate.is_none());
    assert!(report.diagnostics.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-missing" }),
        "{:?}",
        report.diagnostics
    );
    assert!(report.into_native_preview().is_err());
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert_eq!(selected.document(), &selected_document);
    assert_eq!(published.document(), &prefab);
}
