use super::*;
use crate::prefab::extraction::try_capture_source_cut_with_loader;
use crate::{
    PrefabDocument, SceneCompilationLimits, SceneEntityId, SceneEntityRecord, SourceDocumentKind,
};
use nara_core::ItemLimit;
use nara_reflect::{AuthoringSchemaBundle, ComponentSchemaVersion, ComponentValue};
use std::convert::Infallible;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

#[test]
fn late_raw_limit_in_a_preserved_guard_keeps_prefix_work_spent_on_every_retry() {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let selected = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))]);
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(&selected),
        &id("root"),
        &basis,
        Default::default(),
        None,
        |_, _| Ok::<_, Infallible>(None),
    )
    .unwrap();
    assert!(report.failure.is_none());
    let cut = report.cut.unwrap();
    let key = |name| {
        SourceDocumentKey::named(
            "bdd9f9dd-8ce8-4873-a532-e9ee117cd1a2".parse().unwrap(),
            name,
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    };
    let p = key("P");
    let q = key("Q");
    let r = key("R");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::List(
            (0..8192)
                .map(|_| ComponentValue::List(vec![ComponentValue::String("x".into())]))
                .collect(),
        ),
    );
    let guard = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("guard"))
        .with_component(ComponentTypeId::new("unknown.RawGuard"), record)]);
    let mut work = ExtractionWork::new(
        SceneCompilationLimits::default()
            .with_reference_work_nodes(ItemLimit::new(10_000).unwrap()),
        Some(0),
    )
    .unwrap();
    let before = work.usage();
    assert!(matches!(
        requires_relocation(
            SourceDocumentInput::Prefab(&guard),
            &r,
            &relocation,
            &basis,
            true,
            &mut work
        ),
        Err(ExtractionError::Capacity)
    ));
    let first = work.usage();
    assert!(first.0 > before.0 + 3000, "{before:?} -> {first:?}");
    assert!(first.1 > before.1);
    assert!(matches!(
        requires_relocation(
            SourceDocumentInput::Prefab(&guard),
            &r,
            &relocation,
            &basis,
            true,
            &mut work
        ),
        Err(ExtractionError::Capacity)
    ));
    assert!(work.usage().0 > first.0, "{first:?} -> {:?}", work.usage());
    assert!(work.usage().1 >= first.1);
}
