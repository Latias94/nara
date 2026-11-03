//! Actual source construction must preserve coupled placement intent across P and Q.
//! These empty-Schema authoring witnesses grant no Native or publication authority.

use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_reflect::AuthoringSchemaBundle;

use crate::{
    AuthoringSchemaBasis, ExpandedSceneDocument, PrefabDocument, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabProjectionStep,
    ResolvedPrefabSource, SceneDocument, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    prefab::{
        extraction::{
            SourceDocumentInput,
            construction::{SplitSourceDocument, UnpublishedSourceSplit, construct_source_split},
            try_capture_source_cut_with_loader,
            work::ExtractionWork,
        },
        try_compile_authoring_scene_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "080284a1-791a-4896-a66a-aebcb88e68ad".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/extraction-coupling/{value}.ron")).unwrap()
}

fn target(route: &[(&str, &str)], entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: route
            .iter()
            .map(|(anchor, base)| PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: key(base),
            })
            .collect(),
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn under(child: PrefabEntityTarget, parent: PrefabEntityTarget) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        child,
        ScenePlacement::last(ScenePlacementParent::Entity(parent)),
    )
}

fn forest(anchor: &str, source: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(anchor));
    record.prefab = Some(PrefabInstance::forest(request(source)));
    record
}

fn consumer(source: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([forest("host", source)])
}

fn load(
    reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> Option<ResolvedPrefabSource> {
    let (name, document) = sources
        .iter()
        .find(|(name, _)| request(name) == *reference)?;
    Some(ResolvedPrefabSource::bound_authoring_source(
        Arc::clone(document),
        basis.clone(),
        key(name),
    ))
}

fn split<'source>(
    p: &'source PrefabDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> UnpublishedSourceSplit<'source> {
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(p),
        &id("k"),
        basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(load(reference, basis, sources)),
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    assert!(report.work_bytes.is_some_and(|bytes| bytes > 0));
    let cut = report.cut.unwrap();
    assert_eq!(
        cut.entities()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "k"],
    );
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, report.work_bytes).unwrap();
    let result =
        construct_source_split(&cut, &key("p"), &key("q"), &request("q"), basis, &mut work)
            .unwrap();
    assert!(
        matches!(result.original, SourceDocumentInput::Prefab(source) if std::ptr::eq(source, p))
    );
    assert_eq!(&result.basis, basis);
    let SplitSourceDocument::Prefab(containing) = &result.containing else {
        panic!("the containing source must remain a Prefab")
    };
    assert_eq!(
        containing
            .entities
            .iter()
            .find(|record| record.id == id("o")),
        p.entities.iter().find(|record| record.id == id("o")),
    );
    assert!(
        !result
            .extracted
            .entities
            .iter()
            .any(|record| record.id == id("o"))
    );
    result
}

fn compile(
    scene: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, Arc<PrefabDocument>)],
) -> ExpandedSceneDocument {
    let report = try_compile_authoring_scene_with_loader(
        scene,
        basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(load(reference, basis, sources)),
    )
    .unwrap();
    assert!(!report.diagnostics().has_errors(), "{report:?}");
    assert!(report.failure().is_none(), "{report:?}");
    assert!(!report.budget_exceeded());
    report.into_parts().candidate.unwrap()
}

type Hierarchy = BTreeMap<&'static str, Vec<&'static str>>;

fn observe(
    document: &ExpandedSceneDocument,
    routes: &[(&'static str, &[(&str, &str)])],
) -> Hierarchy {
    let mut names = BTreeMap::from([(id("host"), "host")]);
    for (name, route) in routes {
        let steps = route
            .iter()
            .map(|(base, entity)| PrefabProjectionStep::SourceEntity {
                source_document: request(base),
                canonical_source: key(base),
                entity: id(entity),
            })
            .collect::<Vec<_>>();
        let mut matches = document.provenance().iter().filter(|(_, origin)| {
            origin.projection_path().is_some_and(|path| {
                path.root_anchor() == &id("host") && path.steps() == steps.as_slice()
            })
        });
        let (entity, _) = matches
            .next()
            .expect("the exact typed occurrence must exist");
        assert!(matches.next().is_none());
        assert!(names.insert(entity.clone(), *name).is_none());
    }
    assert_eq!(document.entities().len(), names.len());
    assert_eq!(document.provenance().len(), names.len());
    assert_eq!(document.roots(), &[id("host")]);
    assert!(
        document.provenance()[&id("host")]
            .projection_path()
            .is_none()
    );
    document
        .entities()
        .iter()
        .map(|entity| {
            assert!(entity.components().is_empty());
            (
                names[entity.id()],
                entity.children().iter().map(|child| names[child]).collect(),
            )
        })
        .collect()
}

#[test]
fn actual_split_preserves_last_cohort_with_a_retained_external_instance_slot() {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let r = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("d")),
    ]));
    let r_before = r.as_ref().clone();
    let p = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::root(id("o")),
            under(target(&[("o", "r")], "d"), target(&[], "k")),
            under(target(&[], "b"), target(&[], "k")),
        ],
        [
            SceneEntityRecord::new(id("k")),
            forest("o", "r"),
            SceneEntityRecord::new(id("b")),
        ],
    );
    let p_before = p.clone();
    let sources_before = [("p", Arc::new(p.clone())), ("r", Arc::clone(&r))];
    let before_routes: &[(&str, &[(&str, &str)])] = &[
        ("k", &[("p", "k")]),
        ("o", &[("p", "o")]),
        ("b", &[("p", "b")]),
        ("d", &[("p", "o"), ("r", "d")]),
    ];
    let expected = observe(
        &compile(&consumer("p"), &basis, &sources_before),
        before_routes,
    );
    assert_eq!(
        expected,
        BTreeMap::from([
            ("host", vec!["k", "o"]),
            ("k", vec!["d", "b"]),
            ("o", vec![]),
            ("d", vec![]),
            ("b", vec![]),
        ])
    );

    let split = split(&p, &basis, &sources_before);
    let SplitSourceDocument::Prefab(after) = &split.containing else {
        unreachable!()
    };
    let sources_after = [
        ("p", Arc::new(after.clone())),
        ("q", Arc::new(split.extracted.clone())),
        ("r", Arc::clone(&r)),
    ];
    let after_routes: &[(&str, &[(&str, &str)])] = &[
        ("k", &[("p", "k")]),
        ("o", &[("p", "o")]),
        ("b", &[("p", "k"), ("q", "b")]),
        ("d", &[("p", "o"), ("r", "d")]),
    ];
    assert_eq!(
        observe(
            &compile(&consumer("p"), &basis, &sources_after),
            after_routes
        ),
        expected
    );
    assert_eq!(p, p_before);
    assert_eq!(r.as_ref(), &r_before);
}

#[test]
fn actual_split_keeps_cross_cut_parent_repairs_coupled_without_a_q_cycle() {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let r = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("h")),
            under(target(&[], "d"), target(&[], "h")),
        ],
        [
            SceneEntityRecord::new(id("h")),
            SceneEntityRecord::new(id("d")),
        ],
    ));
    let r2 = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("x")),
    ]));
    let r_before = r.as_ref().clone();
    let r2_before = r2.as_ref().clone();
    let p = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::root(id("o")),
            under(target(&[("o", "r2")], "x"), target(&[], "k")),
            under(target(&[("k", "r")], "d"), target(&[("o", "r2")], "x")),
            under(target(&[], "b"), target(&[("k", "r")], "d")),
            under(target(&[("k", "r")], "h"), target(&[], "b")),
        ],
        [
            forest("k", "r"),
            forest("o", "r2"),
            SceneEntityRecord::new(id("b")),
        ],
    );
    let p_before = p.clone();
    let sources_before = [
        ("p", Arc::new(p.clone())),
        ("r", Arc::clone(&r)),
        ("r2", Arc::clone(&r2)),
    ];
    let before_routes: &[(&str, &[(&str, &str)])] = &[
        ("k", &[("p", "k")]),
        ("o", &[("p", "o")]),
        ("b", &[("p", "b")]),
        ("h", &[("p", "k"), ("r", "h")]),
        ("d", &[("p", "k"), ("r", "d")]),
        ("x", &[("p", "o"), ("r2", "x")]),
    ];
    let expected = observe(
        &compile(&consumer("p"), &basis, &sources_before),
        before_routes,
    );
    assert_eq!(
        expected,
        BTreeMap::from([
            ("host", vec!["k", "o"]),
            ("k", vec!["x"]),
            ("x", vec!["d"]),
            ("d", vec!["b"]),
            ("b", vec!["h"]),
            ("h", vec![]),
            ("o", vec![]),
        ])
    );

    let split = split(&p, &basis, &sources_before);
    let SplitSourceDocument::Prefab(after) = &split.containing else {
        unreachable!()
    };
    let sources_after = [
        ("p", Arc::new(after.clone())),
        ("q", Arc::new(split.extracted.clone())),
        ("r", Arc::clone(&r)),
        ("r2", Arc::clone(&r2)),
    ];
    // Q must close its own structure before P can apply any retained parent repairs.
    assert_eq!(
        compile(&consumer("q"), &basis, &sources_after)
            .entities()
            .len(),
        5
    );
    let after_routes: &[(&str, &[(&str, &str)])] = &[
        ("k", &[("p", "k")]),
        ("o", &[("p", "o")]),
        ("b", &[("p", "k"), ("q", "b")]),
        ("h", &[("p", "k"), ("q", "k"), ("r", "h")]),
        ("d", &[("p", "k"), ("q", "k"), ("r", "d")]),
        ("x", &[("p", "o"), ("r2", "x")]),
    ];
    assert_eq!(
        observe(
            &compile(&consumer("p"), &basis, &sources_after),
            after_routes
        ),
        expected
    );
    assert_eq!(p, p_before);
    assert_eq!(r.as_ref(), &r_before);
    assert_eq!(r2.as_ref(), &r2_before);
}
