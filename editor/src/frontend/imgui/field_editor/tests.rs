use super::*;
use nara::{
    reflect::ComponentFieldPath,
    scene::{SceneAuthoringSession, SceneDocument},
};

#[test]
fn actual_candidate_conversion_preserves_integer_precision_and_rejects_non_finite_numbers() {
    for (kind, text, expected) in [
        (
            ComponentValueKind::I64,
            "-9223372036854775808",
            ComponentValue::I64(i64::MIN),
        ),
        (
            ComponentValueKind::U64,
            "18446744073709551615",
            ComponentValue::U64(u64::MAX),
        ),
        (
            ComponentValueKind::String,
            "中文 / clipboard",
            ComponentValue::String("中文 / clipboard".into()),
        ),
    ] {
        assert_eq!(
            Candidate::Text(text.into())
                .value(&ComponentValueShape::scalar(kind))
                .unwrap(),
            expected
        );
    }
    for text in ["NaN", "inf", "-inf", "1e999"] {
        assert!(
            Candidate::Text(text.into())
                .value(&ComponentValueShape::scalar(ComponentValueKind::F64))
                .is_err()
        );
    }
    for (kind, text) in [
        (ComponentValueKind::U64, "-1"),
        (ComponentValueKind::I64, "9223372036854775808"),
    ] {
        assert!(
            Candidate::Text(text.into())
                .value(&ComponentValueShape::scalar(kind))
                .is_err()
        );
    }
}

#[test]
fn actual_candidate_conversion_admits_only_canonical_asset_references() {
    for (text, kind, value) in [
        ("textures/player.png", "path", "textures/player.png"),
        (
            "stable_id:550e8400-e29b-41d4-a716-446655440000",
            "stable_id",
            "550e8400-e29b-41d4-a716-446655440000",
        ),
    ] {
        assert_eq!(
            Candidate::Text(text.into())
                .value(&ComponentValueShape::AssetRef)
                .unwrap(),
            ComponentValue::map([
                ("kind", ComponentValue::String(kind.into())),
                ("value", ComponentValue::String(value.into()))
            ])
        );
    }
    assert!(
        Candidate::Text("stable_id:invalid".into())
            .value(&ComponentValueShape::AssetRef)
            .is_err()
    );
    assert!(!value_draft::supported(&ComponentValueShape::EntityRef));
}

fn test_basis(index: usize) -> FieldBasis {
    let session = SceneAuthoringSession::new(SceneDocument::default());
    let field = SceneInspectorFieldView {
        id: ComponentFieldId::new(format!("field{index}")),
        aliases: Vec::new(),
        path: ComponentFieldPath::from_fields([format!("field{index}")]),
        value_kind: ComponentValueKind::String,
        shape: ComponentValueShape::scalar(ComponentValueKind::String),
        required: true,
        capabilities: [ComponentCapability::Edit].into_iter().collect(),
        default_value: None,
        value: Some(ComponentValue::String("accepted".into())),
        state: SceneInspectorFieldState::Present,
    };
    FieldBasis {
        target: EditorFieldTarget::new(
            EditorDocumentId::from_raw(1),
            session.revision(),
            SceneEntityId::new("owner").unwrap(),
            ComponentTypeId::new("test.fields"),
            ComponentSchemaVersion::ONE,
            field.id.clone(),
        ),
        catalog: None,
        document_version: ComponentSchemaVersion::ONE,
        field,
    }
}

#[test]
fn clean_controls_do_not_consume_the_retained_budget_and_dirty_controls_never_evict() {
    let mut fields = FieldEditors::default();
    for index in 0..65 {
        let basis = test_basis(index);
        let key = FieldKey::from_target(&basis.target);
        fields.drafts.insert(key.clone(), FieldDraft::new(basis));
        fields.painted.insert(key);
    }
    assert!(fields.has_draft_capacity(&FieldKey::from_target(&test_basis(64).target)));
    for index in 0..64 {
        let key = FieldKey::from_target(&test_basis(index).target);
        let draft = fields.drafts.get_mut(&key).unwrap();
        draft.dirty = true;
        draft.candidate = Candidate::Text(format!("invalid retained {index}"));
    }
    assert!(!fields.has_draft_capacity(&FieldKey::from_target(&test_basis(64).target)));
    assert!(fields.has_draft_capacity(&FieldKey::from_target(&test_basis(63).target)));
    assert_eq!(
        fields.drafts.values().filter(|draft| draft.dirty).count(),
        64
    );
    assert_eq!(
        fields.drafts[&FieldKey::from_target(&test_basis(0).target)]
            .candidate
            .copy_text(),
        "invalid retained 0"
    );
}

#[test]
fn incompatible_source_and_shape_preserve_the_old_input_without_reinterpretation() {
    let basis = test_basis(0);
    let mut dirty = FieldDraft::new(basis.clone());
    dirty.dirty = true;
    dirty.candidate = Candidate::Text("old schema input 256 中文".into());
    let mut changed_shape = basis.clone();
    changed_shape.field.shape = ComponentValueShape::scalar(ComponentValueKind::U64);
    changed_shape.field.value_kind = ComponentValueKind::U64;
    dirty.synchronize(changed_shape);
    assert!(dirty.incompatible);
    assert_eq!(dirty.candidate.copy_text(), "old schema input 256 中文");
    assert_eq!(
        dirty.basis, basis,
        "incompatible input keeps its original interpretation basis"
    );

    let mut source_draft = FieldDraft::new(basis.clone());
    source_draft.dirty = true;
    source_draft.candidate = Candidate::Text("unapplied source text".into());
    let mut replacement_source = basis.clone();
    let replacement_session = SceneAuthoringSession::new(SceneDocument::default());
    replacement_source.target = EditorFieldTarget::new(
        basis.target.document(),
        replacement_session.revision(),
        basis.target.entity().clone(),
        basis.target.component().clone(),
        basis.target.component_version(),
        basis.target.field().clone(),
    );
    source_draft.synchronize(replacement_source);
    assert!(
        source_draft.incompatible,
        "a same-path replacement session is a new source authority"
    );
    assert_eq!(source_draft.candidate.copy_text(), "unapplied source text");
    assert_eq!(source_draft.basis, basis);
}
