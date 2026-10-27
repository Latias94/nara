//! Failures retain established source ownership without inventing repair targets.

use super::*;

fn sources() -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new()
        .with_prefab(
            request("p"),
            key("p"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("n"))]),
        )
        .with_prefab(
            request("q"),
            key("q"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("e"))]),
        )
}

fn assert_location(
    report: &PrefabExpansionReport,
    source: PrefabDeclarationSource,
    anchor: &str,
    steps: Vec<PrefabInstancePathSegment>,
    slot: Option<PrefabOverlaySlot>,
) {
    assert!(report.candidate.is_none());
    let failure = report.failure.as_ref().expect("typed compilation failure");
    let location = failure.location().expect("established declaring source");
    assert_eq!(location.source(), &source);
    assert_eq!(
        location.path(),
        &PrefabInstancePath {
            anchor: id(anchor),
            steps,
        },
    );
    assert_eq!(failure.slot(), slot);
}

#[test]
fn an_unrelated_first_root_is_never_used_as_the_failure_anchor() {
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("unrelated")),
        anchor("actual", "p", Some(PrefabOverlay::new(key("retired")))),
    ]);
    let report = document.expand_prefabs(&registry(), &sources());
    assert_eq!(
        report.failure.as_ref().unwrap().reason(),
        &PrefabExpansionFailureKind::SourceChanged,
    );
    assert_location(
        &report,
        PrefabDeclarationSource::Containing,
        "actual",
        Vec::new(),
        None,
    );
}

#[test]
fn a_resolved_prefab_declaration_keeps_its_own_canonical_owner() {
    let sources = sources().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([anchor(
            "nested",
            "q",
            Some(PrefabOverlay::new(key("retired"))),
        )]),
    );
    let report = scene("p", None).expand_prefabs(&registry(), &sources);
    assert_location(
        &report,
        PrefabDeclarationSource::Prefab(key("p")),
        "nested",
        Vec::new(),
        None,
    );
}

#[test]
fn nested_additions_retain_source_array_selectors() {
    let mut addition = PrefabAddition::new(id("nested"));
    addition.prefab = Some(instance("q", Some(PrefabOverlay::new(key("retired")))));
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.additions.push(addition);
    overlay
        .placements
        .push(root(PrefabEntityTarget::addition(id("nested"))));
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources());
    assert_location(
        &report,
        PrefabDeclarationSource::Containing,
        "host",
        vec![PrefabInstancePathSegment::Addition { index: 0 }],
        None,
    );
}

#[test]
fn a_selected_instance_claim_is_located_in_its_declaring_snapshot() {
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("n")),
        value: Some(instance("q", Some(PrefabOverlay::new(key("retired"))))),
    });
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources());
    assert_location(
        &report,
        PrefabDeclarationSource::Containing,
        "host",
        vec![PrefabInstancePathSegment::InstanceClaim { index: 0 }],
        None,
    );
}

#[test]
fn a_missing_component_target_keeps_the_actual_intent_slot() {
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("missing")),
        component: ComponentTypeId::new("test.CompilerC"),
    });
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources());
    assert_eq!(
        report.failure.as_ref().unwrap().reason(),
        &PrefabExpansionFailureKind::MissingTarget,
    );
    assert_location(
        &report,
        PrefabDeclarationSource::Containing,
        "host",
        Vec::new(),
        Some(PrefabOverlaySlot::Component { index: 0 }),
    );
}

#[test]
fn an_invalid_component_operation_keeps_the_actual_intent_slot() {
    let mut overlay = PrefabOverlay::new(key("p"));
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: PrefabEntityTarget::source(id("n")),
        component: ComponentTypeId::new("test.CompilerC"),
    });
    let report = scene("p", Some(overlay)).expand_prefabs(&registry(), &sources());
    assert_eq!(
        report.failure.as_ref().unwrap().reason(),
        &PrefabExpansionFailureKind::Component,
    );
    assert_location(
        &report,
        PrefabDeclarationSource::Containing,
        "host",
        Vec::new(),
        Some(PrefabOverlaySlot::Component { index: 0 }),
    );
}

#[test]
fn a_global_budget_refusal_has_no_fabricated_declaration_or_intent() {
    let report = scene("p", None).expand_prefabs_with_options(
        &registry(),
        &sources(),
        PrefabExpansionOptions::default()
            .with_limits(PrefabExpansionLimits::default().with_working_bytes(ByteLimit::ONE)),
    );
    let failure = report.failure.as_ref().expect("typed budget refusal");
    assert_eq!(failure.reason(), &PrefabExpansionFailureKind::WorkLimit);
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
    assert!(report.diagnostics.has_errors());
}

#[test]
fn an_inherited_component_error_is_not_attributed_to_the_containing_overlay() {
    let mut source = SceneEntityRecord::new(id("n"));
    source.components.insert(
        ComponentTypeId::new("test.CompilerC"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::Bool(false))]),
        ),
    );
    let sources = sources().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([source]),
    );
    let report =
        scene("p", Some(PrefabOverlay::new(key("p")))).expand_prefabs(&registry(), &sources);
    let failure = report.failure.as_ref().expect("typed component refusal");
    assert_eq!(failure.reason(), &PrefabExpansionFailureKind::Component);
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
}

#[test]
fn invalid_loaded_source_structure_has_no_instance_repair_location() {
    let sources = sources().with_prefab(
        request("p"),
        key("p"),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("n")).with_children([id("missing")])
        ]),
    );
    let report = scene("p", None).expand_prefabs(&registry(), &sources);
    let failure = report.failure.as_ref().expect("typed source refusal");
    assert_eq!(
        failure.reason(),
        &PrefabExpansionFailureKind::OwnerAdmission
    );
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
    assert!(report.diagnostics.has_errors());
    assert!(report.candidate.is_none());
}

#[test]
fn a_loaded_scene_key_cannot_be_reported_as_an_instance_base_repair() {
    let wrong_kind = SourceDocumentKey::named(
        "17148e7d-a75b-4f3d-9132-759252fbd872".parse().unwrap(),
        "p",
        SourceDocumentKind::Scene,
    )
    .unwrap();
    let sources = InMemoryPrefabSourceResolver::new().with_prefab(
        request("p"),
        wrong_kind,
        PrefabDocument::default(),
    );
    let report = scene("p", None).expand_prefabs(&registry(), &sources);
    let failure = report.failure.as_ref().expect("typed source-kind refusal");
    assert_eq!(
        failure.reason(),
        &PrefabExpansionFailureKind::OwnerAdmission
    );
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-source-kind-invalid" })
    );
    assert!(report.candidate.is_none());
}

#[cfg(feature = "serde")]
#[test]
fn a_different_authoring_admission_is_not_a_repairable_base_change() {
    use crate::AuthoringSchemaBasis;
    use nara_reflect::AuthoringSchemaBundle;

    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    let foreign_basis = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    assert_ne!(basis, foreign_basis);
    let document = scene("p", None);
    let loaded = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("n")),
        ])),
        foreign_basis,
        key("p"),
    );
    let report = crate::__private::try_expand_authoring_prefabs_with_loader(
        &document,
        &basis,
        Default::default(),
        None,
        |_, _| Ok::<_, Infallible>(Some(loaded.clone())),
    )
    .unwrap();
    let failure = report.failure.as_ref().expect("typed admission refusal");
    assert_eq!(
        failure.reason(),
        &PrefabExpansionFailureKind::OwnerAdmission
    );
    assert!(failure.location().is_none());
    assert!(failure.slot().is_none());
    assert!(report.candidate.is_none());
}
