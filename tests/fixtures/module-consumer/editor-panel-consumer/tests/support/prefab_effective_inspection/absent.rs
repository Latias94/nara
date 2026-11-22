use super::{
    set::{
        CHANGED_NOTE, CHANGED_RANK, DEFAULT_BONUS,
        EffectiveValue::{self, Defaulted, Missing, Present},
        Fixture, base_source_with_optional, fixture_source, instance_patch, with_claim_intents,
    },
    *,
};
use nara::{reflect::ComponentPropertyStep, scene::AuthoringPrefabPropertyClaimKind as ClaimKind};

const BASE_FILLED_OPTIONAL: u64 = 72;
const SUPPRESSED_BONUS: (EffectiveValue, Option<ClaimKind>) = (
    Defaulted(ComponentValue::U64(DEFAULT_BONUS)),
    Some(ClaimKind::Absent),
);
const SUPPRESSED_OPTIONAL: (EffectiveValue, Option<ClaimKind>) = (Missing, Some(ClaimKind::Absent));
const PINNED_NULL: (EffectiveValue, Option<ClaimKind>) =
    (Present(ComponentValue::Null), Some(ClaimKind::Set));

#[test]
fn a_registered_panel_authors_and_reverts_effective_absence_on_a_nonactive_scene() {
    absent_round_trip(EditorDocumentKind::Scene, SCENE);
}

#[test]
fn a_registered_panel_authors_and_reverts_effective_absence_on_a_nonactive_prefab() {
    absent_round_trip(EditorDocumentKind::Prefab, PREFAB);
}

fn absent_round_trip(kind: EditorDocumentKind, source: &'static str) {
    let mut fixture = Fixture::new(kind, source);
    let left = fixture.left.clone();
    let source_file = fixture.project.root.join(source);
    let base_file = fixture.project.root.join(BASE);
    let untouched = [STARTUP, UNRELATED].map(|path| {
        let path = fixture.project.root.join(path);
        let bytes = fs::read(&path).unwrap();
        (path, bytes)
    });
    let initial_basis = basis(&fixture.app, fixture.document);
    let (inspection, [_, optional]) = inspect_fields(
        &mut fixture,
        "left",
        (Present(ComponentValue::U64(INITIAL_SCORE)), None),
        (Missing, None),
    );
    let limits = PrefabDeclarationLimits::default();
    let mut nested = address("left", "profile");
    nested
        .property
        .steps
        .push(ComponentPropertyStep::Member(ComponentFieldId::new(
            "level",
        )));
    for invalid in [nested, address("left", "score"), address("left", "rank")] {
        assert_eq!(
            fixture.prepare_write(inspection, invalid, PrefabPropertyIntent::Absent, limits),
            Outcome::Rejected {
                inspection: Some(inspection),
                reason: Rejection::InvalidProperty,
            }
        );
        expect_phase(&mut fixture.app, inspection, Phase::Ready);
    }
    let tiny =
        PrefabDeclarationLimits::new(1, limits.max_logical_bytes(), limits.max_depth()).unwrap();
    assert_eq!(
        fixture.prepare_write(
            inspection,
            optional.clone(),
            PrefabPropertyIntent::Absent,
            tiny,
        ),
        Outcome::Rejected {
            inspection: Some(inspection),
            reason: Rejection::BudgetExceeded,
        }
    );
    expect_phase(&mut fixture.app, inspection, Phase::Ready);
    assert_eq!(basis(&fixture.app, fixture.document), initial_basis);

    let optional_absent = with_claim_intents(&left, [("optional", PrefabPropertyIntent::Absent)]);
    fixture.review_write(
        inspection,
        optional,
        PrefabPropertyIntent::Absent,
        &left,
        &optional_absent,
    );
    fixture.assert_live(&left, false, 0, 0);
    fixture.assert_unwritten();
    fixture.commit_write(inspection, ClaimKind::Absent);
    fixture.assert_live(&optional_absent, true, 1, 0);

    let set_null = PrefabPropertyIntent::Set(ComponentValue::Null);
    let pinned_null = with_claim_intents(
        &left,
        [
            ("optional", PrefabPropertyIntent::Absent),
            ("bonus", set_null.clone()),
        ],
    );
    let (pinning, [bonus, _]) = inspect_fields(
        &mut fixture,
        "left",
        (Present(ComponentValue::U64(INITIAL_SCORE)), None),
        SUPPRESSED_OPTIONAL,
    );
    fixture.review_write(pinning, bonus, set_null, &optional_absent, &pinned_null);
    fixture.commit_write(pinning, ClaimKind::Set);
    fixture.assert_live(&pinned_null, true, 2, 0);

    let both_absent = with_claim_intents(
        &left,
        [
            ("optional", PrefabPropertyIntent::Absent),
            ("bonus", PrefabPropertyIntent::Absent),
        ],
    );
    let before_absent = basis(&fixture.app, fixture.document);
    let (cancelled, [bonus, _]) =
        inspect_fields(&mut fixture, "left", PINNED_NULL, SUPPRESSED_OPTIONAL);
    fixture.review_write(
        cancelled,
        bonus,
        PrefabPropertyIntent::Absent,
        &pinned_null,
        &both_absent,
    );
    fixture.assert_live(&pinned_null, true, 2, 0);
    cancel(&mut fixture.panel, &mut fixture.app, cancelled);
    assert_eq!(basis(&fixture.app, fixture.document), before_absent);
    fixture.assert_live(&pinned_null, true, 2, 0);
    fixture.assert_unwritten();

    let (writing, [bonus, _]) =
        inspect_fields(&mut fixture, "left", PINNED_NULL, SUPPRESSED_OPTIONAL);
    fixture.review_write(
        writing,
        bonus,
        PrefabPropertyIntent::Absent,
        &pinned_null,
        &both_absent,
    );
    fixture.commit_write(writing, ClaimKind::Absent);
    fixture.assert_live(&both_absent, true, 3, 0);

    let before_repeat = basis(&fixture.app, fixture.document);
    let (repeating, [bonus, _]) =
        inspect_fields(&mut fixture, "left", SUPPRESSED_BONUS, SUPPRESSED_OPTIONAL);
    assert_eq!(
        fixture.prepare_write(repeating, bonus, PrefabPropertyIntent::Absent, limits),
        Outcome::Accepted {
            inspection: repeating,
            phase: Phase::PreparingEdit,
        }
    );
    assert_failure(
        &mut fixture.app,
        repeating,
        Rejection::AlreadyWritten,
        "scene.authoring-prefab-already-written",
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, repeating);
    assert_eq!(basis(&fixture.app, fixture.document), before_repeat);
    fixture.assert_live(&both_absent, true, 3, 0);

    replay(&mut fixture.app, fixture.document, false);
    fixture.assert_live(&pinned_null, true, 2, 1);
    let (undone, _) = inspect_fields(&mut fixture, "left", PINNED_NULL, SUPPRESSED_OPTIONAL);
    cancel(&mut fixture.panel, &mut fixture.app, undone);
    replay(&mut fixture.app, fixture.document, true);
    fixture.assert_live(&both_absent, true, 3, 0);
    let (redone, _) = inspect_fields(&mut fixture, "left", SUPPRESSED_BONUS, SUPPRESSED_OPTIONAL);
    cancel(&mut fixture.panel, &mut fixture.app, redone);
    fixture.assert_unwritten();

    save(&mut fixture.app, fixture.document);
    let saved = fs::read(&source_file).unwrap();
    assert_saved_source(
        kind,
        &saved,
        &fixture_source(kind, &both_absent, &fixture.right),
    );
    fixture.assert_live(&both_absent, false, 3, 0);
    replay(&mut fixture.app, fixture.document, false);
    fixture.assert_live(&pinned_null, true, 2, 1);
    fixture.reopen();
    fixture.assert_live(&both_absent, false, 0, 0);
    assert_eq!(fs::read(&source_file).unwrap(), saved);

    let changed_base = base_source_with_optional(
        CHANGED_SCORE,
        CHANGED_NOTE,
        CHANGED_RANK,
        Some(BASE_FILLED_OPTIONAL),
    );
    fs::write(&base_file, &changed_base).unwrap();
    let (shared, _) = inspect_fields(
        &mut fixture,
        "right",
        (Present(ComponentValue::U64(CHANGED_SCORE)), None),
        (Present(ComponentValue::U64(BASE_FILLED_OPTIONAL)), None),
    );
    cancel(&mut fixture.panel, &mut fixture.app, shared);
    let (reverting, [bonus, _]) =
        inspect_fields(&mut fixture, "left", SUPPRESSED_BONUS, SUPPRESSED_OPTIONAL);
    prepare_revert(
        &mut fixture.panel,
        &mut fixture.app,
        reverting,
        bonus,
        limits,
    );
    expect_phase(&mut fixture.app, reverting, Phase::Review);
    fixture.panel.observe(&mut fixture.app, |context| {
        let edit = context.prefab_inspection_review(reverting).unwrap();
        assert_eq!(
            edit.patch(),
            &instance_patch(&both_absent, &optional_absent)
        );
        assert_eq!(
            edit.inverse(),
            &instance_patch(&optional_absent, &both_absent)
        );
    });
    let before_revert = basis(&fixture.app, fixture.document);
    fixture.assert_live(&both_absent, false, 0, 0);
    fixture.commit(reverting);
    let revision = basis(&fixture.app, fixture.document).revision;
    assert_ne!(revision, before_revert.revision);
    assert_eq!(
        frame(&fixture.app).prefab_inspection().result,
        Some(EditorPrefabInspectionResult::Reverted {
            inspection: reverting,
            document: fixture.document,
            revision,
        })
    );
    acknowledge(&mut fixture.panel, &mut fixture.app, reverting);
    let (reverted, _) = inspect_fields(
        &mut fixture,
        "left",
        (Present(ComponentValue::U64(CHANGED_SCORE)), None),
        SUPPRESSED_OPTIONAL,
    );
    cancel(&mut fixture.panel, &mut fixture.app, reverted);
    fixture.assert_live(&optional_absent, true, 1, 0);
    assert_eq!(fs::read(&source_file).unwrap(), saved);
    assert_eq!(fs::read(&base_file).unwrap(), changed_base.as_bytes());
    for (path, bytes) in untouched {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
    }
    assert!(fixture.app.close());
}

fn inspect_fields(
    fixture: &mut Fixture,
    anchor: &str,
    bonus: (EffectiveValue, Option<ClaimKind>),
    optional: (EffectiveValue, Option<ClaimKind>),
) -> (EditorPrefabInspectionId, [EditorPrefabPropertyAddress; 2]) {
    let (
        inspection,
        [
            (bonus_address, bonus_value),
            (optional_address, optional_value),
        ],
    ) = fixture.inspect_properties(anchor, [("bonus", bonus.1), ("optional", optional.1)]);
    assert_eq!(bonus_value, bonus.0, "{anchor}.bonus");
    assert_eq!(optional_value, optional.0, "{anchor}.optional");
    (inspection, [bonus_address, optional_address])
}
