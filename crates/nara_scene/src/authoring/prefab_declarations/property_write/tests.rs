use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyStep, ComponentSchemaVersion, ComponentTypeId,
};

use super::*;
use crate::{
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, SceneEntityId,
    SourceDocumentKey,
    prefab::{PrefabOverlayCostLimits, prefab_instance_cost},
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn base(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "5e7c0f41-a1f7-4ce4-bb69-e4927f4f661b".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn observed() -> ObservedPrefabPropertyWrite {
    ObservedPrefabPropertyWrite {
        anchor: id("anchor"),
        base: base("canonical"),
        target: PrefabEntityTarget::source(id("target")),
        observed_target: None,
        component: ComponentTypeId::new("test.Component"),
        version: ComponentSchemaVersion::ONE,
        existing_intent: None,
        property: ComponentPropertyPath::new(ComponentFieldId::new("data")),
        intent: PrefabPropertyIntent::Set(ComponentValue::String("new value".into())),
        basis: AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap(),
    }
}

fn source(
    edit: &ObservedPrefabPropertyWrite,
    overlay: Option<PrefabOverlay>,
) -> [SceneEntityRecord; 1] {
    let mut entity = SceneEntityRecord::new(edit.anchor.clone());
    let mut instance = PrefabInstance::forest(AssetRef::path("prefabs/alias.prefab.json").unwrap());
    instance.overrides = overlay;
    entity.prefab = Some(instance);
    [entity]
}

fn candidate(patch: &ScenePatchDocument) -> &PrefabInstance {
    let [
        ScenePatchOperation::SetPrefabInstance {
            value: Some(instance),
            ..
        },
    ] = patch.operations.as_slice()
    else {
        panic!("expected one instance edit");
    };
    instance
}

fn limits_for_growth(
    entity: &SceneEntityRecord,
    updated: &PrefabInstance,
) -> PrefabDeclarationLimits {
    let original = prefab_instance_cost(
        entity.prefab.as_ref().unwrap(),
        PrefabOverlayCostLimits::UNBOUNDED,
    )
    .unwrap();
    let updated = prefab_instance_cost(updated, PrefabOverlayCostLimits::UNBOUNDED).unwrap();
    PrefabDeclarationLimits::new(
        original.nodes + updated.nodes + 1,
        original.logical_bytes() + updated.logical_bytes() + entity.id.as_str().len(),
        original.depth.max(updated.depth),
    )
    .unwrap()
}

#[test]
fn first_pin_admits_both_roots_and_the_new_overlay_at_exact_limits() {
    let mut edit = observed();
    edit.target.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
        expected_base: base("nested-base"),
    });
    edit.property = edit.property.with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("member")),
        ComponentPropertyStep::ListItem {
            by: ComponentFieldId::new("id"),
            key: ComponentPropertyKey::I64(10),
        },
        ComponentPropertyStep::MapEntry("key".into()),
    ]);
    let source = source(&edit, None);
    let original = source[0].prefab.as_ref().unwrap();
    let patch = lower_write(&source, &edit, PrefabDeclarationLimits::default()).unwrap();
    let updated = candidate(&patch);
    assert_eq!(updated.source, original.source);
    assert_eq!(updated.overrides.as_ref().unwrap().base, edit.base);
    assert!(matches!(
        &patch.operations[0],
        ScenePatchOperation::SetPrefabInstance { entity, expected, .. }
            if entity == &edit.anchor && expected == &source[0].prefab
    ));
    let exact = limits_for_growth(&source[0], updated);
    assert_eq!(lower_write(&source, &edit, exact), Ok(patch));
    let nodes = exact.max_nodes();
    let bytes = exact.max_logical_bytes();
    let depth = exact.max_depth();
    for (nodes, bytes, depth) in [
        (nodes - 1, bytes, depth),
        (nodes, bytes - 1, depth),
        (nodes, bytes, depth - 1),
    ] {
        let below = PrefabDeclarationLimits::new(nodes, bytes, depth).unwrap();
        assert_eq!(
            lower_write(&source, &edit, below),
            Err(PrefabDeclarationError::BudgetExceeded),
        );
    }
}

#[test]
fn observed_alias_group_uses_its_original_target_and_exact_claim_slot() {
    let mut edit = observed();
    edit.existing_intent = Some(1);
    let alias = PrefabEntityTarget::source(id("alias"));
    let sibling_intent = PrefabComponentIntent::Suppress {
        target: edit.target.clone(),
        component: ComponentTypeId::new("test.Other"),
    };
    let sibling_claim = PrefabPropertyClaim {
        path: ComponentPropertyPath::new(ComponentFieldId::new("other")),
        intent: PrefabPropertyIntent::Set(ComponentValue::Bool(true)),
    };
    let claim = |intent| PrefabPropertyClaim {
        path: edit.property.clone(),
        intent,
    };
    for (original, expected) in [
        (vec![], vec![claim(edit.intent.clone())]),
        (
            vec![claim(PrefabPropertyIntent::Absent), sibling_claim.clone()],
            vec![claim(edit.intent.clone()), sibling_claim.clone()],
        ),
    ] {
        let mut overlay = PrefabOverlay::new(edit.base.clone());
        overlay.components.push(sibling_intent.clone());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: alias.clone(),
            component: edit.component.clone(),
            version: edit.version,
            claims: original,
        });
        let source = source(&edit, Some(overlay));
        let patch = lower_write(&source, &edit, PrefabDeclarationLimits::default()).unwrap();
        let mut expected_overlay = source[0]
            .prefab
            .as_ref()
            .unwrap()
            .overrides
            .clone()
            .unwrap();
        let PrefabComponentIntent::Properties { claims, .. } = &mut expected_overlay.components[1]
        else {
            panic!("expected an alias property group");
        };
        *claims = expected;
        let updated = candidate(&patch);
        assert_eq!(updated.overrides.as_ref(), Some(&expected_overlay));
        let exact = limits_for_growth(&source[0], updated);
        assert_eq!(lower_write(&source, &edit, exact), Ok(patch));
    }
}

#[test]
fn contradictory_group_and_strict_overlaps_are_refused() {
    let mut edit = observed();
    let parent = edit.property.clone();
    let child = parent
        .clone()
        .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("leaf"))]);
    let mut overlay = PrefabOverlay::new(edit.base.clone());
    for (existing, requested) in [(&parent, &child), (&child, &parent)] {
        edit.property = requested.clone();
        overlay.components = vec![PrefabComponentIntent::Properties {
            target: edit.target.clone(),
            component: edit.component.clone(),
            version: edit.version,
            claims: vec![PrefabPropertyClaim {
                path: existing.clone(),
                intent: PrefabPropertyIntent::Absent,
            }],
        }];
        let source = source(&edit, Some(overlay.clone()));
        let limits = PrefabDeclarationLimits::default();

        edit.existing_intent = None;
        assert_eq!(
            lower_write(&source, &edit, limits),
            Err(PrefabDeclarationError::StaleObservedIntent),
        );
        edit.existing_intent = Some(0);
        assert_eq!(
            lower_write(&source, &edit, limits),
            Err(PrefabDeclarationError::ConflictingClaim),
        );
    }
}

#[test]
fn first_absent_fits_exact_declaration_limits_and_a_repeat_is_already_written() {
    let mut edit = observed();
    edit.intent = PrefabPropertyIntent::Absent;
    let original = source(&edit, None);
    let patch = lower_write(&original, &edit, PrefabDeclarationLimits::default()).unwrap();
    let updated = candidate(&patch);
    let exact = limits_for_growth(&original[0], updated);
    let overlay = updated.overrides.clone().unwrap();
    assert_eq!(
        overlay.components,
        [PrefabComponentIntent::Properties {
            target: edit.target.clone(),
            component: edit.component.clone(),
            version: edit.version,
            claims: vec![PrefabPropertyClaim {
                path: edit.property.clone(),
                intent: PrefabPropertyIntent::Absent,
            }],
        }]
    );
    assert_eq!(lower_write(&original, &edit, exact), Ok(patch));
    let below = PrefabDeclarationLimits::new(
        exact.max_nodes() - 1,
        exact.max_logical_bytes(),
        exact.max_depth(),
    )
    .unwrap();
    assert_eq!(
        lower_write(&original, &edit, below),
        Err(PrefabDeclarationError::BudgetExceeded),
    );

    edit.existing_intent = Some(0);
    let pinned = source(&edit, Some(overlay));
    assert_eq!(
        lower_write(&pinned, &edit, PrefabDeclarationLimits::default()),
        Err(PrefabDeclarationError::AlreadyWritten),
    );
}

#[test]
fn bit_identical_existing_set_is_a_noop_and_signed_zero_changes_still_pin() {
    let mut edit = observed();
    edit.intent = PrefabPropertyIntent::Set(ComponentValue::f64(-0.0).unwrap());
    let limits = PrefabDeclarationLimits::default();
    let first = lower_write(&source(&edit, None), &edit, limits).unwrap();
    let pinned = source(&edit, candidate(&first).overrides.clone());
    edit.existing_intent = Some(0);
    assert_eq!(
        lower_write(&pinned, &edit, limits),
        Err(PrefabDeclarationError::AlreadyWritten),
    );

    edit.intent = PrefabPropertyIntent::Set(ComponentValue::f64(0.0).unwrap());
    let patch = lower_write(&pinned, &edit, limits).unwrap();
    let overlay = candidate(&patch).overrides.as_ref().unwrap();
    let [PrefabComponentIntent::Properties { claims, .. }] = overlay.components.as_slice() else {
        panic!("expected one property group");
    };
    let [
        PrefabPropertyClaim {
            intent: PrefabPropertyIntent::Set(ComponentValue::F64(value)),
            ..
        },
    ] = claims.as_slice()
    else {
        panic!("expected one float Set claim");
    };
    assert_eq!(value.get().to_bits(), 0.0_f64.to_bits());
}
