#![cfg(feature = "serde")]

//! Public conversion distinguishes finite reference entry from recursive activation.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistrySnapshot,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringSchemaBasis, PrefabDocument, PrefabEntityOrigin,
    PrefabInstance, PrefabInstanceHop, PrefabReferenceArguments, ResolvedPrefabSource,
    SceneAuthoringSession, SceneCompilationFailureKind, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
};

const OWNER: &str = "test.selected-reference-entry";

#[derive(Component)]
struct Link(EntityReference);

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn input(name: &str) -> ReferenceInputId {
    ReferenceInputId::new(name).unwrap()
}

fn source(name: &str) -> AssetRef {
    AssetRef::path(format!(
        "prefabs/selected-reference-entry/{name}.prefab.json"
    ))
    .unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "cff67102-e9dd-4871-80be-63965f6140a6".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.SelectedReferenceEntryLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        component(),
        "Reference entry link",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueKind::EntityRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)])
}

fn basis() -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    // Match the existing data-Adapter fixture admission; the exercised conversion,
    // source loading, publication and history operations are public session APIs.
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
        &bundle.owners()[0],
        &Default::default(),
    )
    .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn registry(native_calls: &Arc<AtomicUsize>) -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new(OWNER),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    let decodes = Arc::clone(native_calls);
    let encodes = Arc::clone(native_calls);
    builder
        .register_persistent_component_with_codec::<Link, _, _>(
            schema(),
            move |value| {
                decodes.fetch_add(1, Ordering::Relaxed);
                Ok(Link(value.field_entity_reference("target")?.clone()))
            },
            move |value| {
                encodes.fetch_add(1, Ordering::Relaxed);
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

fn reference(route: &[(&str, &str)], target: AuthoredReferenceTarget) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: route
                .iter()
                .map(|(anchor, base)| PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                    expected_base: key(base),
                })
                .collect(),
            target,
        },
    }
}

fn external_leaf() -> EntityReference {
    reference(
        &[("outside", "P"), ("loop", "P")],
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("leaf"))),
    )
}

fn link(target: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(target))]),
    )
    .with_owner(owner())
}

fn entity<'a>(document: &'a SceneDocument, name: &str) -> &'a SceneEntityRecord {
    document
        .entities
        .iter()
        .find(|record| record.id == id(name))
        .unwrap()
}

struct Fixture {
    basis: AuthoringSchemaBasis,
    registry: ComponentRegistrySnapshot,
    native_calls: Arc<AtomicUsize>,
    document: SceneDocument,
    selected: Arc<PrefabDocument>,
    recursive: Arc<PrefabDocument>,
}

impl Fixture {
    fn new(restart_argument_route: bool) -> Self {
        let basis = basis();
        let native_calls = Arc::new(AtomicUsize::new(0));
        let registry = registry(&native_calls);
        let mut selected =
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("s-root"))]);
        selected.inputs.insert(input("x"));
        let mut loop_record = SceneEntityRecord::new(id("loop"));
        loop_record.prefab = Some(PrefabInstance::forest(source("P")));
        let mut recursive = PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf")).with_component(
                component(),
                link(EntityReference::SceneLocal { entity: id("leaf") }),
            ),
            loop_record,
        ]);
        recursive.inputs.insert(input("u"));

        let mut selected_instance = PrefabInstance::forest(source("S"));
        selected_instance.arguments = Some(PrefabReferenceArguments {
            base: key("S"),
            values: [(
                input("x"),
                if restart_argument_route {
                    reference(
                        &[("outside", "P")],
                        AuthoredReferenceTarget::Input(input("u")),
                    )
                } else {
                    external_leaf()
                },
            )]
            .into(),
        });
        let mut anchor = SceneEntityRecord::new(id("A"));
        anchor.prefab = Some(selected_instance);
        let mut outside_instance = PrefabInstance::forest(source("P"));
        if restart_argument_route {
            // Both P entries have zero remaining hops: resolving u restarts with a
            // different complete reference. A remaining-hop count alone collides.
            outside_instance.arguments = Some(PrefabReferenceArguments {
                base: key("P"),
                values: [(input("u"), external_leaf())].into(),
            });
        }
        let mut outside = SceneEntityRecord::new(id("outside"));
        outside.prefab = Some(outside_instance);
        let watcher = SceneEntityRecord::new(id("watcher")).with_component(
            component(),
            link(reference(
                &[("A", "S")],
                AuthoredReferenceTarget::Input(input("x")),
            )),
        );
        let mut unrelated = SceneEntityRecord::new(id("unrelated")).with_component(
            component(),
            link(EntityReference::SceneLocal {
                entity: id("watcher"),
            }),
        );
        unrelated.prefab = Some(PrefabInstance::forest(source("unrelated")));
        Self {
            basis,
            registry,
            native_calls,
            document: SceneDocument::from_ordered_roots([anchor, outside, watcher, unrelated]),
            selected: Arc::new(selected),
            recursive: Arc::new(recursive),
        }
    }

    fn load(&self, request: &AssetRef) -> Result<Option<ResolvedPrefabSource>, Infallible> {
        let (document, canonical) = if request == &source("S") {
            (&self.selected, key("S"))
        } else if request == &source("P") {
            (&self.recursive, key("P"))
        } else {
            panic!("unrelated source must remain dormant: {request:?}");
        };
        Ok(Some(ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(document),
            self.basis.clone(),
            canonical,
        )))
    }

    fn session(&self) -> SceneAuthoringSession {
        SceneAuthoringSession::from_published_authoring_document(
            SceneDocumentCandidate::decode_json_str(&self.document.to_json_string().unwrap())
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/selected-reference-entry.scene.json").unwrap(),
                    ),
                    &self.basis,
                )
                .unwrap()
                .publish(),
        )
    }
}

fn assert_query_only_conversion(fixture: &Fixture) {
    let mut session = fixture.session();
    let before = session.shared_document();
    let before_json = before.to_json_string().unwrap();
    let revision = session.revision();
    let state = session.content_state();
    let history = session.history_status();
    let usage = session.history_usage();
    let selected = fixture.selected.as_ref().clone();
    let recursive = fixture.recursive.as_ref().clone();
    let mut reads = Vec::new();
    let prepared = session
        .request_complete_prefab_conversion(revision, &id("A"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |request, _| {
            reads.push(request.clone());
            fixture.load(request)
        })
        .expect("a finite source query must not fully activate the recursive Prefab");

    assert_eq!(reads, [source("S"), source("P"), source("P")]);
    assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.document(), before.as_ref());
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), state);
    assert_eq!(session.history_status(), history);
    assert_eq!(session.history_usage(), usage);
    assert_eq!(fixture.selected.as_ref(), &selected);
    assert_eq!(fixture.recursive.as_ref(), &recursive);

    assert_eq!(
        prepared.entities().len(),
        1,
        "P contributes no selected entities or values",
    );
    let localized = &prepared.entities()[0];
    assert_eq!(localized.provenance().local_entity_id(), &id("s-root"));
    assert_eq!(
        localized
            .provenance()
            .projection_path()
            .unwrap()
            .root_anchor(),
        &id("A"),
    );
    assert!(
        prepared
            .document()
            .entities
            .iter()
            .find(|record| &record.id == localized.entity())
            .unwrap()
            .components
            .is_empty()
    );
    let reviewed = prepared.document().clone();
    assert_eq!(reviewed.entities.len(), before.entities.len() + 1);
    assert!(entity(&reviewed, "A").prefab.is_none());
    assert_eq!(
        entity(&reviewed, "watcher").components[&component()],
        link(external_leaf()),
    );
    assert_eq!(
        entity(&reviewed, "outside"),
        entity(before.as_ref(), "outside"),
    );
    assert_eq!(
        entity(&reviewed, "unrelated"),
        entity(before.as_ref(), "unrelated"),
    );

    let receipt = session
        .commit_complete_prefab_conversion_with_loader(prepared, |request| fixture.load(request))
        .unwrap();
    assert!(
        receipt.report().applied,
        "{:?}",
        receipt.report().diagnostics
    );
    assert_eq!(session.document(), &reviewed);
    assert_eq!(session.history_status().undo_depth, history.undo_depth + 1);
    assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);
    let undone = session.undo_authoring();
    assert!(undone.applied, "{:?}", undone.diagnostics);
    assert_eq!(session.document(), before.as_ref());
    assert_eq!(session.document().to_json_string().unwrap(), before_json);
    assert_eq!(session.content_state(), state);
    assert_eq!(fixture.selected.as_ref(), &selected);
    assert_eq!(fixture.recursive.as_ref(), &recursive);
    assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn query_only_entry_through_two_recursive_occurrences_localizes_the_external_route() {
    assert_query_only_conversion(&Fixture::new(false));
}

#[test]
fn argument_binding_restart_distinguishes_references_with_equal_remaining_hops() {
    assert_query_only_conversion(&Fixture::new(true));
}

#[test]
fn full_activation_of_the_unchanged_recursive_prefab_still_refuses_a_cycle() {
    let fixture = Fixture::new(false);
    let document =
        SceneDocument::from_ordered_roots([entity(&fixture.document, "outside").clone()]);
    let before = document.clone();
    let recursive = fixture.recursive.as_ref().clone();
    let mut reads = Vec::new();
    let report = document
        .try_compile_with_loader(&fixture.registry, Default::default(), None, |request, _| {
            reads.push(request.clone());
            assert_eq!(request, &source("P"));
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound(
                Arc::clone(&fixture.recursive),
                key("P"),
            )))
        })
        .unwrap();
    assert!(report.candidate.is_none());
    assert!(report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(
        report.failure.as_ref().map(|failure| *failure.reason()),
        Some(SceneCompilationFailureKind::OwnerAdmission),
        "{:?}",
        report.diagnostics,
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-cycle"),
        "{:?}",
        report.diagnostics,
    );
    assert_eq!(reads, [source("P"), source("P")]);
    assert_eq!(document, before);
    assert_eq!(fixture.recursive.as_ref(), &recursive);
    assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn bridge_reentry_distinguishes_lexical_sources_and_rejects_repeated_reference_programs() {
    for cyclic in [false, true] {
        let mut fixture = Fixture::new(false);
        let incoming = reference(
            &[("outside", "P")],
            AuthoredReferenceTarget::Input(input("u")),
        );
        let via_bridge = reference(
            &[("outside", "P"), ("bridge", "B")],
            AuthoredReferenceTarget::Input(input("v")),
        );
        let mut nested_outside = SceneEntityRecord::new(id("outside"));
        let mut nested_instance = PrefabInstance::forest(source("P"));
        nested_instance.arguments = Some(PrefabReferenceArguments {
            base: key("P"),
            values: [(
                input("u"),
                if cyclic {
                    via_bridge.clone()
                } else {
                    EntityReference::SceneLocal { entity: id("leaf") }
                },
            )]
            .into(),
        });
        nested_outside.prefab = Some(nested_instance);
        let mut bridge_anchor = SceneEntityRecord::new(id("bridge"));
        let mut bridge_instance = PrefabInstance::forest(source("B"));
        bridge_instance.arguments = Some(PrefabReferenceArguments {
            base: key("B"),
            // These bytes equal the containing source's x argument. The first P
            // entry and this P-authored re-entry both have zero remaining hops.
            values: [(input("v"), incoming.clone())].into(),
        });
        bridge_anchor.prefab = Some(bridge_instance);
        let leaf = fixture
            .recursive
            .entities
            .iter()
            .find(|record| record.id == id("leaf"))
            .unwrap()
            .clone();
        let mut recursive =
            PrefabDocument::from_ordered_roots([leaf, nested_outside, bridge_anchor]);
        recursive.inputs.insert(input("u"));
        fixture.recursive = Arc::new(recursive);
        let mut bridge = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b-root"))]);
        bridge.inputs.insert(input("v"));
        let bridge = Arc::new(bridge);
        for record in &mut fixture.document.entities {
            if record.id == id("A") {
                record
                    .prefab
                    .as_mut()
                    .unwrap()
                    .arguments
                    .as_mut()
                    .unwrap()
                    .values
                    .insert(input("x"), incoming.clone());
            } else if record.id == id("outside") {
                record.prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
                    base: key("P"),
                    values: [(input("u"), via_bridge.clone())].into(),
                });
            }
        }
        let load = |request: &AssetRef| {
            if request == &source("B") {
                Ok(Some(ResolvedPrefabSource::bound_authoring_source(
                    Arc::clone(&bridge),
                    fixture.basis.clone(),
                    key("B"),
                )))
            } else {
                fixture.load(request)
            }
        };
        let mut session = fixture.session();
        let before = session.shared_document();
        let before_json = before.to_json_string().unwrap();
        let revision = session.revision();
        let state = session.content_state();
        let history = session.history_status();
        let usage = session.history_usage();
        let mut reads = Vec::new();
        let prepared = session
            .request_complete_prefab_conversion(revision, &id("A"), Default::default())
            .unwrap()
            .prepare_with_loader(None, |request, _| {
                reads.push(request.clone());
                load(request)
            });
        assert!(Arc::ptr_eq(&before, &session.shared_document()));
        assert_eq!(session.document(), before.as_ref());
        assert_eq!(session.revision(), revision);
        assert_eq!(session.content_state(), state);
        assert_eq!(session.history_status(), history);
        assert_eq!(session.history_usage(), usage);
        assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);

        if cyclic {
            let Err(nara_scene::PrefabConversionError::Source(report)) = prepared else {
                panic!("repeated P/B reference programs must refuse a cycle");
            };
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-cycle" }),
                "{:?}",
                report.diagnostics
            );
            // Canonical P-authored programs repeat despite distinct occurrence IDs.
            // A ScopeId-based state would miss this cycle and run into the depth cap.
            assert_eq!(
                reads,
                [
                    source("S"),
                    source("P"),
                    source("B"),
                    source("P"),
                    source("B"),
                    source("P"),
                ]
            );
            continue;
        }

        let prepared =
            prepared.expect("identical reference bytes in distinct lexical sources are finite");
        assert_eq!(reads, [source("S"), source("P"), source("B"), source("P")]);
        assert_eq!(prepared.entities().len(), 1);
        let localized = &prepared.entities()[0];
        assert_eq!(localized.provenance().local_entity_id(), &id("s-root"));
        let reviewed = prepared.document().clone();
        assert!(
            reviewed
                .entities
                .iter()
                .find(|record| &record.id == localized.entity())
                .unwrap()
                .components
                .is_empty()
        );
        assert_eq!(reviewed.entities.len(), before.entities.len() + 1);
        assert!(entity(&reviewed, "A").prefab.is_none());
        assert_eq!(
            entity(&reviewed, "watcher").components[&component()],
            link(reference(
                &[("outside", "P")],
                AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("leaf"))),
            ))
        );
        for name in ["outside", "unrelated"] {
            assert_eq!(entity(&reviewed, name), entity(before.as_ref(), name));
        }
        let receipt = session
            .commit_complete_prefab_conversion_with_loader(prepared, load)
            .unwrap();
        assert!(
            receipt.report().applied,
            "{:?}",
            receipt.report().diagnostics
        );
        assert_eq!(session.document(), &reviewed);
        assert_eq!(session.history_status().undo_depth, history.undo_depth + 1);
        let undone = session.undo_authoring();
        assert!(undone.applied, "{:?}", undone.diagnostics);
        assert_eq!(session.document(), before.as_ref());
        assert_eq!(session.document().to_json_string().unwrap(), before_json);
        assert_eq!(session.content_state(), state);
        assert_eq!(fixture.native_calls.load(Ordering::Relaxed), 0);
    }
}
