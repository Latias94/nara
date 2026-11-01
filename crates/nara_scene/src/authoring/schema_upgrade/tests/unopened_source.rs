use super::*;

use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceClaim, PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent,
    PrefabReferenceBinding,
};

fn scene_input(document: SceneDocument, limits: SceneFileLimits) -> SceneDocumentCandidate {
    SceneDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
}

fn prefab_input(document: SceneDocument, limits: SceneFileLimits) -> PrefabDocumentCandidate {
    let document = PrefabDocument {
        roots: document.roots,
        entities: document.entities,
    };
    PrefabDocumentCandidate::decode_json_bytes_with_limits(
        document.to_json_string().unwrap().as_bytes(),
        limits,
    )
    .unwrap()
}

macro_rules! each_unopened_source {
    (|$factory:ident, $input:ident, $source:ident, $kind:ident| $body:block) => {{
        {
            let $factory = admit_authored_upgrade_scene_source;
            let $input = scene_input;
            let $source =
                AuthoringDocumentSource::new(AssetRef::path("scenes/unopened.scene.json").unwrap());
            let $kind = SourceDocumentKind::Scene;
            $body
        }
        {
            let $factory = admit_authored_upgrade_prefab_source;
            let $input = prefab_input;
            let $source = AuthoringDocumentSource::new(
                AssetRef::path("prefabs/unopened.prefab.json").unwrap(),
            );
            let $kind = SourceDocumentKind::Prefab;
            $body
        }
    }};
}

fn known_document() -> SceneDocument {
    let mut document = document(false, 7);
    document.roots.retain(|id| id == &entity());
    document.entities.retain(|record| record.id == entity());
    document
}

fn dormant_document() -> SceneDocument {
    let mut document = document(false, 7);
    let target = PrefabEntityTarget::source(SceneEntityId::new("not-loaded").unwrap());
    let mut nested = PrefabOverlay::new(key(SourceDocumentKind::Prefab));
    nested.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: target.clone(),
        component: ComponentTypeId::new(COMPONENT),
        record: SceneComponentRecord::new(
            ComponentSchemaVersion(71),
            ComponentValue::String("preserve this dormant future value".into()),
        )
        .with_owner(owner()),
        bindings: Vec::new(),
    });
    nested.components.push(PrefabComponentIntent::Properties {
        target: target.clone(),
        component: ComponentTypeId::new(COMPONENT),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("retired-property"))
                .with_steps([ComponentPropertyStep::MapEntry("opaque-key".into())]),
            intent: PrefabPropertyIntent::Set(ComponentValue::List(vec![ComponentValue::String(
                "preserve unresolved selector".into(),
            )])),
            bindings: vec![PrefabReferenceBinding {
                slot: ComponentFieldPath::empty(),
                expected: entity(),
                target: target.clone(),
            }],
        }],
    });
    let mut addition = PrefabAddition::new(SceneEntityId::new("dormant-addition").unwrap());
    addition.components.insert(
        ComponentTypeId::new("missing.Dormant"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(47),
            ComponentValue::map([("opaque", ComponentValue::U64(23))]),
        ),
    );
    nested.additions.push(addition);
    let mut outer = PrefabOverlay::new(key(SourceDocumentKind::Prefab));
    outer.instances.push(PrefabInstanceClaim {
        target,
        value: Some(PrefabInstance {
            source: AssetRef::path("prefabs/not-read.prefab.json").unwrap(),
            overrides: Some(nested),
        }),
    });
    document.entities[0].prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/not-present.prefab.json").unwrap(),
        overrides: Some(outer),
    });
    document
}

#[test]
fn factories_retain_known_originals_and_tokens_prepare_without_readmitting_them() {
    let fixture = Fixture::new();
    let original = known_document();
    let limits = SceneFileLimits::default().with_entities(ItemLimit::new(7).unwrap());
    each_unopened_source!(|factory, input, source, kind| {
        let mut work = AuthoredSchemaUpgradeWork::default();
        let (session, retained) = factory(
            input(original.clone(), limits),
            source.clone(),
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        assert_eq!(session.document().roots, original.roots);
        assert_eq!(session.document().entities, original.entities);
        assert_eq!(session.authoring_source(), Some(&source));
        assert_eq!(
            session.authoring_schema_basis(),
            Some(fixture.previous.clone())
        );
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Complete)
        );
        assert_eq!(
            session.history_limits(),
            SceneAuthoringHistoryLimits::default()
        );
        assert_eq!(
            session.history_status(),
            crate::SceneAuthoringHistoryStatus::default()
        );
        assert_eq!(
            session.history_usage(),
            crate::SceneAuthoringHistoryUsage::default()
        );
        assert!(!session.source_upgrade_required());
        assert_eq!(session.authority.published().unwrap().file_limits(), limits);
        assert_eq!(retained.revision, session.revision());
        assert_eq!(retained.previous, fixture.previous);
        assert_eq!(retained.kind, kind);
        assert!(Arc::ptr_eq(&retained.work, &work.identity));
        let before = snapshot!(session);
        let mut next = document(true, 7);
        next.roots.retain(|id| id == &entity());
        next.entities.retain(|record| record.id == entity());
        let prepared = session
            .prepare_authored_upgrade_with_retained_source(
                &key(kind),
                session.revision(),
                input(next, limits),
                &fixture.upgrade,
                retained,
                &mut work,
            )
            .unwrap();
        assert_eq!(prepared.previous_basis(), fixture.previous);
        assert_eq!(prepared.schema_basis(), fixture.successor);
        assert_eq!(prepared.file_limits(), limits);
        assert_eq!(snapshot!(session), before);
    });
}

#[test]
fn factories_preserve_unavailable_and_nested_dormant_inventory_losslessly() {
    let fixture = Fixture::new();
    let document = dormant_document();
    let limits = SceneFileLimits::default();
    each_unopened_source!(|factory, input, source, kind| {
        let mut work = AuthoredSchemaUpgradeWork::default();
        let expected = input(document.clone(), limits)
            .admit_authoring_with_basis(source.clone(), &fixture.previous)
            .unwrap()
            .publish();
        let (session, retained) = factory(
            input(document.clone(), limits),
            source.clone(),
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        assert_eq!(session.document().roots, document.roots);
        assert_eq!(session.document().entities, document.entities);
        assert_eq!(session.authoring_source(), Some(&source));
        assert_eq!(
            session.authoring_schema_basis(),
            Some(expected.schema_basis())
        );
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        assert_eq!(
            session.authoring_record_facts(),
            Some(expected.record_facts())
        );
        assert_eq!(
            session.unavailable_record_manifest(),
            Some(expected.unavailable_manifest())
        );
        for readiness in [
            AuthoringRecordReadiness::UnknownSchema,
            AuthoringRecordReadiness::FutureSchema,
        ] {
            assert!(
                session
                    .authoring_record_facts()
                    .unwrap()
                    .iter()
                    .any(|fact| { fact.readiness() == readiness })
            );
        }
        assert!(
            session
                .unavailable_record_manifest()
                .unwrap()
                .entries()
                .iter()
                .any(|entry| {
                    matches!(entry, crate::UnavailableRecordManifestEntry::OverlayTarget(target)
                if target.readiness() == AuthoringRecordReadiness::UnresolvedOverlay)
                })
        );
        assert_eq!(retained.kind, kind);
        assert_eq!(retained.revision, session.revision());
        assert!(work.usage().work_bytes > 0);
    });
}

#[test]
fn factories_use_exact_limits_and_failed_work_is_never_reset() {
    let fixture = Fixture::new();
    let document = dormant_document();
    let limits = SceneFileLimits::default();
    each_unopened_source!(|factory, input, source, kind| {
        let mut measured = AuthoredSchemaUpgradeWork::default();
        drop(
            factory(
                input(document.clone(), limits),
                source.clone(),
                &fixture.previous,
                &mut measured,
            )
            .unwrap(),
        );
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
            let result = factory(
                input(document.clone(), limits),
                source.clone(),
                &fixture.previous,
                &mut work,
            );
            assert_eq!(result.is_ok(), succeeds, "{kind:?}");
            if succeeds {
                assert_eq!(work.usage(), required);
            } else {
                let report = result.unwrap_err();
                assert!(report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
                }));
                let failed = work.usage();
                assert!(failed.work_bytes > 0);
                assert!(
                    factory(
                        input(document.clone(), limits),
                        source.clone(),
                        &fixture.previous,
                        &mut work,
                    )
                    .is_err()
                );
                assert!(work.usage().work_bytes >= failed.work_bytes);
                assert!(work.usage().peak_retained_bytes >= failed.peak_retained_bytes);
            }
        }
    });
}

#[test]
fn factory_keeps_exactly_one_incumbent_after_classifier_scratch_ends() {
    let fixture = Fixture::new();
    let document = dormant_document();
    let limits = SceneFileLimits::default();
    each_unopened_source!(|factory, input, source, kind| {
        let mut measured = AuthoredSchemaUpgradeWork::default();
        drop(
            factory(
                input(document.clone(), limits),
                source.clone(),
                &fixture.previous,
                &mut measured,
            )
            .unwrap(),
        );
        let peak = measured.usage().peak_retained_bytes;
        let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
            AuthoredSchemaUpgradeLimits::default().work_bytes(),
            ByteLimit::new(peak).unwrap(),
        ));
        let (session, _retained) = factory(
            input(document.clone(), limits),
            source,
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        let raw = crate::authoring::prefab_conversion::retention::document_dynamic_bytes(
            &document.roots,
            &document.entities,
            limits,
        )
        .unwrap();
        let classification = session
            .authority
            .published()
            .unwrap()
            .checked_retained_bytes()
            .unwrap();
        let source_layout = match kind {
            SourceDocumentKind::Scene => {
                size_of::<SceneAuthoringSession>() + size_of::<SceneDocument>()
            }
            SourceDocumentKind::Prefab => {
                size_of::<PrefabAuthoringSession>() + size_of::<PrefabDocument>()
            }
        };
        let incumbent = raw
            + classification
            + source_layout
            + 4 * size_of::<usize>()
            + size_of::<RetainedAuthoredSchemaUpgradeSource>();
        let remaining = peak - incumbent;
        // A real next preparation can use all scratch released by classification, while the
        // complete raw source, classification, empty history and token stay live exactly once.
        work.with_preparation_work("next-owner-exact", Some(remaining), || ())
            .unwrap();
        assert!(
            work.with_preparation_work("next-owner-one-short", Some(remaining + 1), || ())
                .is_err()
        );
        assert_eq!(session.document().entities, document.entities);
    });
}

#[test]
fn structural_refusal_returns_no_session_and_releases_only_its_step_scratch() {
    let fixture = Fixture::new();
    let document = dormant_document();
    let limits = SceneFileLimits::default();
    each_unopened_source!(|factory, input, source, kind| {
        let mut measured = AuthoredSchemaUpgradeWork::default();
        drop(
            factory(
                input(document.clone(), limits),
                source.clone(),
                &fixture.previous,
                &mut measured,
            )
            .unwrap(),
        );
        let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
            AuthoredSchemaUpgradeLimits::default().work_bytes(),
            ByteLimit::new(measured.usage().peak_retained_bytes).unwrap(),
        ));
        let mut invalid = document.clone();
        invalid.roots[0] = SceneEntityId::new("missing-root").unwrap();
        let refused = factory(
            input(invalid, limits),
            source.clone(),
            &fixture.previous,
            &mut work,
        );
        assert!(refused.is_err(), "{kind:?}");
        let failed = work.usage();
        assert!(failed.work_bytes > 0);
        let (session, retained) = factory(
            input(document.clone(), limits),
            source,
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        assert_eq!(session.document().entities, document.entities);
        assert_eq!(retained.revision, session.revision());
        assert_eq!(retained.kind, kind);
        assert_eq!(
            work.usage().work_bytes,
            failed.work_bytes + measured.usage().work_bytes
        );
    });
}

#[test]
fn factory_reuses_the_nominal_basis_and_does_not_admit_equal_independent_authority() {
    let fixture = Fixture::new();
    let independent = admit_basis(previous_catalog(), None);
    assert_ne!(independent, fixture.previous);
    let document = document(false, 7);
    let limits = SceneFileLimits::default();
    each_unopened_source!(|factory, input, source, kind| {
        let mut work = AuthoredSchemaUpgradeWork::default();
        let (first, first_retained) = factory(
            input(document.clone(), limits),
            source.clone(),
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        let (second, second_retained) = factory(
            input(document.clone(), limits),
            source.clone(),
            &fixture.previous,
            &mut work,
        )
        .unwrap();
        assert_eq!(
            first.authoring_schema_basis(),
            second.authoring_schema_basis()
        );
        assert_ne!(first.revision().source_id(), second.revision().source_id());
        let before = work.usage();
        assert!(
            second
                .prepare_authored_upgrade_with_retained_source(
                    &key(kind),
                    second.revision(),
                    input(super::document(true, 7), limits),
                    &fixture.upgrade,
                    first_retained,
                    &mut work,
                )
                .is_err()
        );
        assert_eq!(work.usage(), before);
        let (foreign, foreign_retained) = factory(
            input(document.clone(), limits),
            source,
            &independent,
            &mut work,
        )
        .unwrap();
        assert_eq!(foreign.authoring_schema_basis(), Some(independent.clone()));
        let before = work.usage();
        assert!(
            foreign
                .prepare_authored_upgrade_with_retained_source(
                    &key(kind),
                    foreign.revision(),
                    input(super::document(true, 7), limits),
                    &fixture.upgrade,
                    foreign_retained,
                    &mut work,
                )
                .is_err()
        );
        assert_eq!(work.usage(), before);
        assert_eq!(second_retained.revision, second.revision());
    });
}
