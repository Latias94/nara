//! Correspondence consumes original-compiler observations of actual constructed P/Q sources.

use std::{collections::BTreeSet, convert::Infallible, sync::Arc};

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt,
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceHop,
    ResolvedPrefabSource, SceneCompilationLimits, SceneDocument, SceneEntityPlacement,
    SceneEntityRecord, ScenePlacement, ScenePlacementParent, SourceDocumentKind,
    prefab::{
        SourceDocumentInput,
        extraction::{
            construction::{SplitSourceDocument, construct_source_split},
            dependants::rewrite_dependant,
            try_capture_source_cut_with_loader,
        },
        try_compile_authoring_scene_with_loader,
    },
};

type Sources<'a> = &'a [(&'a str, &'a str, Arc<PrefabDocument>)];

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e79aecfa-fc50-4fa7-8904-67baf94bcaaf".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    source_key(value, SourceDocumentKind::Prefab)
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/extraction-correspondence/{value}.ron")).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.OccurrenceLinks")
}

fn basis_with(
    owner: &str,
    version: ComponentSchemaVersion,
    with_adapter: bool,
) -> AuthoringSchemaBasis {
    let references = ComponentFieldSchema::required(
        ComponentFieldId::new("references"),
        "References",
        ComponentFieldPath::from_fields(["references"]),
        ComponentValueShape::list(ComponentValueShape::EntityRef),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let score = ComponentFieldSchema::required(
        ComponentFieldId::new("score"),
        "Score",
        ComponentFieldPath::from_fields(["score"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    let schema = ComponentSchema::new(component(), "Occurrence links", version)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([references, score]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(owner).unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapters = if with_adapter {
        vec![
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ]
    } else {
        vec![]
    };
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    basis_with("game.occurrence-links", ComponentSchemaVersion::ONE, true)
}

fn local(value: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(value) }
}

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("cfb34113-32ac-41ae-8573-e54c95cf373f").unwrap(),
        ),
    }
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

fn reference(route: &[(&str, &str)], entity: &str) -> EntityReference {
    let target = target(route, entity);
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: target.through,
            target: AuthoredReferenceTarget::Entity(target.entity),
        },
    }
}

fn record(
    references: impl IntoIterator<Item = EntityReference>,
    score: u64,
) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "references",
                ComponentValue::List(
                    references
                        .into_iter()
                        .map(ComponentValue::EntityReference)
                        .collect(),
                ),
            ),
            ("score", ComponentValue::U64(score)),
        ]),
    )
    .with_owner(PersistentComponentSchemaOwnerId::try_new("game.occurrence-links").unwrap())
}

fn under(entity: &str, parent: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        target(&[], entity),
        ScenePlacement::last(ScenePlacementParent::Entity(target(&[], parent))),
    )
}

fn forest(entity: &str, lookup: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(entity));
    record.prefab = Some(PrefabInstance::forest(request(lookup)));
    record
}

fn original_scene() -> SceneDocument {
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("outside")),
            SceneEntityPlacement::root(id("k")),
            under("b", "k"),
            under("c", "k"),
        ],
        [
            SceneEntityRecord::new(id("outside")),
            SceneEntityRecord::new(id("k")).with_component(
                component(),
                record([local("b"), local("outside"), persistent()], 1),
            ),
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record([local("k"), local("c")], 2)),
            SceneEntityRecord::new(id("c")),
        ],
    )
}

fn load(
    reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    sources: Sources<'_>,
) -> Option<ResolvedPrefabSource> {
    let (_, canonical, document) = sources
        .iter()
        .find(|(lookup, _, _)| request(lookup) == *reference)?;
    Some(ResolvedPrefabSource::bound_authoring_source(
        Arc::clone(document),
        basis.clone(),
        key(canonical),
    ))
}

fn capture<'a>(
    source: SourceDocumentInput<'a>,
    basis: &AuthoringSchemaBasis,
    sources: Sources<'_>,
) -> (SourceCut<'a>, Option<usize>) {
    let report = try_capture_source_cut_with_loader(
        source,
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
    (report.cut.unwrap(), report.work_bytes)
}

fn compile(
    scene: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    sources: Sources<'_>,
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

fn occurrence<'a>(
    document: &'a ExpandedSceneDocument,
    anchor: &str,
    route: &[(&str, &str)],
) -> &'a SceneEntityId {
    let mut matches = document.provenance().iter().filter(|(_, origin)| {
        origin.projection_path().is_some_and(|path| {
            path.root_anchor() == &id(anchor)
                && path.steps().len() == route.len()
                && path.steps().iter().zip(route).all(|(step, (base, entity))| {
                    matches!(step, PrefabProjectionStep::SourceEntity { canonical_source, entity: actual, .. }
                        if canonical_source == &key(base) && actual == &id(entity))
                })
        })
    });
    let entity = matches.next().expect("actual compiler occurrence").0;
    assert!(matches.next().is_none());
    entity
}

fn mapped<'a>(pairs: &'a [OccurrenceCorrespondence], old: &SceneEntityId) -> &'a SceneEntityId {
    pairs.iter().find(|pair| pair.old() == old).unwrap().new()
}

#[test]
fn scene_cut_remaps_actual_growing_ids_and_keeps_persistent_references() {
    let (basis, source) = (basis(), original_scene());
    let original = source.clone();
    let (pk, qk) = (source_key("scene", SourceDocumentKind::Scene), key("q"));
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &basis, &[]);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Scene(retained) = split.containing else {
        panic!("actual Scene source")
    };
    let q = Arc::new(split.extracted);
    let before = compile(&source, &basis, &[]);
    let after = compile(&retained, &basis, &[("q", "q", Arc::clone(&q))]);
    let pairs = verify_occurrences(
        &cut,
        &pk,
        &qk,
        &request("q"),
        &pk,
        &before,
        &after,
        &basis,
        &mut work,
    )
    .unwrap();
    assert_eq!(pairs.len(), source.entities.len());
    assert_eq!(mapped(&pairs, &id("k")), &id("k"));
    assert_eq!(mapped(&pairs, &id("outside")), &id("outside"));
    let b = occurrence(&after, "k", &[("q", "b")]);
    assert_eq!(mapped(&pairs, &id("b")), b);
    assert!(b.as_str().len() > "b".len());
    let root = after
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("k"))
        .unwrap();
    let value = &root.components()[&component()].value;
    assert_eq!(
        value.get("references"),
        Some(&ComponentValue::List(vec![
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: b.clone() }),
            ComponentValue::EntityReference(local("outside")),
            ComponentValue::EntityReference(persistent()),
        ]))
    );
    assert_eq!(root.children()[0], *b);
    assert_eq!(source, original);
}

#[test]
fn repeated_nested_occurrences_match_actual_alias_addresses_and_rewritten_dependants() {
    let basis = basis();
    let r = Arc::new(PrefabDocument::new(
        [SceneEntityPlacement::root(id("r")), under("leaf", "r")],
        [
            SceneEntityRecord::new(id("r")),
            SceneEntityRecord::new(id("leaf"))
                .with_component(component(), record([local("r"), persistent()], 7)),
        ],
    ));
    let mut root = SceneEntityRecord::new(id("k")).with_component(
        component(),
        record([local("b"), reference(&[("k", "r")], "leaf")], 1),
    );
    root.prefab = Some(PrefabInstance::root(
        request("r"),
        key("r"),
        target(&[], "r"),
    ));
    let p = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("x")),
            SceneEntityPlacement::root(id("k")),
            under("b", "k"),
        ],
        [
            SceneEntityRecord::new(id("x")),
            root,
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record([local("k"), local("x")], 2)),
        ],
    ));
    let t = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("top")),
            under("slot", "top"),
            under("watch", "top"),
        ],
        [
            SceneEntityRecord::new(id("top")),
            forest("slot", "p-alias"),
            SceneEntityRecord::new(id("watch"))
                .with_component(component(), record([reference(&[("slot", "p")], "b")], 3)),
        ],
    ));
    let scene = SceneDocument::from_ordered_roots([
        forest("h1", "p"),
        forest("h2", "p-alias"),
        forest("nested", "t"),
        SceneEntityRecord::new(id("observer")).with_component(
            component(),
            record([reference(&[("nested", "t"), ("slot", "p")], "b")], 4),
        ),
    ]);
    let (p_original, t_original, scene_original) =
        (p.as_ref().clone(), t.as_ref().clone(), scene.clone());
    let before_sources = [
        ("p", "p", Arc::clone(&p)),
        ("p-alias", "p", Arc::clone(&p)),
        ("r", "r", Arc::clone(&r)),
        ("t", "t", Arc::clone(&t)),
    ];
    let (cut, captured) = capture(SourceDocumentInput::Prefab(&p), &basis, &before_sources);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let (pk, qk, tk) = (key("p"), key("q"), key("t"));
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Prefab(mut retained) = split.containing else {
        panic!("actual Prefab source")
    };
    // Recompile an equivalent actual declaration whose lookup differs from the proposal. The
    // canonical address comparison must ignore this spelling while retaining the Q source key.
    retained
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("k"))
        .unwrap()
        .prefab
        .as_mut()
        .unwrap()
        .source = request("q-alias");
    let relocation = BranchRelocation::new(&cut, &pk, &qk).unwrap();
    let SplitSourceDocument::Prefab(rewritten_t) = rewrite_dependant(
        SourceDocumentInput::Prefab(&t),
        &tk,
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("actual dependant Prefab")
    };
    let containing = source_key("consumer", SourceDocumentKind::Scene);
    let SplitSourceDocument::Scene(rewritten_scene) = rewrite_dependant(
        SourceDocumentInput::Scene(&scene),
        &containing,
        &relocation,
        &basis,
        false,
        &mut work,
    )
    .unwrap()
    .document
    else {
        panic!("actual dependant Scene")
    };
    let retained = Arc::new(retained);
    let q = Arc::new(split.extracted);
    let after_sources = [
        ("p", "p", Arc::clone(&retained)),
        ("p-alias", "p", Arc::clone(&retained)),
        ("q-alias", "q", Arc::clone(&q)),
        ("r", "r", Arc::clone(&r)),
        ("t", "t", Arc::new(rewritten_t)),
    ];
    let before = compile(&scene, &basis, &before_sources);
    let after = compile(&rewritten_scene, &basis, &after_sources);
    let pairs = verify_occurrences(
        &cut,
        &pk,
        &qk,
        &request("q"),
        &containing,
        &before,
        &after,
        &basis,
        &mut work,
    )
    .unwrap();
    assert_eq!(pairs.len(), before.entities().len());
    assert_eq!(pairs.len(), after.entities().len());
    assert_eq!(
        pairs
            .iter()
            .map(OccurrenceCorrespondence::new)
            .collect::<BTreeSet<_>>()
            .len(),
        pairs.len()
    );
    for anchor in ["h1", "h2"] {
        assert_eq!(
            mapped(&pairs, occurrence(&before, anchor, &[("p", "b")])),
            occurrence(&after, anchor, &[("p", "k"), ("q", "b")])
        );
        assert_eq!(
            mapped(
                &pairs,
                occurrence(&before, anchor, &[("p", "k"), ("r", "leaf")])
            ),
            occurrence(&after, anchor, &[("p", "k"), ("q", "k"), ("r", "leaf")])
        );
    }
    assert_eq!(
        mapped(
            &pairs,
            occurrence(&before, "nested", &[("t", "slot"), ("p", "b")])
        ),
        occurrence(&after, "nested", &[("t", "slot"), ("p", "k"), ("q", "b")])
    );
    assert_eq!(p.as_ref(), &p_original);
    assert_eq!(t.as_ref(), &t_original);
    assert_eq!(scene, scene_original);
}

#[test]
fn independently_recompiled_wrong_actual_sources_refuse_hierarchy_values_and_wrappers() {
    let (basis, source) = (basis(), original_scene());
    let original = source.clone();
    let (pk, qk) = (source_key("scene", SourceDocumentKind::Scene), key("q"));
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &basis, &[]);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Scene(retained) = split.containing else {
        panic!("Scene")
    };
    let before = compile(&source, &basis, &[]);
    let original_observation = format!("{before:?}");
    for change in [
        "children",
        "roots",
        "scalar",
        "reference",
        "components",
        "wrapper",
    ] {
        let mut wrong_scene = retained.clone();
        let mut wrong_q = split.extracted.clone();
        match change {
            "children" => wrong_q.placements.swap(1, 2),
            "roots" => wrong_scene.placements.swap(0, 1),
            "scalar" | "reference" => {
                let value = &mut wrong_q
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == id("k"))
                    .unwrap()
                    .components
                    .get_mut(&component())
                    .unwrap()
                    .value;
                let ComponentValue::Map(fields) = value else {
                    panic!("component map")
                };
                if change == "scalar" {
                    fields.insert("score".into(), ComponentValue::U64(99));
                } else {
                    let ComponentValue::List(references) = fields.get_mut("references").unwrap()
                    else {
                        panic!("reference list")
                    };
                    // Preserve the actual generated Q input for the outward reference.
                    references[0] = ComponentValue::EntityReference(local("c"));
                }
            }
            "components" => wrong_q
                .entities
                .iter_mut()
                .find(|entity| entity.id == id("k"))
                .unwrap()
                .components
                .clear(),
            "wrapper" => {
                wrong_scene
                    .entities
                    .push(SceneEntityRecord::new(id("extra")));
                wrong_scene
                    .placements
                    .push(SceneEntityPlacement::root(id("extra")));
            }
            _ => unreachable!(),
        }
        let wrong = compile(&wrong_scene, &basis, &[("q", "q", Arc::new(wrong_q))]);
        assert!(
            matches!(
                verify_occurrences(
                    &cut,
                    &pk,
                    &qk,
                    &request("q"),
                    &pk,
                    &before,
                    &wrong,
                    &basis,
                    &mut work
                ),
                Err(ExtractionError::InvalidSource)
            ),
            "{change}"
        );
    }
    assert_eq!(source, original);
    assert_eq!(format!("{before:?}"), original_observation);
}

#[test]
fn current_schema_adapter_owner_and_version_are_required_for_surviving_records() {
    let (basis, source) = (basis(), original_scene());
    let (pk, qk) = (source_key("scene", SourceDocumentKind::Scene), key("q"));
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &basis, &[]);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Scene(retained) = split.containing else {
        panic!("Scene")
    };
    let before = compile(&source, &basis, &[]);
    let after = compile(&retained, &basis, &[("q", "q", Arc::new(split.extracted))]);
    let unknown =
        AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let no_adapter = basis_with("game.occurrence-links", ComponentSchemaVersion::ONE, false);
    let new_version = basis_with("game.occurrence-links", ComponentSchemaVersion(2), true);
    let new_owner = basis_with("game.other-owner", ComponentSchemaVersion::ONE, true);
    for (current, expected) in [
        (&unknown, "unknown"),
        (&no_adapter, "adapter"),
        (&new_version, "version"),
        (&new_owner, "owner"),
    ] {
        let result = verify_occurrences(
            &cut,
            &pk,
            &qk,
            &request("q"),
            &pk,
            &before,
            &after,
            current,
            &mut work,
        );
        assert!(
            match expected {
                "unknown" => matches!(result, Err(ExtractionError::UnknownSchema)),
                "adapter" => matches!(result, Err(ExtractionError::AdapterUnavailable)),
                "version" => matches!(result, Err(ExtractionError::VersionMismatch)),
                "owner" => matches!(result, Err(ExtractionError::OwnerMismatch)),
                _ => unreachable!(),
            },
            "{expected}"
        );
    }
}

#[test]
fn cumulative_construction_and_verification_budget_is_inclusive_and_leaves_inputs_unchanged() {
    let (basis, source) = (basis(), original_scene());
    let original = source.clone();
    let (pk, qk) = (source_key("scene", SourceDocumentKind::Scene), key("q"));
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &basis, &[]);
    let defaults = SceneCompilationLimits::default();
    let mut setup = ExtractionWork::from_cut(defaults, &cut, captured).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut setup).unwrap();
    let SplitSourceDocument::Scene(retained) = split.containing else {
        panic!("Scene")
    };
    let before = compile(&source, &basis, &[]);
    let after = compile(&retained, &basis, &[("q", "q", Arc::new(split.extracted))]);
    let snapshots = (format!("{before:?}"), format!("{after:?}"));
    let execute = |limits| -> Result<ExtractionWork, ExtractionError> {
        let mut work = ExtractionWork::from_cut(limits, &cut, captured)?;
        construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work)?;
        verify_occurrences(
            &cut,
            &pk,
            &qk,
            &request("q"),
            &pk,
            &before,
            &after,
            &basis,
            &mut work,
        )?;
        Ok(work)
    };
    let baseline = execute(defaults).unwrap();
    let (nodes, bytes) = baseline.usage();
    let reference_bytes = defaults.reference_work_bytes().get()
        - baseline
            .compiler_options(32)
            .unwrap()
            .limits
            .reference_work_bytes()
            .get();
    assert!(bytes > captured.unwrap());
    assert!(nodes > 1 && reference_bytes > 1);
    let exact = defaults
        .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
        .with_reference_work_bytes(ByteLimit::new(reference_bytes).unwrap())
        .with_working_bytes(ByteLimit::new(bytes).unwrap());
    let mut exact_work = execute(exact).unwrap();
    assert_eq!(exact_work.usage(), (nodes, bytes));
    assert!(matches!(
        verify_occurrences(
            &cut,
            &pk,
            &qk,
            &request("q"),
            &pk,
            &before,
            &after,
            &basis,
            &mut exact_work
        ),
        Err(ExtractionError::Capacity)
    ));
    assert_eq!(exact_work.usage(), (nodes, bytes));
    for short in [
        exact.with_reference_work_nodes(ItemLimit::new(nodes - 1).unwrap()),
        exact.with_reference_work_bytes(ByteLimit::new(reference_bytes - 1).unwrap()),
        exact.with_working_bytes(ByteLimit::new(bytes - 1).unwrap()),
    ] {
        assert!(matches!(execute(short), Err(ExtractionError::Capacity)));
    }
    assert_eq!(source, original);
    assert_eq!((format!("{before:?}"), format!("{after:?}")), snapshots);
}

#[test]
fn a_large_scalar_payload_does_not_pay_for_nonexistent_reference_path_rewrites() {
    let owner = PersistentComponentSchemaOwnerId::try_new("game.scalar-correspondence").unwrap();
    let schema = ComponentSchema::new(component(), "Scalar payload", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("numbers"),
            "Numbers",
            ComponentFieldPath::from_fields(["numbers"]),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let value = ComponentValue::map([(
        "numbers",
        ComponentValue::List((0..1024).map(ComponentValue::U64).collect()),
    )]);
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("k"))
        .with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner),
        )]);
    let (pk, qk) = (
        source_key("scalar-scene", SourceDocumentKind::Scene),
        key("q"),
    );
    let (cut, captured) = capture(SourceDocumentInput::Scene(&source), &basis, &[]);
    let mut work = ExtractionWork::from_cut(Default::default(), &cut, captured).unwrap();
    let split = construct_source_split(&cut, &pk, &qk, &request("q"), &basis, &mut work).unwrap();
    let SplitSourceDocument::Scene(retained) = split.containing else {
        panic!("Scene")
    };
    let before = compile(&source, &basis, &[]);
    let after = compile(&retained, &basis, &[("q", "q", Arc::new(split.extracted))]);
    let pairs = verify_occurrences(
        &cut,
        &pk,
        &qk,
        &request("q"),
        &pk,
        &before,
        &after,
        &basis,
        &mut work,
    )
    .unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].old(), &id("k"));
    assert_eq!(pairs[0].new(), &id("k"));
}
