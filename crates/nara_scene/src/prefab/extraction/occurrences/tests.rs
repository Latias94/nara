//! Canonical provenance proposals, using a real structural cut and an empty authoring basis.

use std::{collections::BTreeSet, convert::Infallible};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::AuthoringSchemaBundle;

use super::relocate_provenance;
use crate::{
    AuthoringSchemaBasis, PrefabDocument, PrefabEntityTarget, PrefabProjectionPath,
    PrefabProjectionStep, ResolvedPrefabSource, SceneCompilationLimits, SceneEntityId,
    SceneEntityPlacement, SceneEntityProvenance, SceneEntityRecord, ScenePlacement,
    ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    prefab::{
        SourceDocumentInput,
        extraction::{
            ExtractionError, ExtractionWork, SourceCut, try_capture_source_cut_with_loader,
        },
    },
    provenance::CanonicalSceneEntityAddress,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "bc03e23d-6677-4d99-86eb-4a4961b6bcbe".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn lookup(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/occurrences/{value}.ron")).unwrap()
}

fn source_step(alias: &str, source: &str, entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::SourceEntity {
        source_document: lookup(alias),
        canonical_source: key(source),
        entity: id(entity),
    }
}

fn addition(entity: &str) -> PrefabProjectionStep {
    PrefabProjectionStep::InstanceAddition { entity: id(entity) }
}

fn projected(
    anchor: &str,
    local: &str,
    steps: impl IntoIterator<Item = PrefabProjectionStep>,
) -> SceneEntityProvenance {
    SceneEntityProvenance::projected(
        id(local),
        PrefabProjectionPath::from_resolved_steps(id(anchor), steps.into_iter().collect()),
    )
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
        // Declaration order is deliberately different from the hierarchy's order.
        entities: vec![
            SceneEntityRecord::new(id("b")),
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("k")),
        ],
    }
}

fn capture(source: &PrefabDocument) -> (SourceCut<'_>, Option<usize>) {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(source),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("a direct-only cut must not load {reference:?}")
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    let cut = report.cut.unwrap();
    assert_eq!(cut.reference_work, report.reference_work);
    (cut, report.work_bytes)
}

#[test]
fn member_comparisons_consume_reference_bytes_and_capture_work_is_not_reset() {
    let source = source();
    let (cut, bytes) = capture(&source);
    let defaults = SceneCompilationLimits::default();
    let work = ExtractionWork::from_cut(defaults, &cut, bytes).unwrap();
    assert_eq!(work.usage(), (cut.reference_work.0, bytes.unwrap()));
    assert!(cut.reference_work.0 > 0 && cut.reference_work.1 > 0);
    let remaining = work.compiler_options(32).unwrap().limits;
    assert_eq!(
        remaining.reference_work_nodes().get(),
        defaults.reference_work_nodes().get() - cut.reference_work.0
    );
    assert_eq!(
        remaining.reference_work_bytes().get(),
        defaults.reference_work_bytes().get() - cut.reference_work.1
    );
    let mut short =
        ExtractionWork::new(defaults.with_reference_work_bytes(ByteLimit::ONE), Some(0)).unwrap();
    assert!(matches!(
        super::cut_contains(&cut, &id("b"), &mut short),
        Err(ExtractionError::Capacity)
    ));
    let mut baseline = ExtractionWork::new(defaults, Some(0)).unwrap();
    assert!(super::cut_contains(&cut, &id("b"), &mut baseline).unwrap());
    let used = defaults.reference_work_bytes().get()
        - baseline
            .compiler_options(32)
            .unwrap()
            .limits
            .reference_work_bytes()
            .get();
    assert!(used > 1);
    let mut exact = ExtractionWork::new(
        defaults.with_reference_work_bytes(ByteLimit::new(used).unwrap()),
        Some(0),
    )
    .unwrap();
    assert!(super::cut_contains(&cut, &id("b"), &mut exact).unwrap());
}

#[test]
fn local_nonroot_moves_but_root_and_other_containing_sources_keep_their_entities() {
    let source = source();
    let before = source.clone();
    let (cut, captured) = capture(&source);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    for (containing, local, expected) in [
        (
            "p",
            "b",
            projected("k", "b", [source_step("q-alias", "q", "b")]),
        ),
        ("p", "k", SceneEntityProvenance::local(id("k"))),
        ("p", "x", SceneEntityProvenance::local(id("x"))),
        ("other", "b", SceneEntityProvenance::local(id("b"))),
        ("other", "k", SceneEntityProvenance::local(id("k"))),
    ] {
        let provenance = SceneEntityProvenance::local(id(local));
        let proposed = relocate_provenance(
            &cut,
            &key("p"),
            &key("q"),
            &lookup("q-alias"),
            &key(containing),
            &provenance,
            &mut work,
        )
        .unwrap();
        assert_eq!(proposed, expected);
        assert_ne!(
            proposed.local_entity_id().as_str().as_ptr(),
            provenance.local_entity_id().as_str().as_ptr(),
        );
    }
    assert_eq!(source, before);
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
}

#[test]
fn containing_p_projection_enters_each_moved_original_instance_slot() {
    let source = source();
    let (cut, captured) = capture(&source);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    for (containing, anchor, expected_anchor, prefix) in [
        ("p", "k", "k", Some("k")),
        ("p", "b", "k", Some("b")),
        ("p", "x", "x", None),
        ("other", "b", "b", None),
    ] {
        let tail = source_step("r-alias", "r", "leaf");
        let provenance = projected(anchor, "leaf", [tail.clone()]);
        let mut expected = Vec::new();
        if let Some(entity) = prefix {
            expected.push(source_step("q-alias", "q", entity));
        }
        expected.push(tail);
        let proposed = relocate_provenance(
            &cut,
            &key("p"),
            &key("q"),
            &lookup("q-alias"),
            &key(containing),
            &provenance,
            &mut work,
        )
        .unwrap();
        assert_eq!(proposed, projected(expected_anchor, "leaf", expected));
    }
}

#[test]
fn repeated_p_steps_rewrite_independently_and_only_root_slot_steps_gain_q_k() {
    let source = source();
    let (cut, captured) = capture(&source);
    let provenance = projected(
        "host",
        "k",
        [
            source_step("p-first", "p", "k"),
            source_step("r", "r", "bridge"),
            source_step("p-second", "p", "b"),
            source_step("q-original", "q", "k"),
            source_step("p-unmoved", "p", "x"),
            source_step("p-endpoint", "p", "k"),
        ],
    );
    let before = provenance.clone();
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let proposed = relocate_provenance(
        &cut,
        &key("p"),
        &key("q"),
        &lookup("q-alias"),
        &key("s"),
        &provenance,
        &mut work,
    )
    .unwrap();
    assert_eq!(
        proposed,
        projected(
            "host",
            "k",
            [
                source_step("p-first", "p", "k"),
                source_step("q-alias", "q", "k"),
                source_step("r", "r", "bridge"),
                source_step("p-second", "p", "k"),
                source_step("q-alias", "q", "b"),
                source_step("q-original", "q", "k"),
                source_step("p-unmoved", "p", "x"),
                source_step("p-endpoint", "p", "k"),
            ],
        ),
    );
    assert_eq!(provenance, before);
}

#[test]
fn additions_keep_their_namespace_and_aliases_keep_evidence_without_changing_canonical_addresses() {
    let source = source();
    let (cut, captured) = capture(&source);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let mut proposed = Vec::new();
    for alias in ["p-alias-a", "p-alias-b"] {
        let provenance = projected(
            "host",
            "b",
            [
                source_step(alias, "p", "k"),
                addition("b"),
                source_step(alias, "p", "b"),
                addition("k"),
                source_step("r", "r", "b"),
                addition("b"),
            ],
        );
        let after = relocate_provenance(
            &cut,
            &key("p"),
            &key("q"),
            &lookup("q-alias"),
            &key("s"),
            &provenance,
            &mut work,
        )
        .unwrap();
        assert_eq!(
            after,
            projected(
                "host",
                "b",
                [
                    source_step(alias, "p", "k"),
                    source_step("q-alias", "q", "k"),
                    addition("b"),
                    source_step(alias, "p", "k"),
                    source_step("q-alias", "q", "b"),
                    addition("k"),
                    source_step("r", "r", "b"),
                    addition("b"),
                ],
            ),
        );
        proposed.push(after);
    }
    assert_ne!(proposed[0], proposed[1]);
    assert_eq!(
        CanonicalSceneEntityAddress::new(&proposed[0]),
        CanonicalSceneEntityAddress::new(&proposed[1]),
    );
    let addresses = proposed
        .iter()
        .map(CanonicalSceneEntityAddress::new)
        .collect::<BTreeSet<_>>();
    assert_eq!(addresses.len(), 1);
    let source_b = projected("host", "b", [source_step("r", "r", "b")]);
    let addition_b = projected("host", "b", [addition("b")]);
    assert_ne!(
        CanonicalSceneEntityAddress::new(&source_b),
        CanonicalSceneEntityAddress::new(&addition_b),
    );
}

#[test]
fn cumulative_exact_and_one_short_allowances_preserve_all_inputs_and_own_unchanged_outputs() {
    let source = source();
    let source_before = source.clone();
    let entity_storage = source.entities.as_ptr();
    let (cut, captured) = capture(&source);
    let (p, q, reference) = (key("p"), key("q"), lookup("q-alias"));
    let inputs = [
        SceneEntityProvenance::local(id("b")),
        SceneEntityProvenance::local(id("k")),
        projected("b", "leaf", [source_step("r", "r", "leaf")]),
        projected("x", "b", [source_step("r", "r", "b"), addition("b")]),
        projected(
            "x",
            "b",
            [
                source_step("p-alias", "p", "k"),
                addition("b"),
                source_step("p-again", "p", "b"),
            ],
        ),
    ];
    let before = inputs.clone();
    let routes = inputs
        .iter()
        .map(|origin| origin.projection_path().map(|path| path.steps().as_ptr()))
        .collect::<Vec<_>>();
    let run = |work: &mut ExtractionWork| {
        inputs
            .iter()
            .map(|origin| relocate_provenance(&cut, &p, &q, &reference, &p, origin, work))
            .collect::<Result<Vec<_>, _>>()
    };
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let expected = run(&mut work).unwrap();
    let (nodes, bytes) = work.usage();
    assert!(nodes > 1);
    assert!(bytes > captured.unwrap());
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap());
    let mut work = ExtractionWork::from_cut(exact, &cut, captured).unwrap();
    assert_eq!(run(&mut work).unwrap(), expected);
    assert_eq!(work.usage(), (nodes, bytes));
    for short in [
        exact.with_working_bytes(ByteLimit::new(bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
    ] {
        let mut work = ExtractionWork::from_cut(short, &cut, captured).unwrap();
        assert!(matches!(run(&mut work), Err(ExtractionError::Capacity)));
        assert_eq!(inputs, before);
        assert_eq!(source, source_before);
        assert_eq!(source.entities.as_ptr(), entity_storage);
        for (origin, route) in inputs.iter().zip(&routes) {
            assert_eq!(
                origin.projection_path().map(|path| path.steps().as_ptr()),
                *route,
            );
        }
    }
    let unchanged = &expected[3];
    assert_eq!(unchanged, &inputs[3]);
    assert_ne!(
        unchanged.projection_path().unwrap().steps().as_ptr(),
        inputs[3].projection_path().unwrap().steps().as_ptr(),
    );
    assert_ne!(
        unchanged.local_entity_id().as_str().as_ptr(),
        inputs[3].local_entity_id().as_str().as_ptr(),
    );
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
}
