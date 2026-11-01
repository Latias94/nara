use super::*;

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    AuthoringFieldEdit, AuthoringRecordReadiness, SceneComponentRecord, ScenePatchDocument,
    SchemaReclassificationKind, SchemaReclassificationWork,
    authoring_decode::AuthoringAdapterReceipt,
};

const COMPONENT: &str = "trial.enemy.EnemyStats";

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap()
}

fn entity() -> SceneEntityId {
    SceneEntityId::new("enemy").unwrap()
}

fn schema(next: bool) -> ComponentSchema {
    let health = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["wire_health"]),
            health,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("armor"),
            "Armor",
            ComponentFieldPath::from_fields(["armor"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn admit_basis(
    catalog: ComponentSchemaCatalog,
    previous: Option<ComponentSchemaCatalog>,
) -> AuthoringSchemaBasis {
    let bundle =
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(owner(), catalog, previous)])
            .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn previous_catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![schema(false)],
        ..Default::default()
    }
}

struct Fixture {
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
    upgrade: AuthoredFieldUpgrade,
}

impl Fixture {
    fn new() -> Self {
        let old = previous_catalog();
        let mut next = ComponentSchemaCatalog::successor_of(&old).unwrap();
        next.components.push(schema(true));
        let previous = admit_basis(old.clone(), None);
        let successor = admit_basis(next, Some(old));
        let upgrade = AuthoredFieldUpgrade::prepare(
            previous.clone(),
            successor.clone(),
            ComponentTypeId::new(COMPONENT),
            ComponentFieldId::new("health"),
            ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
            ]),
            &mut ComponentPropertyReadBudget::new(
                ItemLimit::new(4096).unwrap(),
                ByteLimit::new(1024 * 1024).unwrap(),
            ),
        )
        .unwrap();
        Self {
            previous,
            successor,
            upgrade,
        }
    }
}

fn document(next: bool, armor: i64) -> SceneDocument {
    let health = if next {
        ComponentValue::map([("milli", ComponentValue::I64(105000))])
    } else {
        ComponentValue::I64(105)
    };
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(entity()).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion(if next { 2 } else { 1 }),
                ComponentValue::map([
                    ("wire_health", health),
                    ("armor", ComponentValue::I64(armor)),
                ]),
            )
            .with_owner(owner()),
        ),
        SceneEntityRecord::new(SceneEntityId::new("opaque").unwrap()).with_component(
            ComponentTypeId::new("missing.Opaque"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve this unavailable record".into()),
            ),
        ),
    ])
}

fn key(kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        "checkpoint-source",
        kind,
    )
    .unwrap()
}

fn scene_candidate(document: SceneDocument) -> SceneDocumentCandidate {
    SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap()
}

fn prefab_candidate(document: SceneDocument) -> PrefabDocumentCandidate {
    let document = PrefabDocument::new(document.roots, document.entities);
    PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap()).unwrap()
}

fn scene(
    document: SceneDocument,
    basis: &AuthoringSchemaBasis,
    limits: SceneFileLimits,
) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            document.to_json_string().unwrap().as_bytes(),
            limits,
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/checkpoint.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish(),
        history_limits(),
    )
}

fn prefab(
    document: SceneDocument,
    basis: &AuthoringSchemaBasis,
    limits: SceneFileLimits,
) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.roots, document.entities);
    PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            document.to_json_string().unwrap().as_bytes(),
            limits,
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/checkpoint.prefab.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish(),
        history_limits(),
    )
}

fn history_limits() -> SceneAuthoringHistoryLimits {
    SceneAuthoringHistoryLimits::new(8, 128, 1024, 65536, 16, 128, 8192).unwrap()
}

fn edit(armor: i64) -> ScenePatchDocument {
    AuthoringFieldEdit::single(
        entity(),
        ComponentTypeId::new(COMPONENT),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("armor"),
        ComponentValue::I64(armor),
    )
    .unwrap()
    .to_patch()
}

// The callback is one caller-owned cumulative allowance, including discarded preparations.
struct Allowance {
    used: usize,
    limit: usize,
}

impl Allowance {
    fn new() -> Self {
        Self {
            used: 0,
            limit: 64 * 1024 * 1024,
        }
    }

    fn admit(&mut self, _: &'static str, bytes: Option<usize>) -> Result<(), ScenePatchReport> {
        let used = bytes.and_then(|bytes| self.used.checked_add(bytes));
        match used {
            Some(used) if used <= self.limit => {
                self.used = used;
                Ok(())
            }
            _ => Err(history_error_report(
                "test.upgrade-budget",
                "Preparation allowance exhausted",
            )),
        }
    }
}

// Keep the assertions identical while exercising each actual nominal source owner.
macro_rules! each_source {
    ($document:expr, $basis:expr, $limits:expr, |$session:ident, $key:ident, $candidate:ident| $body:block) => {{
        {
            #[allow(unused_mut)]
            let mut $session = scene($document, $basis, $limits);
            let $key = key(SourceDocumentKind::Scene);
            let $candidate = scene_candidate;
            $body
        }
        {
            #[allow(unused_mut)]
            let mut $session = prefab($document, $basis, $limits);
            let $key = key(SourceDocumentKind::Prefab);
            let $candidate = prefab_candidate;
            $body
        }
    }};
}

// The snapshots only observe real admitted sessions; no private state is fabricated for admission.
macro_rules! snapshot {
    ($session:ident) => {
        (
            $session.revision(),
            $session.content_state(),
            $session.history_status(),
            $session.history_usage(),
            $session.source_upgrade_required(),
            $session.authoring_schema_basis(),
        )
    };
}

#[test]
fn prepared_checkpoint_installs_exact_future_source_then_ordinary_edit_and_undo_work() {
    let fixture = Fixture::new();
    let limits = SceneFileLimits::default().with_entities(ItemLimit::new(17).unwrap());
    each_source!(
        document(false, 7),
        &fixture.previous,
        limits,
        |session, source_key, candidate| {
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            assert!(session.apply_authoring_patch(&edit(9)).applied);
            assert!(session.undo_authoring().applied);
            let original = session.shared_document();
            let old_content = session.content_state();
            let old_revision = session.revision();
            let old_source = session.authoring_source().cloned().unwrap();
            let old_manifest = session.unavailable_record_manifest().cloned().unwrap();
            let old_edit = session
                .prepare_authoring_edit(old_revision, &edit(10))
                .unwrap();
            let mut allowance = Allowance::new();
            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    old_revision,
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            assert!(allowance.used > 0);
            assert_eq!(prepared.source_key(), &source_key);
            assert_eq!(prepared.source(), &old_source);
            assert_eq!(prepared.base_revision(), old_revision);
            assert_eq!(prepared.base_content_state(), old_content);
            assert_eq!(prepared.previous_basis(), fixture.previous);
            assert_eq!(prepared.schema_basis(), fixture.successor);
            assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Degraded);
            assert_eq!(prepared.unavailable_record_manifest(), &old_manifest);
            assert!(!prepared.diagnostics().has_errors());
            assert_eq!(prepared.file_limits(), limits);
            assert_eq!(prepared.history_limits(), history_limits());
            assert_ne!(prepared.revision().source_id(), old_revision.source_id());
            assert_ne!(prepared.content_state(), old_content);
            assert!(!session.owns_content_state(prepared.content_state()));
            assert!(Arc::ptr_eq(&original, &session.shared_document()));
            let future_document = prepared.shared_document();
            assert!(std::ptr::eq(prepared.document(), future_document.as_ref()));
            let future_revision = prepared.revision();
            let future_content = prepared.content_state();
            let guard = session
                .bind_authored_schema_upgrade(&source_key, prepared)
                .unwrap();
            assert_eq!(guard.prepared().revision(), future_revision);
            assert_eq!(guard.prepared().content_state(), future_content);
            assert!(Arc::ptr_eq(
                &guard.prepared().shared_document(),
                &future_document
            ));
            guard.commit();
            assert_eq!(session.revision(), future_revision);
            assert_eq!(session.content_state(), future_content);
            assert_eq!(
                session.history_status(),
                super::super::SceneAuthoringHistoryStatus::default()
            );
            assert_eq!(
                session.history_usage(),
                super::super::SceneAuthoringHistoryUsage::default()
            );
            assert_eq!(session.history_limits(), history_limits());
            assert_eq!(
                session.authoring_schema_basis(),
                Some(fixture.successor.clone())
            );
            assert_eq!(session.authoring_source(), Some(&old_source));
            assert!(Arc::ptr_eq(&session.shared_document(), &future_document));
            assert_eq!(session.document().roots, original.roots);
            assert_eq!(session.document().entities[0].id, original.entities[0].id);
            assert_eq!(session.document().entities[1], original.entities[1]);
            // Source preparation minted no saved receipt. An old content observation cannot even
            // acknowledge saving the new incarnation through the existing source API.
            assert!(!session.owns_content_state(old_content));
            assert!(!session.acknowledge_source_saved(old_content));
            assert!(!session.commit_authoring_edit(old_edit).applied);
            assert!(!session.undo_authoring().applied);
            assert!(!session.redo_authoring().applied);
            let next_edit = AuthoringFieldEdit::single(
                entity(),
                ComponentTypeId::new(COMPONENT),
                ComponentSchemaVersion(2),
                ComponentFieldId::new("health"),
                ComponentValue::map([("milli", ComponentValue::I64(106000))]),
            )
            .unwrap();
            assert!(session.apply_authoring_field_edit(next_edit).applied);
            assert_ne!(session.content_state(), future_content);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), future_document.as_ref());
            assert_eq!(session.content_state(), future_content);
            assert_eq!(session.revision().source_id(), future_revision.source_id());
        }
    );
}

#[test]
fn preparation_cancellation_bound_guard_drop_and_budget_refusal_preserve_both_old_branches() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let original_content = session.content_state();
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            assert!(session.apply_authoring_patch(&edit(9)).applied);
            assert!(session.undo_authoring().applied);
            assert_ne!(session.content_state(), original_content);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(session.history_status().redo_depth, 1);
            let before = snapshot!(session);
            let document = session.shared_document();
            let state = session.authority.published_shared().unwrap();
            let mut allowance = Allowance::new();
            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(self::document(true, 8)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            assert_eq!(snapshot!(session), before);
            drop(prepared);
            assert_eq!(snapshot!(session), before);
            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(self::document(true, 8)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            drop(
                session
                    .bind_authored_schema_upgrade(&source_key, prepared)
                    .unwrap(),
            );
            assert_eq!(snapshot!(session), before);
            allowance.limit = allowance.used;
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(self::document(true, 8)),
                        &fixture.upgrade,
                        &mut |stage, bytes| allowance.admit(stage, bytes),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
            assert!(Arc::ptr_eq(&document, &session.shared_document()));
            assert!(Arc::ptr_eq(
                &state,
                &session.authority.published_shared().unwrap()
            ));
            // Follow the actual old edges after cancellation, including the retained Redo branch.
            assert!(session.redo_authoring().applied);
            assert_eq!(
                session.document().entities,
                self::document(false, 9).entities
            );
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), document.as_ref());
            assert!(session.undo_authoring().applied);
            assert_eq!(session.content_state(), original_content);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), document.as_ref());
        }
    );
}

#[test]
fn stale_original_revision_or_nominal_basis_cannot_prepare_or_bind() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let original_revision = session.revision();
            let original_document = session.shared_document();
            let mut allowance = Allowance::new();
            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    original_revision,
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), original_document.as_ref());
            let before = snapshot!(session);
            assert!(
                session
                    .bind_authored_schema_upgrade(&source_key, prepared)
                    .is_err()
            );
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        original_revision,
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut |stage, bytes| allowance.admit(stage, bytes),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);

            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            let independent_previous = admit_basis(previous_catalog(), None);
            assert_ne!(independent_previous, fixture.previous);
            let classification = session
                .prepare_schema_reclassification(
                    &independent_previous,
                    &SchemaReclassificationKind::DeclarationExtension,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap();
            session
                .commit_schema_reclassification(classification)
                .unwrap();
            assert_eq!(session.revision(), before.0);
            let before = snapshot!(session);
            assert!(
                session
                    .bind_authored_schema_upgrade(&source_key, prepared)
                    .is_err()
            );
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut |stage, bytes| allowance.admit(stage, bytes),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );
}

#[test]
fn wrong_kind_foreign_key_unavailable_record_and_native_authority_refuse() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let before = snapshot!(session);
            let mut allowance = Allowance::new();
            let wrong_kind = key(match source_key.kind() {
                SourceDocumentKind::Scene => SourceDocumentKind::Prefab,
                SourceDocumentKind::Prefab => SourceDocumentKind::Scene,
            });
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &wrong_kind,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut |stage, bytes| allowance.admit(stage, bytes),
                    )
                    .is_err()
            );
            let prepared = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut |stage, bytes| allowance.admit(stage, bytes),
                )
                .unwrap();
            let foreign_key = SourceDocumentKey::named(
                "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
                "different-source",
                source_key.kind(),
            )
            .unwrap();
            assert!(
                session
                    .bind_authored_schema_upgrade(&foreign_key, prepared)
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );

    let mut future = document(false, 7);
    future.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .version = ComponentSchemaVersion(3);
    each_source!(
        future.clone(),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            assert!(
                session
                    .authoring_record_facts()
                    .unwrap()
                    .iter()
                    .any(|fact| {
                        fact.component() == &ComponentTypeId::new(COMPONENT)
                            && fact.readiness() == AuthoringRecordReadiness::FutureSchema
                    })
            );
            let before = snapshot!(session);
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut |_, _| Ok(()),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );

    let native = SceneAuthoringSession::new(document(false, 7));
    assert!(
        native
            .prepare_authored_schema_upgrade(
                &key(SourceDocumentKind::Scene),
                native.revision(),
                scene_candidate(document(true, 7)),
                &fixture.upgrade,
                &mut |_, _| Ok(()),
            )
            .is_err()
    );
    let original = document(false, 7);
    let native =
        PrefabAuthoringSession::new(PrefabDocument::new(original.roots, original.entities));
    assert!(
        native
            .prepare_authored_schema_upgrade(
                &key(SourceDocumentKind::Prefab),
                native.revision(),
                prefab_candidate(document(true, 7)),
                &fixture.upgrade,
                &mut |_, _| Ok(()),
            )
            .is_err()
    );
}

#[test]
fn permissive_callback_and_broader_candidate_limits_cannot_replace_original_file_bounds() {
    let fixture = Fixture::new();
    // The original direct values contain four nodes in total; the new health Record adds one.
    let limits = SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(4).unwrap());
    each_source!(
        document(false, 7),
        &fixture.previous,
        limits,
        |session, source_key, candidate| {
            let before = snapshot!(session);
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut |_, _| Ok(()),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );
}

#[test]
fn the_successor_must_remain_saveable_under_original_encoded_file_limits() {
    let fixture = Fixture::new();
    let old = document(false, 7);
    let old_prefab = PrefabDocument::new(old.roots.clone(), old.entities.clone());
    let original_bytes = old
        .to_json_string()
        .unwrap()
        .len()
        .max(old_prefab.to_json_string().unwrap().len());
    let next = document(true, 7);
    let next_prefab = PrefabDocument::new(next.roots.clone(), next.entities.clone());
    assert!(next.to_json_string().unwrap().len() > original_bytes);
    assert!(next_prefab.to_json_string().unwrap().len() > original_bytes);
    let limits =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(original_bytes).unwrap());
    each_source!(
        old.clone(),
        &fixture.previous,
        limits,
        |session, source_key, candidate| {
            let before = snapshot!(session);
            assert!(
                session
                    .prepare_authored_schema_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(next.clone()),
                        &fixture.upgrade,
                        &mut |_, _| Ok(()),
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );
}

#[test]
fn scene_commit_and_cancellation_preserve_other_source_owner_observations() {
    let fixture = Fixture::new();
    let mut session = scene(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
    );
    let live_dirty = session.is_live_dirty();
    let source_upgrade_required = session.source_upgrade_required();
    let source_key = key(SourceDocumentKind::Scene);
    let mut allowance = Allowance::new();
    let prepared = session
        .prepare_authored_schema_upgrade(
            &source_key,
            session.revision(),
            scene_candidate(document(true, 7)),
            &fixture.upgrade,
            &mut |stage, bytes| allowance.admit(stage, bytes),
        )
        .unwrap();
    drop(prepared);
    assert_eq!(session.is_live_dirty(), live_dirty);
    assert_eq!(session.source_upgrade_required(), source_upgrade_required);
    assert!(session.live_instance().is_none());
    let prepared = session
        .prepare_authored_schema_upgrade(
            &source_key,
            session.revision(),
            scene_candidate(document(true, 7)),
            &fixture.upgrade,
            &mut |stage, bytes| allowance.admit(stage, bytes),
        )
        .unwrap();
    session
        .bind_authored_schema_upgrade(&source_key, prepared)
        .unwrap()
        .commit();
    assert!(session.is_live_dirty());
    assert!(session.live_instance().is_none());
    assert_eq!(session.source_upgrade_required(), source_upgrade_required);
}

#[test]
fn bounded_preparation_retains_the_exact_future_checkpoint_for_each_source_kind() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            let before = snapshot!(session);
            let mut work = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let incumbent = work.usage();
            assert!(incumbent.work_bytes > 0);
            assert!(incumbent.peak_retained_bytes > 0);
            let prepared = session
                .prepare_authored_upgrade_with_retained_source(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    retained,
                    &mut work,
                )
                .unwrap();
            assert!(work.usage().work_bytes > incumbent.work_bytes);
            assert!(work.usage().peak_retained_bytes > incumbent.peak_retained_bytes);
            assert_eq!(snapshot!(session), before);
            let future = prepared.content_state();
            session
                .bind_authored_schema_upgrade(&source_key, prepared)
                .unwrap()
                .commit();
            assert_eq!(session.content_state(), future);
            assert_eq!(session.history_status().undo_depth, 0);
            assert_eq!(
                session.authoring_schema_basis(),
                Some(fixture.successor.clone())
            );
        }
    );
}

#[test]
fn a_retained_incumbent_cannot_skip_another_allowances_or_revisions_admission() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let before = snapshot!(session);
            let mut original = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut original)
                .unwrap();
            let mut foreign = AuthoredSchemaUpgradeWork::default();
            assert!(
                session
                    .prepare_authored_upgrade_with_retained_source(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        retained,
                        &mut foreign,
                    )
                    .is_err()
            );
            assert_eq!(foreign.usage(), AuthoredSchemaUpgradeUsage::default());
            assert_eq!(snapshot!(session), before);

            let retained = session
                .retain_authored_schema_upgrade_source(&mut original)
                .unwrap();
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            let after_edit = snapshot!(session);
            assert!(
                session
                    .prepare_authored_upgrade_with_retained_source(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 8)),
                        &fixture.upgrade,
                        retained,
                        &mut original,
                    )
                    .is_err()
            );
            assert_eq!(snapshot!(session), after_edit);
        }
    );
}

#[test]
fn exact_work_and_retention_limits_admit_but_one_short_refuses_without_installing() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let before = snapshot!(session);
            let mut measured = AuthoredSchemaUpgradeWork::default();
            let prepared = session
                .prepare_authored_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut measured,
                )
                .unwrap();
            drop(prepared);
            let required = measured.usage();
            for (work_bytes, retained_bytes, succeeds) in [
                (required.work_bytes, required.peak_retained_bytes, true),
                (required.work_bytes - 1, required.peak_retained_bytes, false),
                (required.work_bytes, required.peak_retained_bytes - 1, false),
            ] {
                let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
                    ByteLimit::new(work_bytes).unwrap(),
                    ByteLimit::new(retained_bytes).unwrap(),
                ));
                let result = session.prepare_authored_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut work,
                );
                assert_eq!(result.is_ok(), succeeds);
                assert_eq!(snapshot!(session), before);
            }
        }
    );
}

mod owned_input;
mod preparation_attempt;
mod recoverable_binding;
mod unopened_source;
