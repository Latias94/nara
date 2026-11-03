//! Owner tests for authored relocation proposals, without route admission claims.

use std::{borrow::Cow, convert::Infallible, io};

use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, ReferenceInputId,
};
use nara_reflect::AuthoringSchemaBundle;

use super::{AddressRole, BranchRelocation, RelocationSetupError};
use crate::{
    AuthoringSchemaBasis, PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstanceBinding, PrefabInstanceHop, ResolvedPrefabSource, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
    prefab::{
        SourceDocumentInput,
        extraction::{SourceCut, try_capture_source_cut_with_loader},
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "b55b96a6-6c0a-4f57-993e-e16f458cfd15".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    source_key(value, SourceDocumentKind::Prefab)
}

fn hop(anchor: &str, base: &str) -> PrefabInstanceHop {
    PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
        expected_base: key(base),
    }
}

fn source() -> PrefabDocument {
    PrefabDocument {
        inputs: Default::default(),
        placements: vec![
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("b")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("k"),
                ))),
            ),
            SceneEntityPlacement::root(id("x")),
        ],
        // Raw source order deliberately differs from the hierarchy's order.
        entities: vec![
            SceneEntityRecord::new(id("b")),
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("k")),
        ],
    }
}

fn capture_cut(source: SourceDocumentInput<'_>) -> SourceCut<'_> {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let report = try_capture_source_cut_with_loader(
        source,
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("a direct-only cut must not load a Prefab: {reference:?}")
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    report
        .cut
        .expect("the direct k branch must have a source-owned cut")
}

fn charge(remaining: &mut usize, bytes: Option<usize>) -> Result<(), ()> {
    *remaining = bytes
        .and_then(|bytes| remaining.checked_sub(bytes))
        .ok_or(())?;
    Ok(())
}

#[test]
fn retained_root_entity_and_instance_slot_have_distinct_continuity() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();

    let SourceDocumentInput::Prefab(original) = cut.source() else {
        panic!("the cut must retain the actual Prefab input");
    };
    assert!(std::ptr::eq(original, &source));
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    let selected = cut.entities().collect::<Vec<_>>();
    assert_eq!(selected.len(), 2);
    assert!(std::ptr::eq(selected[0], &source.entities[0]));
    assert!(std::ptr::eq(selected[1], &source.entities[2]));

    for (target, role, moves) in [
        (
            PrefabEntityTarget::source(id("k")),
            AddressRole::Entity,
            false,
        ),
        (
            PrefabEntityTarget::source(id("k")),
            AddressRole::InstanceSlot,
            true,
        ),
        (
            PrefabEntityTarget::source(id("b")),
            AddressRole::Entity,
            true,
        ),
        (
            PrefabEntityTarget::source(id("b")),
            AddressRole::InstanceSlot,
            true,
        ),
        (
            PrefabEntityTarget::source(id("x")),
            AddressRole::Entity,
            false,
        ),
        (
            PrefabEntityTarget::source(id("x")),
            AddressRole::InstanceSlot,
            false,
        ),
        (
            PrefabEntityTarget::addition(id("b")),
            AddressRole::Entity,
            false,
        ),
        (
            PrefabEntityTarget::addition(id("b")),
            AddressRole::InstanceSlot,
            false,
        ),
    ] {
        let target_before = target.clone();
        let proposed = relocation
            .retained_target(&p, &target, role, |_| Ok::<_, Infallible>(()))
            .unwrap();
        if moves {
            assert!(matches!(&proposed, Cow::Owned(_)));
            assert_eq!(
                proposed.as_ref(),
                &PrefabEntityTarget {
                    through: vec![hop("k", "q")],
                    entity: target.entity.clone(),
                },
            );
        } else {
            assert!(matches!(proposed, Cow::Borrowed(value) if std::ptr::eq(value, &target)));
        }
        assert_eq!(target, target_before);
    }
    assert_eq!(source, before);
}

#[test]
fn outer_route_inserts_before_the_moved_slot_and_preserves_its_tail() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let s = source_key("s", SourceDocumentKind::Scene);
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let tail = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::InstanceAddition(id("b")),
        expected_base: key("t"),
    };
    let target = PrefabEntityTarget {
        through: vec![hop("host", "p"), hop("k", "r"), tail.clone()],
        entity: PrefabEntityOrigin::SourceEntity(id("b")),
    };
    let target_before = target.clone();
    let proposed = relocation
        .retained_target(
            &s,
            &target,
            AddressRole::Entity,
            |_| Ok::<_, Infallible>(()),
        )
        .unwrap();
    assert!(matches!(&proposed, Cow::Owned(_)));
    assert_eq!(
        proposed.as_ref(),
        &PrefabEntityTarget {
            through: vec![hop("host", "p"), hop("k", "q"), hop("k", "r"), tail],
            entity: target.entity.clone(),
        },
    );

    let root = PrefabEntityTarget {
        through: vec![hop("host", "p")],
        entity: PrefabEntityOrigin::SourceEntity(id("k")),
    };
    let unchanged = relocation
        .retained_target(&s, &root, AddressRole::Entity, |_| Ok::<_, Infallible>(()))
        .unwrap();
    assert!(matches!(unchanged, Cow::Borrowed(value) if std::ptr::eq(value, &root)));
    assert_eq!(target, target_before);
    assert_eq!(source, before);
}

#[test]
fn repeated_p_claims_each_insert_a_hop_without_global_key_substitution() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let s = source_key("s", SourceDocumentKind::Scene);
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let target = PrefabEntityTarget {
        through: vec![
            hop("host", "p"),
            hop("k", "r"),
            hop("k", "p"),
            hop("k", "r"),
            hop("return", "p"),
        ],
        entity: PrefabEntityOrigin::SourceEntity(id("b")),
    };
    let target_before = target.clone();
    let proposed = relocation
        .retained_target(
            &s,
            &target,
            AddressRole::Entity,
            |_| Ok::<_, Infallible>(()),
        )
        .unwrap();
    assert_eq!(
        proposed.as_ref(),
        &PrefabEntityTarget {
            through: vec![
                hop("host", "p"),
                hop("k", "q"),
                hop("k", "r"),
                // This k slot is declared in R, even though it claims P as its base.
                hop("k", "p"),
                hop("k", "q"),
                hop("k", "r"),
                hop("return", "p"),
                hop("k", "q"),
            ],
            entity: target.entity.clone(),
        },
    );
    assert_eq!(target, target_before);
    assert_eq!(source, before);
}

#[test]
fn setup_requires_the_actual_source_kind_a_prefab_destination_and_a_fresh_identity() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let scene_p = source_key("p", SourceDocumentKind::Scene);
    let scene_q = source_key("q", SourceDocumentKind::Scene);
    assert_eq!(
        BranchRelocation::new(&cut, &scene_p, &q).err(),
        Some(RelocationSetupError::SourceKind),
    );
    assert_eq!(
        BranchRelocation::new(&cut, &p, &scene_q).err(),
        Some(RelocationSetupError::DestinationKind),
    );
    // Independently constructed keys still collide by canonical value.
    let same_p = key("p");
    assert!(!std::ptr::eq(&p, &same_p));
    assert_eq!(
        BranchRelocation::new(&cut, &p, &same_p).err(),
        Some(RelocationSetupError::IdentityCollision),
    );
    assert!(BranchRelocation::new(&cut, &p, &q).is_ok());

    let scene = SceneDocument {
        placements: source.placements.clone(),
        entities: source.entities.clone(),
    };
    let scene_before = scene.clone();
    let scene_cut = capture_cut(SourceDocumentInput::Scene(&scene));
    assert_eq!(
        BranchRelocation::new(&scene_cut, &p, &q).err(),
        Some(RelocationSetupError::SourceKind),
    );
    assert!(BranchRelocation::new(&scene_cut, &scene_p, &q).is_ok());
    assert_eq!(scene, scene_before);
    assert_eq!(source, before);
}

#[test]
fn finite_cumulative_budget_is_inclusive_and_refusal_preserves_all_inputs() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let s = source_key("s", SourceDocumentKind::Scene);
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let target = PrefabEntityTarget {
        through: vec![hop("host", "p"), hop("k", "r"), hop("return", "p")],
        entity: PrefabEntityOrigin::SourceEntity(id("b")),
    };
    let target_before = target.clone();
    let original_hops = target.through.as_ptr();
    let original_entities = source.entities.as_ptr();
    const ALLOWANCE: usize = 64 * 1024;

    // Reject unknown byte counts without consuming the finite callback's allowance.
    let mut remaining = ALLOWANCE;
    assert_eq!(charge(&mut remaining, None), Err(()));
    assert_eq!(remaining, ALLOWANCE);
    let observed = relocation
        .retained_target(&s, &target, AddressRole::Entity, |bytes| {
            charge(&mut remaining, bytes)
        })
        .unwrap();
    assert!(matches!(&observed, Cow::Owned(_)));
    let used = ALLOWANCE - remaining;
    assert!(used > 1);

    for (allowance, admitted) in [(used, true), (used - 1, false)] {
        let mut remaining = allowance;
        let result = relocation.retained_target(&s, &target, AddressRole::Entity, |bytes| {
            charge(&mut remaining, bytes)
        });
        if admitted {
            let proposed = result.unwrap();
            assert_eq!(proposed.as_ref(), observed.as_ref());
            assert_eq!(remaining, 0);
        } else {
            assert_eq!(result, Err(()));
        }
        assert_eq!(target, target_before);
        assert_eq!(source, before);
        assert_eq!(target.through.as_ptr(), original_hops);
        assert_eq!(source.entities.as_ptr(), original_entities);
        assert!(std::ptr::eq(cut.root(), &source.entities[2]));
        let SourceDocumentInput::Prefab(original) = cut.source() else {
            panic!("budget refusal must preserve the original Prefab borrow");
        };
        assert!(std::ptr::eq(original, &source));
    }
}

#[test]
fn admission_callback_errors_escape_with_their_exact_identity() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let target = PrefabEntityTarget::source(id("b"));
    let target_before = target.clone();

    for accepted_charges in [0, 1] {
        let error = Box::new(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "relocation denied",
        ));
        let original_error = std::ptr::from_ref(error.as_ref());
        let mut error = Some(error);
        let mut calls = 0;
        let result = relocation.retained_target(&p, &target, AddressRole::Entity, |bytes| {
            assert!(bytes.is_some());
            calls += 1;
            if calls > accepted_charges {
                Err(error
                    .take()
                    .expect("the callback must stop after its first error"))
            } else {
                Ok(())
            }
        });
        let Err(returned) = result else {
            panic!("callback refusal must not return a partial proposal");
        };
        assert_eq!(calls, accepted_charges + 1);
        assert!(std::ptr::eq(returned.as_ref(), original_error));
        assert_eq!(returned.kind(), io::ErrorKind::OutOfMemory);
        assert_eq!(target, target_before);
        assert_eq!(source, before);
        assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    }
}

#[test]
fn retained_references_preserve_identity_and_rewrite_entity_and_input_routes() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let r = key("r");
    let s = source_key("s", SourceDocumentKind::Scene);
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let local = EntityReference::SceneLocal { entity: id("b") };
    let local_before = local.clone();
    let proposed = relocation
        .retained_reference(&p, &local, |_| Ok::<_, Infallible>(()))
        .unwrap();
    assert!(matches!(&proposed, Cow::Owned(_)));
    assert_eq!(
        proposed.as_ref(),
        &EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![hop("k", "q")],
                target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
            },
        },
    );
    assert_eq!(local, local_before);

    let input = ReferenceInputId::new("external").unwrap();
    for (scope, reference) in [
        (&p, EntityReference::SceneLocal { entity: id("k") }),
        (&r, EntityReference::SceneLocal { entity: id("b") }),
        (
            &p,
            EntityReference::Persistent {
                entity: PersistentRuntimeReference::new(
                    PersistentRuntimeNamespaceId::new("relocation").unwrap(),
                    PersistentRuntimeId::parse_str("9144561b-82a0-463d-b15c-eb17e2329d1a").unwrap(),
                ),
            },
        ),
        (
            &p,
            EntityReference::Authored {
                reference: AuthoredEntityReference::input(input.clone()),
            },
        ),
    ] {
        let reference_before = reference.clone();
        let unchanged = relocation
            .retained_reference(scope, &reference, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(unchanged, Cow::Borrowed(value) if std::ptr::eq(value, &reference)));
        assert_eq!(reference, reference_before);
    }

    let tail = PrefabInstanceHop {
        anchor: PrefabEntityOrigin::InstanceAddition(id("b")),
        expected_base: key("t"),
    };
    for endpoint in [
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
        AuthoredReferenceTarget::Input(input),
    ] {
        let reference = EntityReference::Authored {
            reference: AuthoredEntityReference {
                through: vec![hop("host", "p"), hop("k", "r"), tail.clone()],
                target: endpoint.clone(),
            },
        };
        let reference_before = reference.clone();
        let proposed = relocation
            .retained_reference(&s, &reference, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(&proposed, Cow::Owned(_)));
        assert_eq!(
            proposed.as_ref(),
            &EntityReference::Authored {
                reference: AuthoredEntityReference {
                    through: vec![hop("host", "p"), hop("k", "q"), hop("k", "r"), tail.clone(),],
                    target: endpoint,
                },
            },
        );
        assert_eq!(reference, reference_before);
    }
    assert_eq!(source, before);
}

#[test]
fn retained_binding_keeps_its_base_and_relocates_only_affected_root_routes() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();

    for (binding, expected) in [
        (
            PrefabInstanceBinding::Root {
                base: p.clone(),
                root: PrefabEntityTarget::source(id("b")),
            },
            PrefabInstanceBinding::Root {
                base: p.clone(),
                root: PrefabEntityTarget {
                    through: vec![hop("k", "q")],
                    entity: PrefabEntityOrigin::SourceEntity(id("b")),
                },
            },
        ),
        (
            PrefabInstanceBinding::Root {
                base: key("r"),
                root: PrefabEntityTarget {
                    through: vec![hop("host", "p"), hop("k", "r")],
                    entity: PrefabEntityOrigin::SourceEntity(id("b")),
                },
            },
            PrefabInstanceBinding::Root {
                base: key("r"),
                root: PrefabEntityTarget {
                    through: vec![hop("host", "p"), hop("k", "q"), hop("k", "r")],
                    entity: PrefabEntityOrigin::SourceEntity(id("b")),
                },
            },
        ),
    ] {
        let binding_before = binding.clone();
        let proposed = relocation
            .retained_binding(&binding, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(&proposed, Cow::Owned(_)));
        assert_eq!(proposed.as_ref(), &expected);
        assert_eq!(binding, binding_before);
    }

    for binding in [
        PrefabInstanceBinding::Root {
            base: p.clone(),
            root: PrefabEntityTarget::source(id("k")),
        },
        PrefabInstanceBinding::Root {
            base: key("r"),
            root: PrefabEntityTarget::source(id("b")),
        },
        PrefabInstanceBinding::Forest,
    ] {
        let binding_before = binding.clone();
        let unchanged = relocation
            .retained_binding(&binding, |_| Ok::<_, Infallible>(()))
            .unwrap();
        assert!(matches!(unchanged, Cow::Borrowed(value) if std::ptr::eq(value, &binding)));
        assert_eq!(binding, binding_before);
    }
    assert_eq!(source, before);
}

#[test]
fn reference_and_binding_budgets_are_inclusive_and_refuse_without_mutating_inputs() {
    let source = source();
    let before = source.clone();
    let cut = capture_cut(SourceDocumentInput::Prefab(&source));
    let p = key("p");
    let q = key("q");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let reference = EntityReference::SceneLocal { entity: id("b") };
    let reference_before = reference.clone();
    let binding = PrefabInstanceBinding::Root {
        base: p.clone(),
        root: PrefabEntityTarget::source(id("b")),
    };
    let binding_before = binding.clone();
    let original_entities = source.entities.as_ptr();
    const ALLOWANCE: usize = 64 * 1024;

    let mut remaining = ALLOWANCE;
    let observed_reference = relocation
        .retained_reference(&p, &reference, |bytes| charge(&mut remaining, bytes))
        .unwrap();
    assert!(matches!(&observed_reference, Cow::Owned(_)));
    let used = ALLOWANCE - remaining;
    assert!(used > 1);
    for (allowance, admitted) in [(used, true), (used - 1, false)] {
        let mut remaining = allowance;
        let result =
            relocation.retained_reference(&p, &reference, |bytes| charge(&mut remaining, bytes));
        if admitted {
            assert_eq!(result.unwrap().as_ref(), observed_reference.as_ref());
            assert_eq!(remaining, 0);
        } else {
            assert_eq!(result, Err(()));
        }
        assert_eq!(reference, reference_before);
        assert_eq!(source, before);
    }

    let mut remaining = ALLOWANCE;
    let observed_binding = relocation
        .retained_binding(&binding, |bytes| charge(&mut remaining, bytes))
        .unwrap();
    assert!(matches!(&observed_binding, Cow::Owned(_)));
    let used = ALLOWANCE - remaining;
    assert!(used > 1);
    for (allowance, admitted) in [(used, true), (used - 1, false)] {
        let mut remaining = allowance;
        let result = relocation.retained_binding(&binding, |bytes| charge(&mut remaining, bytes));
        if admitted {
            assert_eq!(result.unwrap().as_ref(), observed_binding.as_ref());
            assert_eq!(remaining, 0);
        } else {
            assert_eq!(result, Err(()));
        }
        assert_eq!(binding, binding_before);
        assert_eq!(source, before);
    }
    assert_eq!(source.entities.as_ptr(), original_entities);
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    let SourceDocumentInput::Prefab(original) = cut.source() else {
        panic!("reference and binding refusal must preserve the original Prefab borrow");
    };
    assert!(std::ptr::eq(original, &source));
}
