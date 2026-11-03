//! Actual source cuts exercise candidate construction, not a second hierarchy evaluator.

use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::AuthoringSchemaBundle;

use super::*;
use crate::{
    AuthoringSchemaBasis, PrefabAddition, PrefabDocument, PrefabInstance, PrefabInstanceHop,
    PrefabOverlay, ResolvedPrefabSource, SceneCompilationLimits, SceneDocument, SceneEntityId,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
    prefab::extraction::try_capture_source_cut_with_loader,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "77b161d7-12df-4d5a-8ef2-a06617c0308f".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    source_key(value, SourceDocumentKind::Prefab)
}

fn request() -> AssetRef {
    AssetRef::path("prefabs/extraction-placements/r.ron").unwrap()
}

fn local(entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget::source(id(entity))
}

fn inside(anchor: &str, origin: PrefabEntityOrigin) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: key("r"),
        }],
        entity: origin,
    }
}

fn inherited(anchor: &str, entity: &str) -> PrefabEntityTarget {
    inside(anchor, PrefabEntityOrigin::SourceEntity(id(entity)))
}

fn retained(mut target: PrefabEntityTarget) -> PrefabEntityTarget {
    target.through.insert(
        0,
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("k")),
            expected_base: key("q"),
        },
    );
    target
}

fn under(target: PrefabEntityTarget, parent: PrefabEntityTarget) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        target,
        ScenePlacement::last(ScenePlacementParent::Entity(parent)),
    )
}

fn bound(anchor: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(anchor));
    record.prefab = Some(PrefabInstance::root(request(), key("r"), local("r-root")));
    record
}

fn base() -> Arc<PrefabDocument> {
    Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("r-root")),
            under(local("d1"), local("r-root")),
            under(local("d2"), local("d1")),
        ],
        ["r-root", "d1", "d2"].map(|name| SceneEntityRecord::new(id(name))),
    ))
}

fn capture<'source>(
    source: SourceDocumentInput<'source>,
    r: &Arc<PrefabDocument>,
) -> (SourceCut<'source>, usize) {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let report = try_capture_source_cut_with_loader(
        source,
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| {
            assert_eq!(reference, &request());
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(r),
                basis.clone(),
                key("r"),
            )))
        },
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    (report.cut.unwrap(), report.work_bytes.unwrap())
}

#[test]
fn internal_mixed_parent_and_distinct_addition_anchor_move_in_stored_order() {
    let r = base();
    let r_before = r.as_ref().clone();
    let mut k = bound("k");
    let mut overlay = PrefabOverlay::new(key("r"));
    overlay.additions.push(PrefabAddition::new(id("d2")));
    overlay
        .placements
        .push(under(PrefabEntityTarget::addition(id("d2")), local("d1")));
    k.prefab.as_mut().unwrap().overrides = Some(overlay);
    let source = PrefabDocument {
        inputs: Default::default(),
        placements: vec![
            SceneEntityPlacement::root(id("outer")),
            under(local("x"), local("outer")),
            SceneEntityPlacement::new(
                local("k"),
                ScenePlacement::before(ScenePlacementParent::Entity(local("outer")), local("x")),
            ),
            SceneEntityPlacement::new(
                local("b"),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(inherited("k", "d1")),
                    inherited("k", "d2"),
                ),
            ),
            SceneEntityPlacement::new(
                inherited("k", "d2"),
                ScenePlacement::after(
                    ScenePlacementParent::Entity(inherited("k", "d1")),
                    inside("k", PrefabEntityOrigin::InstanceAddition(id("d2"))),
                ),
            ),
        ],
        entities: vec![
            SceneEntityRecord::new(id("b")),
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("outer")),
            k,
        ],
    };
    let before = source.clone();
    let (cut, captured) = capture(SourceDocumentInput::Prefab(&source), &r);
    assert_eq!(
        cut.entities()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"],
    );
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, Some(captured)).unwrap();
    let split = split_placements(&cut, &relocation, &mut work).unwrap();

    assert_eq!(split.retained, source.placements[..3]);
    assert_eq!(
        split.extracted,
        [
            SceneEntityPlacement::root(id("k")),
            source.placements[3].clone(),
            source.placements[4].clone(),
        ],
    );
    assert_eq!(
        split.originals,
        [
            PlacementDestination::Retained(0),
            PlacementDestination::Retained(1),
            PlacementDestination::Retained(2),
            PlacementDestination::Extracted(1),
            PlacementDestination::Extracted(2),
        ],
    );
    assert!(std::ptr::eq(cut.root(), &source.entities[3]));
    assert_eq!(source, before);
    assert_eq!(r.as_ref(), &r_before);
}

#[test]
fn outside_slot_sibling_keeps_full_p_pins_and_one_q_baseline_with_exact_budget() {
    let r = base();
    let r_before = r.as_ref().clone();
    let source = PrefabDocument {
        inputs: Default::default(),
        placements: vec![
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::root(id("outside")),
            under(inherited("outside", "d2"), inherited("k", "d1")),
            SceneEntityPlacement::new(
                local("b"),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(inherited("k", "d1")),
                    inherited("outside", "d2"),
                ),
            ),
            SceneEntityPlacement::new(
                inherited("k", "d2"),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(inherited("k", "d1")),
                    inherited("outside", "d2"),
                ),
            ),
        ],
        entities: vec![
            SceneEntityRecord::new(id("b")),
            bound("outside"),
            bound("k"),
        ],
    };
    let before = source.clone();
    let original_placements = source.placements.as_ptr();
    let (cut, captured) = capture(SourceDocumentInput::Prefab(&source), &r);
    assert_eq!(
        cut.entities()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"],
    );
    let (p, q) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, Some(captured)).unwrap();
    let split = split_placements(&cut, &relocation, &mut work).unwrap();
    let (nodes, bytes) = work.usage();

    assert_eq!(
        split.extracted,
        [
            SceneEntityPlacement::root(id("k")),
            // The complete coupled parent/sibling intent remains in P. Q uses a safe local
            // baseline that cannot recreate a base-source parent cycle before those pins.
            under(local("b"), local("k")),
        ],
    );
    assert_eq!(
        split.retained,
        [
            source.placements[0].clone(),
            source.placements[1].clone(),
            under(inherited("outside", "d2"), retained(inherited("k", "d1"))),
            SceneEntityPlacement::new(
                retained(local("b")),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(retained(inherited("k", "d1"))),
                    inherited("outside", "d2"),
                ),
            ),
            SceneEntityPlacement::new(
                retained(inherited("k", "d2")),
                ScenePlacement::before(
                    ScenePlacementParent::Entity(retained(inherited("k", "d1"))),
                    inherited("outside", "d2"),
                ),
            ),
        ],
    );
    assert_eq!(
        split.originals,
        (0..source.placements.len())
            .map(PlacementDestination::Retained)
            .collect::<Vec<_>>(),
    );

    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap());
    let mut work = ExtractionWork::from_cut(exact, &cut, Some(captured)).unwrap();
    let repeated = split_placements(&cut, &relocation, &mut work).unwrap();
    assert_eq!(work.usage(), (nodes, bytes));
    assert_eq!(repeated.retained, split.retained);
    assert_eq!(repeated.extracted, split.extracted);
    assert_eq!(repeated.originals, split.originals);
    for short in [
        exact.with_working_bytes(ByteLimit::new(bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
    ] {
        let mut work = ExtractionWork::from_cut(short, &cut, Some(captured)).unwrap();
        assert!(matches!(
            split_placements(&cut, &relocation, &mut work),
            Err(ExtractionError::Capacity),
        ));
        assert_eq!(source, before);
        assert_eq!(source.placements.as_ptr(), original_placements);
        assert!(std::ptr::eq(cut.root(), &source.entities[2]));
        assert_eq!(r.as_ref(), &r_before);
    }
}

#[test]
fn inherited_target_at_containing_scene_root_remains_a_pin_without_an_extra_q_root() {
    let r = base();
    let r_before = r.as_ref().clone();
    let source = SceneDocument {
        placements: vec![
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::root(id("x")),
            under(local("b"), inherited("k", "d1")),
            SceneEntityPlacement::new(
                inherited("k", "d2"),
                ScenePlacement::before(ScenePlacementParent::Root, local("x")),
            ),
        ],
        entities: vec![
            bound("k"),
            SceneEntityRecord::new(id("x")),
            SceneEntityRecord::new(id("b")),
        ],
    };
    let before = source.clone();
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &r);
    let p = source_key("p", SourceDocumentKind::Scene);
    let q = key("q");
    let relocation = BranchRelocation::new(&cut, &p, &q).unwrap();
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, Some(captured)).unwrap();
    let split = split_placements(&cut, &relocation, &mut work).unwrap();

    assert_eq!(
        split.retained,
        [
            source.placements[0].clone(),
            source.placements[1].clone(),
            SceneEntityPlacement::new(
                retained(inherited("k", "d2")),
                ScenePlacement::before(ScenePlacementParent::Root, local("x")),
            ),
        ],
    );
    assert_eq!(
        split.extracted,
        [
            SceneEntityPlacement::root(id("k")),
            source.placements[2].clone()
        ],
    );
    assert_eq!(
        split.originals,
        [
            PlacementDestination::Retained(0),
            PlacementDestination::Retained(1),
            PlacementDestination::Extracted(1),
            PlacementDestination::Retained(2),
        ],
    );
    assert_eq!(
        split
            .extracted
            .iter()
            .filter(|placement| { placement.placement.parent == ScenePlacementParent::Root })
            .count(),
        1,
    );
    assert_eq!(source, before);
    assert_eq!(r.as_ref(), &r_before);
}
