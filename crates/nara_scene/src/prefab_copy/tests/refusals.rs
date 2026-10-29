use super::*;

pub(super) fn prepare_error(request: &PrefabCopyRequest) -> PrefabCopyError {
    let before: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let error = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap_err();
    for (source, before) in request.sources.iter().zip(before) {
        assert_eq!(source.document, before);
    }
    error
}

#[test]
fn ambiguous_or_nonindependent_source_mappings_are_refused() {
    let source = || mapped_source("leaf", PrefabDocument::default());
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![])),
        PrefabCopyError::EmptyClosure
    ));
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(
            (0..33)
                .map(|index| mapped_source(&format!("p{index}"), PrefabDocument::default()))
                .collect()
        )),
        PrefabCopyError::DocumentLimit
    ));
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![source(), source()])),
        PrefabCopyError::DuplicateOriginal { .. }
    ));
    let mut other = mapped_source("other", PrefabDocument::default());
    other.destination = key("copied-leaf");
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![source(), other])),
        PrefabCopyError::DuplicateDestination { .. }
    ));
    let mut other = source();
    other.destination = other.original.clone();
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![other])),
        PrefabCopyError::DestinationNotIndependent { .. }
    ));
    let mut other = source();
    other.original_references.push(reference("leaf"));
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![other])),
        PrefabCopyError::DuplicateAlias { .. }
    ));
    let mut other = mapped_source("other", PrefabDocument::default());
    other.original_references = vec![reference("leaf")];
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![source(), other])),
        PrefabCopyError::DuplicateAlias { .. }
    ));
    let mut other = source();
    other.destination_reference = reference("leaf");
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![other])),
        PrefabCopyError::DestinationAliasCollision { .. }
    ));
    let mut other = source();
    other.original_references.clear();
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![other])),
        PrefabCopyError::MissingAliases { .. }
    ));
    let mut other = source();
    other.original = SourceDocumentKey::named(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        "scene",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    assert!(matches!(
        prepare_error(&PrefabCopyRequest::new(vec![other])),
        PrefabCopyError::InvalidDocumentKind { .. }
    ));
}

#[test]
fn unsupplied_dependencies_and_identity_facts_have_no_fallback() {
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "outer",
        PrefabDocument::from_ordered_roots([anchor("host", "external", None)]),
    )]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::UnmappedAssetReference { .. }
    ));
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf")).with_component(
            component("Assets"),
            record(ComponentValue::map([
                ("asset", asset(&reference("external"))),
                ("plain", ComponentValue::String("unchanged".into())),
            ])),
        )]),
    )]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::UnmappedAssetReference { .. }
    ));
    let mut request = nested_fixture();
    request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .suppressions[0]
        .through[0]
        .expected_base = key("external");
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::UnmappedSourceKey { .. }
    ));
    let mut request = nested_fixture();
    request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .base = key("external");
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::UnmappedSourceKey { .. }
    ));
}

#[test]
fn auxiliary_prefab_source_cannot_be_copied_into_a_primary_alias() {
    let old: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: product(old, false),
        overrides: None,
    });
    let request = PrefabCopyRequest::new(vec![
        mapped_source("outer", PrefabDocument::from_ordered_roots([anchor])),
        PrefabCopySource::new(
            key("leaf"),
            SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab),
            vec![product(old, false)],
            product(new, true),
            PrefabDocument::default(),
        ),
    ]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::AuxiliaryPrefabSource { .. }
    ));
}

#[test]
fn dormant_unknown_future_or_wrong_owner_records_still_refuse_copying() {
    for kind in 0..3 {
        let mut request = nested_fixture();
        let overlay = request.sources[0].document.entities[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap();
        let mut record = number();
        let ty = if kind == 0 {
            component("Unknown")
        } else {
            component("Number")
        };
        if kind == 1 {
            record.version = ComponentSchemaVersion::new(2).unwrap();
        }
        if kind == 2 {
            record.owner =
                Some(PersistentComponentSchemaOwnerId::try_new("test.some-other-owner").unwrap());
        }
        overlay.additions[0].components.insert(ty.clone(), record);
        overlay.components.push(PrefabComponentIntent::Suppress {
            target: PrefabEntityTarget::addition(id("added")),
            component: ty,
        });
        let PrefabCopyError::Schema { reason, .. } = prepare_error(&request) else {
            panic!("stored Schema must be interpreted");
        };
        assert_eq!(
            reason,
            [
                PrefabCopySchemaError::UnknownComponent,
                PrefabCopySchemaError::VersionMismatch,
                PrefabCopySchemaError::OwnerMismatch
            ][kind]
        );
    }
}

#[test]
fn persistent_references_and_absent_reference_defaults_are_refused() {
    let persistent = nara_reflect::EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference {
            namespace: nara_identity::PersistentRuntimeNamespaceId::new("test.external").unwrap(),
            entity: nara_identity::PersistentRuntimeId::parse_str(
                "00000000-0000-4000-8000-000000000033",
            )
            .unwrap(),
        },
    };
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf")).with_component(
            component("Link"),
            record(ComponentValue::map([(
                "target",
                ComponentValue::EntityReference(persistent),
            )])),
        )]),
    )]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::PersistentReference { .. }
    ));
    let mut request = nested_fixture();
    let overlay = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = &mut overlay.components[1] else {
        panic!("asset property");
    };
    claims[0].intent = PrefabPropertyIntent::Absent;
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::ReferenceDefaultAbsent { .. }
    ));
}

#[test]
fn deep_programmatic_values_and_nested_declarations_refuse_before_copy() {
    let mut value = ComponentValue::U64(1);
    for _ in 0..90 {
        value = ComponentValue::List(vec![value]);
    }
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf")).with_component(component("Number"), record(value))
        ]),
    )]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::BudgetExceeded
    ));
    let mut nested = instance("leaf", None);
    for index in 0..90 {
        let mut overlay = PrefabOverlay::new(key("leaf"));
        let mut addition = PrefabAddition::new(id(&format!("added{index}")));
        addition.prefab = Some(nested);
        overlay.additions.push(addition);
        nested = instance("leaf", Some(overlay));
    }
    let mut entity = SceneEntityRecord::new(id("host"));
    entity.prefab = Some(nested);
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([entity]),
    )]);
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::BudgetExceeded
    ));
}

#[test]
fn preparation_limits_apply_to_the_whole_supplied_closure() {
    let limits = PrefabCopyLimits::new(10, 100_000, 1_000_000, 100_000, 8 * 1024 * 1024).unwrap();
    let source = |name: &str| {
        mapped_source(
            name,
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
        )
    };
    PrefabCopyRequest::new(vec![source("one")])
        .prepare(&basis(), limits)
        .unwrap();
    assert!(matches!(
        PrefabCopyRequest::new(vec![source("one"), source("two"), source("three")])
            .prepare(&basis(), limits),
        Err(PrefabCopyError::BudgetExceeded)
    ));

    let limits = PrefabCopyLimits::new(10_000, 20, 1_000_000, 100_000, 8 * 1024 * 1024).unwrap();
    let leaf = || {
        mapped_source(
            "leaf",
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("leaf")).with_component(component("Assets"), assets())
            ]),
        )
    };
    PrefabCopyRequest::new(vec![leaf()])
        .prepare(&basis(), limits)
        .unwrap();
    let other = mapped_source(
        "other",
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("other")).with_component(component("Assets"), assets())
        ]),
    );
    assert!(matches!(
        PrefabCopyRequest::new(vec![leaf(), other]).prepare(&basis(), limits),
        Err(PrefabCopyError::BudgetExceeded) | Err(PrefabCopyError::Compiler { .. })
    ));
}

#[test]
fn actual_compiler_failure_is_distinct_after_successful_reference_mapping() {
    let mut request = nested_fixture();
    // This key is admitted and remaps successfully, but does not identify the actual source.
    request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .base = key("leaf");
    assert!(matches!(
        prepare_error(&request),
        PrefabCopyError::Compiler { .. }
    ));
}
