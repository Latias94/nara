//! Structural membership borrows complete source records without proving their relocation.

use std::{convert::Infallible, io, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::staged::{self, SourceCutMembership, SourceTopology};
use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringSchemaBasis, PrefabAddition, PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, ResolvedPrefabSource,
    SceneCompilationFailureKind, SceneCompilationLimits, SceneCompilationOptions,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    prefab::{
        SceneCompilationContext, SceneCompilationWorkingSet, SourceDocumentInput,
        extraction::{SourceCutReport, try_capture_source_cut_with_loader},
        try_compile_authoring_scene_with_loader,
    },
    register_scene_components,
    schema_authority::SchemaAuthority,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/extraction-cut/{value}.ron")).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "135b9a72-ef49-4e5a-8b2b-77a37a18d686".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.ExtractionCutLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.extraction-cut-links").unwrap()
}

fn local(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(entity) })
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(component(), "Cut link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
            local("socket"),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn record(target: ComponentValue, default: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", target)]),
    )
    .with_owner(owner())
    .with_contextual_default(ComponentFieldId::new("target"), default)
}

fn under(target: PrefabEntityTarget, parent: PrefabEntityTarget) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        target,
        ScenePlacement::last(ScenePlacementParent::Entity(parent)),
    )
}

fn inside(anchor: &str, source: &str, entity: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: key(source),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(entity)),
    }
}

fn root_bound(anchor: &str, source: &str, root: &str) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(anchor));
    entity.prefab = Some(PrefabInstance::root(
        request(source),
        key(source),
        PrefabEntityTarget::source(id(root)),
    ));
    entity
}

fn no_sources(
    source: &AssetRef,
    _: &mut SceneCompilationWorkingSet,
) -> Result<Option<ResolvedPrefabSource>, Infallible> {
    panic!("unexpected Prefab source read: {source:?}")
}

fn assert_success(report: &SourceCutReport<'_>) {
    assert!(report.cut.is_some(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(!report.budget_failed);
    assert!(report.work_bytes.is_some_and(|bytes| bytes > 0));
}

fn ordinary_source() -> PrefabDocument {
    // Deliberately retain raw record order, distinct from both ID order and tree order.
    PrefabDocument {
        inputs: [ReferenceInputId::new("external").unwrap()].into(),
        placements: vec![
            SceneEntityPlacement::root(id("tree")),
            under(
                PrefabEntityTarget::source(id("branch")),
                PrefabEntityTarget::source(id("tree")),
            ),
            under(
                PrefabEntityTarget::source(id("leaf")),
                PrefabEntityTarget::source(id("branch")),
            ),
            under(
                PrefabEntityTarget::source(id("sibling")),
                PrefabEntityTarget::source(id("tree")),
            ),
            SceneEntityPlacement::root(id("other")),
        ],
        entities: vec![
            SceneEntityRecord::new(id("leaf"))
                .with_component(component(), record(local("branch"), local("tree"))),
            SceneEntityRecord::new(id("other")),
            SceneEntityRecord::new(id("branch"))
                .with_component(component(), record(local("leaf"), local("tree"))),
            SceneEntityRecord::new(id("tree")),
            SceneEntityRecord::new(id("sibling")),
        ],
    }
}

#[test]
fn non_root_cut_borrows_unchanged_records_in_exact_raw_input_order() {
    let basis = basis();
    let source = ordinary_source();
    let before = source.clone();
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(&source),
        &id("branch"),
        &basis,
        Default::default(),
        None,
        no_sources,
    )
    .unwrap();
    assert_success(&report);
    let cut = report.cut.as_ref().unwrap();
    let SourceDocumentInput::Prefab(original) = cut.source() else {
        panic!("a Prefab cut retains its original document kind");
    };
    assert!(std::ptr::eq(original, &source));
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    assert_eq!(cut.entities().len(), 2);
    let selected = cut.entities().collect::<Vec<_>>();
    assert!(std::ptr::eq(selected[0], &source.entities[0]));
    assert!(std::ptr::eq(selected[1], &source.entities[2]));
    assert!(std::ptr::eq(
        &selected[1].components[&component()],
        &source.entities[2].components[&component()],
    ));
    assert_eq!(source, before);
}

#[test]
fn mixed_projected_parent_keeps_local_child_and_nested_intent_in_their_physical_records() {
    let basis = basis();
    let q = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q-root")),
            under(
                PrefabEntityTarget::source(id("d1")),
                PrefabEntityTarget::source(id("q-root")),
            ),
            under(
                PrefabEntityTarget::source(id("d2")),
                PrefabEntityTarget::source(id("q-root")),
            ),
        ],
        [
            SceneEntityRecord::new(id("q-root")),
            SceneEntityRecord::new(id("d1")),
            SceneEntityRecord::new(id("d2")),
        ],
    ));
    let leaf = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("leaf-root")),
    ]));
    let mut nested = PrefabAddition::new(id("badge"));
    nested
        .components
        .insert(component(), record(local("d1"), local("d2")));
    nested.prefab = Some(PrefabInstance::forest(request("leaf")));
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.additions.push(nested);
    overlay.placements.push(under(
        PrefabEntityTarget::addition(id("badge")),
        PrefabEntityTarget::source(id("d1")),
    ));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("d2")),
        value: Some(PrefabInstance::forest(request("leaf"))),
    });
    let mut k = root_bound("k", "q", "q-root")
        .with_component(component(), record(local("b"), local("parent")));
    k.prefab.as_mut().unwrap().overrides = Some(overlay);
    let source = PrefabDocument {
        inputs: Default::default(),
        placements: vec![
            SceneEntityPlacement::root(id("parent")),
            SceneEntityPlacement::root(id("outside")),
            under(
                PrefabEntityTarget::source(id("k")),
                PrefabEntityTarget::source(id("parent")),
            ),
            under(PrefabEntityTarget::source(id("b")), inside("k", "q", "d1")),
        ],
        entities: vec![
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record(local("k"), local("parent"))),
            SceneEntityRecord::new(id("outside")),
            SceneEntityRecord::new(id("parent")),
            k,
        ],
    };
    let before = source.clone();
    let q_before = q.as_ref().clone();
    let leaf_before = leaf.as_ref().clone();
    let mut reads = Vec::new();
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(&source),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| {
            reads.push(reference.clone());
            let (document, source_key) = if reference == &request("q") {
                (&q, key("q"))
            } else if reference == &request("leaf") {
                (&leaf, key("leaf"))
            } else {
                panic!("unexpected mixed-parent source read: {reference:?}");
            };
            let resolved = ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                basis.clone(),
                source_key,
            );
            assert!(std::ptr::eq(resolved.document(), document.as_ref()));
            Ok::<_, Infallible>(Some(resolved))
        },
    )
    .unwrap();
    assert_success(&report);
    let cut = report.cut.as_ref().unwrap();
    assert!(std::ptr::eq(cut.root(), &source.entities[3]));
    let SourceDocumentInput::Prefab(original) = cut.source() else {
        panic!("mixed-parent membership retains the actual Prefab input");
    };
    assert!(std::ptr::eq(original, &source));
    let selected = cut.entities().collect::<Vec<_>>();
    assert_eq!(selected.len(), 2);
    assert!(std::ptr::eq(selected[0], &source.entities[0]));
    assert!(std::ptr::eq(selected[1], &source.entities[3]));
    let intent = selected[1]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let original_intent = source.entities[3]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    assert!(std::ptr::eq(intent, original_intent));
    assert_eq!(intent.additions.len(), 1);
    assert_eq!(intent.instances.len(), 1);
    assert_eq!(
        reads.iter().filter(|read| *read == &request("q")).count(),
        1
    );
    assert_eq!(
        reads
            .iter()
            .filter(|read| *read == &request("leaf"))
            .count(),
        2
    );
    assert_eq!(reads.len(), 3);
    assert_eq!(source, before);
    assert_eq!(q.as_ref(), &q_before);
    assert_eq!(leaf.as_ref(), &leaf_before);
}

#[test]
fn chained_root_bindings_normalize_membership_without_including_other_direct_roots() {
    let basis = basis();
    let q = Arc::new(PrefabDocument::from_ordered_roots([root_bound(
        "q-root", "r", "r-root",
    )]));
    let r = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("r-root")),
            under(
                PrefabEntityTarget::source(id("r-child")),
                PrefabEntityTarget::source(id("r-root")),
            ),
        ],
        [
            SceneEntityRecord::new(id("r-root")),
            SceneEntityRecord::new(id("r-child")),
        ],
    ));
    let mut parent = inside("k", "q", "r-child");
    parent.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("q-root")),
        expected_base: key("r"),
    });
    let source = SceneDocument {
        placements: vec![
            SceneEntityPlacement::root(id("outside")),
            SceneEntityPlacement::root(id("k")),
            under(PrefabEntityTarget::source(id("local-child")), parent),
        ],
        entities: vec![
            SceneEntityRecord::new(id("outside")),
            SceneEntityRecord::new(id("local-child")),
            root_bound("k", "q", "q-root"),
        ],
    };
    let before = source.clone();
    let mut reads = Vec::new();
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Scene(&source),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| {
            reads.push(reference.clone());
            let (document, source_key) = if reference == &request("q") {
                (&q, key("q"))
            } else if reference == &request("r") {
                (&r, key("r"))
            } else {
                panic!("unexpected root-binding chain read: {reference:?}");
            };
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                basis.clone(),
                source_key,
            )))
        },
    )
    .unwrap();
    assert_success(&report);
    let cut = report.cut.as_ref().unwrap();
    let SourceDocumentInput::Scene(original) = cut.source() else {
        panic!("a Scene cut retains its original document kind");
    };
    assert!(std::ptr::eq(original, &source));
    assert!(std::ptr::eq(cut.root(), &source.entities[2]));
    let selected = cut.entities().collect::<Vec<_>>();
    assert_eq!(selected.len(), 2);
    assert!(std::ptr::eq(selected[0], &source.entities[1]));
    assert!(std::ptr::eq(selected[1], &source.entities[2]));
    assert_eq!(reads, [request("q"), request("r")]);
    assert_eq!(source, before);
}

#[test]
fn missing_root_refuses_by_borrowed_lookup_before_loading_or_copying_its_id() {
    let basis = basis();
    let source = SceneDocument::from_ordered_roots([root_bound("k", "unavailable", "root")]);
    let before = source.clone();
    let options = SceneCompilationOptions::default().with_limits(
        SceneCompilationLimits::default().with_working_bytes(ByteLimit::new(4096).unwrap()),
    );
    let mut observed = None;
    for missing in [id("absent"), id(&"absent".repeat(16 * 1024))] {
        let report = try_capture_source_cut_with_loader(
            SourceDocumentInput::Scene(&source),
            &missing,
            &basis,
            options,
            None,
            no_sources,
        )
        .unwrap();
        assert!(report.cut.is_none());
        assert_eq!(
            report.failure.as_ref().unwrap().reason(),
            &SceneCompilationFailureKind::MissingTarget,
        );
        assert!(!report.budget_failed);
        assert!(report.diagnostics.has_errors());
        if let Some(bytes) = observed {
            assert_eq!(report.work_bytes, Some(bytes));
            assert!(bytes < missing.as_str().len());
        } else {
            observed = report.work_bytes;
        }
    }
    assert_eq!(source, before);
}

#[test]
fn loader_failure_returns_the_exact_error_retained_by_the_compilation_context() {
    let basis = basis();
    let source = SceneDocument::from_ordered_roots([root_bound("k", "unreadable", "root")]);
    let before = source.clone();
    let error = Box::new(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "cut source denied",
    ));
    let original = std::ptr::from_ref(error.as_ref());
    let mut error = Some(error);
    let mut calls = 0;
    let result = try_capture_source_cut_with_loader(
        SourceDocumentInput::Scene(&source),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| {
            calls += 1;
            assert_eq!(reference, &request("unreadable"));
            Err(error.take().expect("the loader is called only once"))
        },
    );
    let Err(returned) = result else {
        panic!("the original loader failure must escape the structural report");
    };
    assert_eq!(calls, 1);
    assert!(std::ptr::eq(returned.as_ref(), original));
    assert_eq!(returned.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(source, before);
}

#[test]
fn structural_observation_does_not_activate_component_references_or_overridden_sources() {
    let basis = basis();
    let mut dead = SceneEntityRecord::new(id("dead"));
    dead.prefab = Some(PrefabInstance::forest(request("overridden-missing")));
    let mut ghost = SceneEntityRecord::new(id("ghost"));
    ghost.prefab = Some(PrefabInstance::forest(request("reference-only-missing")));
    let q = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("q-root")),
            under(
                PrefabEntityTarget::source(id("dead")),
                PrefabEntityTarget::source(id("q-root")),
            ),
            under(
                PrefabEntityTarget::source(id("ghost")),
                PrefabEntityTarget::source(id("q-root")),
            ),
        ],
        [SceneEntityRecord::new(id("q-root")), dead, ghost],
    ));
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("dead")),
        value: None,
    });
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("ghost")));
    let reference = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("k")),
                    expected_base: key("q"),
                },
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id("ghost")),
                    expected_base: key("reference-only-missing"),
                },
            ],
            target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("socket"))),
        },
    });
    let mut k = root_bound("k", "q", "q-root").with_component(
        component(),
        record(reference, local("hidden-missing-default")),
    );
    k.prefab.as_mut().unwrap().overrides = Some(overlay);
    let source = SceneDocument::from_ordered_roots([k]);
    let before = source.clone();
    let q_before = q.as_ref().clone();
    let mut reads = Vec::new();
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Scene(&source),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| {
            reads.push(reference.clone());
            assert_eq!(reference, &request("q"));
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(&q),
                basis.clone(),
                key("q"),
            )))
        },
    )
    .unwrap();
    assert_success(&report);
    let cut = report.cut.as_ref().unwrap();
    assert_eq!(cut.entities().len(), 1);
    assert!(std::ptr::eq(cut.root(), &source.entities[0]));
    assert_eq!(reads, [request("q")]);

    // The same real compiler interprets this surviving reference only in its value phase.
    let mut value_reads = Vec::new();
    let full = try_compile_authoring_scene_with_loader(
        &source,
        &basis,
        Default::default(),
        None,
        |reference, _| {
            value_reads.push(reference.clone());
            Ok::<_, Infallible>((reference == &request("q")).then(|| {
                ResolvedPrefabSource::bound_authoring_source(
                    Arc::clone(&q),
                    basis.clone(),
                    key("q"),
                )
            }))
        },
    )
    .unwrap();
    assert!(full.candidate.is_none());
    assert!(full.diagnostics.has_errors());
    assert!(value_reads.contains(&request("reference-only-missing")));
    assert!(!value_reads.contains(&request("overridden-missing")));
    assert_eq!(source, before);
    assert_eq!(q.as_ref(), &q_before);
}

#[test]
fn structural_observation_does_not_prepare_values_with_real_native_codecs() {
    let mut definitions = ComponentRegistryBuilder::new();
    register_scene_components(&mut definitions).unwrap();
    let registry = definitions.freeze().unwrap();
    let component = ComponentTypeId::new("nara.scene.Visibility");
    let value = ComponentValue::String("not-a-visibility".into());
    let codec_error = registry
        .preflight_component(&component, &value)
        .err()
        .expect("the production Visibility codec rejects this schema-valid string");
    assert!(codec_error.to_string().contains("Visibility"));
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
        .with_component(
            component,
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
        )]);
    let before = source.clone();
    let mut loader = no_sources;
    let mut context = SceneCompilationContext::new(
        SchemaAuthority::Native(&registry),
        &mut loader,
        SceneCompilationOptions::default().with_limits(
            SceneCompilationLimits::default()
                .with_materialized_components(ItemLimit::ONE)
                .with_materialized_value_nodes(ItemLimit::ONE)
                .with_materialized_value_bytes(ByteLimit::ONE),
        ),
        None,
    );
    let cut = staged::capture_source_cut(
        &mut context,
        SourceTopology {
            placements: &source.placements,
            entities: &source.entities,
            inputs: None,
            reference_queries: &[],
        },
        &id("root"),
    )
    .unwrap();
    assert_eq!(
        cut,
        SourceCutMembership {
            root: 0,
            entities: vec![0],
            targets: [(PrefabEntityTarget::source(id("root")), 0)].into(),
            target_fact_count: 1,
        }
    );
    assert_eq!(context.usage.materialized_components, 0);
    assert_eq!(context.usage.materialized_value_nodes, 0);
    assert_eq!(context.usage.materialized_value_bytes, 0);
    // The borrowed root lookup is charged; no component/default/reference payload was read.
    assert_eq!(context.reference_work, (1, 2 * "root".len()));
    assert!(!context.diagnostics.has_errors());
    assert_eq!(source, before);
}

#[test]
fn missing_root_lookup_is_admitted_before_scanning_the_source() {
    let source = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("first")),
        SceneEntityRecord::new(id("second")),
    ]);
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Scene(&source),
        &id("missing"),
        &basis(),
        SceneCompilationOptions::default().with_limits(
            SceneCompilationLimits::default().with_reference_work_nodes(ItemLimit::ONE),
        ),
        None,
        no_sources,
    )
    .unwrap();
    assert!(report.cut.is_none());
    assert!(report.budget_failed);
    assert_eq!(
        report.failure.unwrap().reason,
        SceneCompilationFailureKind::WorkLimit
    );
    assert!(
        report.diagnostics.iter().any(
            |diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
        )
    );
}

#[test]
fn cut_work_allowance_is_inclusive_at_the_observed_owner_usage() {
    let basis = basis();
    let source = ordinary_source();
    let before = source.clone();
    let observed = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(&source),
        &id("branch"),
        &basis,
        Default::default(),
        None,
        no_sources,
    )
    .unwrap();
    assert_success(&observed);
    let bytes = observed.work_bytes.unwrap();
    assert!(bytes > 1);
    for (allowance, admitted) in [(bytes, true), (bytes - 1, false)] {
        let report = try_capture_source_cut_with_loader(
            SourceDocumentInput::Prefab(&source),
            &id("branch"),
            &basis,
            SceneCompilationOptions::default().with_limits(
                SceneCompilationLimits::default()
                    .with_working_bytes(ByteLimit::new(allowance).unwrap()),
            ),
            None,
            no_sources,
        )
        .unwrap();
        if admitted {
            assert_success(&report);
            assert_eq!(report.work_bytes, Some(bytes));
            assert!(std::ptr::eq(
                report.cut.as_ref().unwrap().root(),
                &source.entities[2]
            ));
        } else {
            assert!(report.cut.is_none());
            assert!(report.budget_failed);
            assert!(report.diagnostics.has_errors());
            assert_eq!(
                report.failure.as_ref().unwrap().reason(),
                &SceneCompilationFailureKind::WorkLimit,
            );
        }
    }
    assert_eq!(source, before);
}
