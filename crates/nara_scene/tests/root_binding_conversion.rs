#![cfg(feature = "serde")]

//! Public source conversion, exact history, persistence, strict compilation and Native spawn.

use std::{collections::HashMap, convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::{Component, Entity, World};
use nara_hierarchy::{Children, Parent};
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityLookup, EntityReference,
    ReferenceInputId, SpawnedSceneInstance,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistrySnapshot,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringSchemaBasis, CompiledSceneDocument,
    InMemoryPrefabSourceResolver, LocalizedPrefabEntity, PrefabAuthoringSession,
    PrefabConversionError, PrefabDocument, PrefabDocumentCandidate, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceHop, PrefabReferenceArguments,
    ResolvedPrefabSource, SceneAuthoringSession, SceneCompilationLimits, SceneCompilationOptions,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind, spawn_scene,
};

const OWNER: &str = "test.root-binding-conversion";

#[derive(Debug, PartialEq, Component)]
struct Link(EntityReference);

#[derive(Debug, PartialEq, Component)]
struct Hidden(ComponentValue);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn ty() -> ComponentTypeId {
    ComponentTypeId::new("test.RootConversionLink")
}
fn hidden_ty() -> ComponentTypeId {
    ComponentTypeId::new("test.RootConversionHidden")
}
fn field() -> ComponentFieldId {
    ComponentFieldId::new("target")
}
fn input() -> ReferenceInputId {
    ReferenceInputId::new("u").unwrap()
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()
}
fn source(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/root-conversion/{name}.prefab.json")).unwrap()
}
fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "55b6bcf2-d244-4d84-99c7-f8c2445cec90".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}
fn prefab_key(name: &str) -> SourceDocumentKey {
    key(name, SourceDocumentKind::Prefab)
}
fn local(name: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(name) }
}
fn route(anchor: &str, base: &str, endpoint: AuthoredReferenceTarget) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: prefab_key(base),
            }],
            target: endpoint,
        },
    }
}
fn selected_root() -> EntityReference {
    route(
        "anchor",
        "q",
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("root"))),
    )
}
fn selected_input() -> EntityReference {
    route("anchor", "q", AuthoredReferenceTarget::Input(input()))
}
fn external() -> EntityReference {
    route(
        "outside",
        "r",
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("external-child"))),
    )
}
fn link(reference: EntityReference) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("target", ComponentValue::EntityReference(reference))]),
    )
    .with_owner(owner())
}
fn child(parent: &str, entity: &str) -> SceneEntityPlacement {
    SceneEntityPlacement::new(
        PrefabEntityTarget::source(id(entity)),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id(parent),
        ))),
    )
}

fn schemas() -> [ComponentSchema; 2] {
    [
        ComponentSchema::new(ty(), "Link", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                field(),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]),
        ComponentSchema::new(hidden_ty(), "Hidden default", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::optional_with_default(
                field(),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
                ComponentValue::EntityReference(local("root")),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]),
    ]
}

fn basis() -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: schemas().into(),
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    // Existing data-Adapter admission only; all conversion and materialization below use
    // public source/session APIs. No compiler fragment or history authority is fabricated.
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
        &bundle.owners()[0],
        &Default::default(),
    )
    .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new(OWNER),
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    let [link_schema, hidden_schema] = schemas();
    builder
        .register_persistent_component_with_codec::<Link, _, _>(
            link_schema,
            |value| Ok(Link(value.field_entity_reference("target")?.clone())),
            |value| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    builder
        .register_persistent_component_with_codec::<Hidden, _, _>(
            hidden_schema,
            |value| Ok(Hidden(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    builder.freeze().unwrap()
}

struct Fixture {
    document: SceneDocument,
    sources: HashMap<AssetRef, (SourceDocumentKey, Arc<PrefabDocument>)>,
}

impl Fixture {
    fn new(outside_input: bool) -> Self {
        let mut q = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("root")),
                child("root", "d1"),
                child("root", "d2"),
            ],
            [
                SceneEntityRecord::new(id("root")).with_component(
                    ty(),
                    link(EntityReference::Authored {
                        reference: AuthoredEntityReference::input(input()),
                    }),
                ),
                SceneEntityRecord::new(id("d1")).with_component(ty(), link(local("root"))),
                SceneEntityRecord::new(id("d2")),
            ],
        );
        q.inputs.insert(input());
        let mut r = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("external-root")),
                child("external-root", "external-child"),
            ],
            [
                SceneEntityRecord::new(id("external-root")),
                SceneEntityRecord::new(id("external-child")),
            ],
        );
        let unused = ReferenceInputId::new("unused").unwrap();
        r.inputs.insert(unused.clone());
        let mut declaration = PrefabInstance::root(source("q"), prefab_key("q"), id("root"));
        declaration.arguments = Some(PrefabReferenceArguments {
            base: prefab_key("q"),
            values: [(
                input(),
                if outside_input {
                    external()
                } else {
                    local("b")
                },
            )]
            .into(),
        });
        let mut anchor = SceneEntityRecord::new(id("anchor")).with_component(
            hidden_ty(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("target", ComponentValue::Null)]),
            )
            .with_owner(owner())
            .with_contextual_default(field(), ComponentValue::EntityReference(selected_root())),
        );
        anchor.prefab = Some(declaration);
        let mut outside = SceneEntityRecord::new(id("outside"));
        let mut other = PrefabInstance::root(source("r"), prefab_key("r"), id("external-root"));
        other.arguments = Some(PrefabReferenceArguments {
            base: prefab_key("r"),
            values: [(unused, selected_input())].into(),
        });
        outside.prefab = Some(other);
        let d2 = PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("anchor")),
                expected_base: prefab_key("q"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("d2")),
        };
        Self {
            document: SceneDocument::new(
                [
                    SceneEntityPlacement::root(id("anchor")),
                    SceneEntityPlacement::root(id("outside")),
                    SceneEntityPlacement::root(id("watcher")),
                    SceneEntityPlacement::new(
                        PrefabEntityTarget::source(id("b")),
                        ScenePlacement::before(
                            ScenePlacementParent::Entity(PrefabEntityTarget::source(id("anchor"))),
                            d2,
                        ),
                    ),
                ],
                [
                    anchor,
                    outside,
                    SceneEntityRecord::new(id("b")).with_component(ty(), link(selected_root())),
                    SceneEntityRecord::new(id("watcher"))
                        .with_component(ty(), link(selected_input())),
                ],
            ),
            sources: [
                (source("q"), (prefab_key("q"), Arc::new(q))),
                (source("r"), (prefab_key("r"), Arc::new(r))),
            ]
            .into(),
        }
    }

    fn load(
        &self,
        reference: &AssetRef,
        basis: &AuthoringSchemaBasis,
    ) -> Result<Option<ResolvedPrefabSource>, Infallible> {
        Ok(self.sources.get(reference).map(|(key, document)| {
            ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(document),
                basis.clone(),
                key.clone(),
            )
        }))
    }

    fn resolver(&self) -> InMemoryPrefabSourceResolver {
        let mut resolver = InMemoryPrefabSourceResolver::new();
        for (reference, (key, document)) in &self.sources {
            resolver.insert(reference.clone(), key.clone(), document.as_ref().clone());
        }
        resolver
    }
}

fn scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/root-conversion.scene.json").unwrap(),
                ),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

fn prefab(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.placements.clone(), document.entities.clone());
    PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(source("container-prefab")),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

fn mapped(mappings: &[LocalizedPrefabEntity], local: &str) -> SceneEntityId {
    mappings
        .iter()
        .find(|mapping| {
            mapping.provenance().local_entity_id() == &id(local)
                && mapping.provenance().projection_path().is_some()
        })
        .unwrap()
        .entity()
        .clone()
}

fn resolve(world: &World, instance: &SpawnedSceneInstance, id: &SceneEntityId) -> Entity {
    match instance.resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("{other:?}"),
    }
}

fn compile_and_spawn(
    document: CompiledSceneDocument,
    registry: &ComponentRegistrySnapshot,
    kind: SourceDocumentKind,
    d1: &SceneEntityId,
    d2: &SceneEntityId,
    outside_input: bool,
    reset: bool,
) {
    let anchor = document
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("anchor"))
        .unwrap();
    assert_eq!(anchor.children(), [d1.clone(), id("b"), d2.clone()]);
    assert_eq!(
        document.entities().len(),
        7,
        "both bound roots merge into their existing anchors"
    );
    let external_id = document
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| {
            origin.local_entity_id() == &id("external-child")
                && origin
                    .projection_path()
                    .is_some_and(|path| path.root_anchor() == &id("outside"))
        })
        .unwrap()
        .0;
    let expected = EntityReference::SceneLocal {
        entity: if outside_input {
            external_id.clone()
        } else {
            id("b")
        },
    };
    assert_eq!(
        anchor.components()[&ty()]
            .value()
            .field_entity_reference("target")
            .unwrap(),
        &expected
    );
    let watcher = document
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("watcher"))
        .unwrap();
    assert_eq!(
        watcher.components()[&ty()]
            .value()
            .field_entity_reference("target")
            .unwrap(),
        &expected
    );
    assert!(
        document.canonical_provenance()[&id("anchor")]
            .projection_path()
            .is_none()
    );
    let candidate = document.bind_source(key("converted", kind));
    let mut world = World::new();
    let report = spawn_scene(
        &mut world,
        registry,
        &candidate,
        &SceneOccurrenceKey::new("conversion", "qualification").unwrap(),
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let instance = report
        .instance
        .expect("the complete converted source spawns");
    assert_eq!(instance.len(), 7);
    let root = resolve(&world, &instance, &id("anchor"));
    let expected_children =
        [d1.clone(), id("b"), d2.clone()].map(|entity| resolve(&world, &instance, &entity));
    assert_eq!(
        world.get::<Children>(root).unwrap().as_slice(),
        expected_children
    );
    for child in expected_children {
        assert_eq!(world.get::<Parent>(child).unwrap().parent(), root);
    }
    assert_eq!(
        world
            .iter_entities()
            .filter(|entity| entity.contains::<SceneEntitySource>())
            .count(),
        7
    );
    assert_eq!(world.get::<Link>(root), Some(&Link(expected)));
    let hidden = &world.get::<Hidden>(root).unwrap().0;
    if reset {
        assert_eq!(
            hidden.field_entity_reference("target").unwrap(),
            &local("anchor")
        );
    } else {
        assert_eq!(hidden.get("target"), Some(&ComponentValue::Null));
    }
}

macro_rules! qualify {
    ($name:ident, $open:ident, $candidate:ty, $kind:ident) => {
        #[test]
        fn $name() {
            for outside_input in [false, true] {
                let fixture = Fixture::new(outside_input);
                let basis = basis();
                let registry = registry();
                let mut session = $open(&fixture.document, &basis);
                let before = session.shared_document();
                let state = session.content_state();
                let initial = session.document().compile(&registry, &fixture.resolver());
                assert!(
                    !initial.diagnostics.has_errors(),
                    "{:?}",
                    initial.diagnostics
                );
                let initial = initial.candidate.unwrap();
                let projected = |local: &str| {
                    initial
                        .canonical_provenance()
                        .iter()
                        .find(|(_, origin)| {
                            origin.local_entity_id() == &id(local)
                                && origin
                                    .projection_path()
                                    .is_some_and(|path| path.root_anchor() == &id("anchor"))
                        })
                        .unwrap()
                        .0
                        .clone()
                };
                let initial_d1 = projected("d1");
                let initial_d2 = projected("d2");
                compile_and_spawn(
                    initial,
                    &registry,
                    SourceDocumentKind::$kind,
                    &initial_d1,
                    &initial_d2,
                    outside_input,
                    false,
                );
                let prepared = session
                    .request_complete_prefab_conversion(
                        session.revision(),
                        &id("anchor"),
                        Default::default(),
                    )
                    .unwrap()
                    .prepare_with_loader(None, |reference, _| fixture.load(reference, &basis))
                    .unwrap();
                assert!(prepared.checked_retained_bytes().is_some());
                assert_eq!(prepared.entities().len(), 3);
                assert_eq!(
                    prepared
                        .entities()
                        .iter()
                        .filter(|entity| entity.entity() == &id("anchor"))
                        .count(),
                    1
                );
                let d1 = mapped(prepared.entities(), "d1");
                let d2 = mapped(prepared.entities(), "d2");
                let reviewed = prepared.document().clone();
                assert_eq!(reviewed.entities.len(), before.entities.len() + 2);
                assert!(
                    reviewed
                        .entities
                        .iter()
                        .find(|entity| entity.id == id("anchor"))
                        .unwrap()
                        .prefab
                        .is_none()
                );
                assert_eq!(
                    reviewed
                        .entities
                        .iter()
                        .find(|entity| entity.id == id("b"))
                        .unwrap()
                        .components[&ty()]
                        .value,
                    link(local("anchor")).value
                );
                let hidden = &reviewed
                    .entities
                    .iter()
                    .find(|entity| entity.id == id("anchor"))
                    .unwrap()
                    .components[&hidden_ty()];
                assert_eq!(hidden.value.get("target"), Some(&ComponentValue::Null));
                assert_eq!(
                    hidden.contextual_defaults[&field()],
                    ComponentValue::EntityReference(local("anchor"))
                );
                let unused = &reviewed
                    .entities
                    .iter()
                    .find(|entity| entity.id == id("outside"))
                    .unwrap()
                    .prefab
                    .as_ref()
                    .unwrap()
                    .arguments
                    .as_ref()
                    .unwrap()
                    .values;
                assert_eq!(
                    unused.values().next(),
                    Some(&if outside_input {
                        external()
                    } else {
                        local("b")
                    })
                );
                if outside_input {
                    for name in ["anchor", "watcher"] {
                        assert_eq!(
                            reviewed
                                .entities
                                .iter()
                                .find(|entity| entity.id == id(name))
                                .unwrap()
                                .components[&ty()]
                                .value,
                            link(external()).value
                        );
                    }
                }
                let receipt = session
                    .commit_complete_prefab_conversion_with_loader(prepared, |reference| {
                        fixture.load(reference, &basis)
                    })
                    .unwrap();
                assert!(
                    receipt.report().applied,
                    "{:?}",
                    receipt.report().diagnostics
                );
                assert_eq!(session.document(), &reviewed);
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), before.as_ref());
                assert_eq!(session.content_state(), state);
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &reviewed);
                for candidate in [
                    <$candidate>::decode_json_str(&reviewed.to_json_string().unwrap()).unwrap(),
                    <$candidate>::decode_ron_str(&reviewed.to_ron_string().unwrap()).unwrap(),
                ] {
                    let mut reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                    assert_eq!(reopened.document(), &reviewed);
                    assert_eq!(reopened.authoring_schema_basis().unwrap(), basis);
                    let report = reopened.document().compile(&registry, &fixture.resolver());
                    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                    compile_and_spawn(
                        report.candidate.unwrap(),
                        &registry,
                        SourceDocumentKind::$kind,
                        &d1,
                        &d2,
                        outside_input,
                        false,
                    );
                    let reset = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
                        entity: id("anchor"),
                        component: hidden_ty(),
                        component_version: ComponentSchemaVersion::ONE,
                        field: field(),
                    }]);
                    assert!(reopened.apply_authoring_patch(&reset).applied);
                    let report = reopened.document().compile(&registry, &fixture.resolver());
                    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
                    compile_and_spawn(
                        report.candidate.unwrap(),
                        &registry,
                        SourceDocumentKind::$kind,
                        &d1,
                        &d2,
                        outside_input,
                        true,
                    );
                    assert!(reopened.undo_authoring().applied);
                    assert_eq!(reopened.document(), &reviewed);
                    assert!(reopened.redo_authoring().applied);
                }
            }
        }
    };
}

qualify!(
    scene_conversion_preserves_root_input_defaults_history_and_native_result,
    scene,
    SceneDocumentCandidate,
    Scene
);
qualify!(
    prefab_conversion_preserves_root_input_defaults_history_and_native_result,
    prefab,
    PrefabDocumentCandidate,
    Prefab
);

#[test]
fn conversion_refuses_changed_dependency_stale_revision_and_insufficient_work_before_publication() {
    let fixture = Fixture::new(true);
    let basis = basis();
    let mut session = scene(&fixture.document, &basis);
    let before = session.shared_document();
    let revision = session.revision();
    let mut constrained = session
        .request_complete_prefab_conversion(revision, &id("anchor"), Default::default())
        .unwrap();
    let retained = constrained.checked_retained_bytes().unwrap();
    constrained.restrict_working_bytes(ByteLimit::new(1).unwrap());
    assert_eq!(constrained.checked_retained_bytes(), Some(retained));
    let mut reads = 0;
    assert!(
        constrained
            .prepare_with_loader(None, |reference, _| {
                reads += 1;
                fixture.load(reference, &basis)
            })
            .is_err()
    );
    assert_eq!(reads, 0, "input inventory refuses before source loading");
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    let prepared = session
        .request_complete_prefab_conversion(revision, &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |reference, _| fixture.load(reference, &basis))
        .unwrap();
    let changed = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("changed-root")),
        ])),
        basis.clone(),
        prefab_key("q"),
    );
    let result = session.commit_complete_prefab_conversion_with_loader(prepared, |reference| {
        if reference == &source("q") {
            Ok::<_, Infallible>(Some(changed.clone()))
        } else {
            fixture.load(reference, &basis)
        }
    });
    assert!(matches!(result, Err(PrefabConversionError::Source(_))));
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    let prepared = session
        .request_complete_prefab_conversion(revision, &id("anchor"), Default::default())
        .unwrap()
        .prepare_with_loader(None, |reference, _| fixture.load(reference, &basis))
        .unwrap();
    assert!(
        session
            .apply_authoring_patch(&ScenePatchDocument::new([
                ScenePatchOperation::RemoveField {
                    entity: id("anchor"),
                    component: hidden_ty(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: field(),
                }
            ]))
            .applied
    );
    let edited = session.shared_document();
    let history = session.history_status();
    let result = session.commit_complete_prefab_conversion_with_loader(
        prepared,
        |_| -> Result<Option<ResolvedPrefabSource>, Infallible> {
            panic!("stale revision must refuse before acquiring dependencies")
        },
    );
    assert!(matches!(result, Err(PrefabConversionError::Source(_))));
    assert!(Arc::ptr_eq(&edited, &session.shared_document()));
    assert_eq!(session.history_status(), history);
}

#[test]
fn retained_source_reference_inventory_is_independent_of_selected_materialized_output() {
    let component = ComponentTypeId::new("test.RetainedSourceReferences");
    let schema = ComponentSchema::new(
        component.clone(),
        "Retained references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("targets"),
        "Targets",
        ComponentFieldPath::from_fields(["targets"]),
        ComponentValueShape::list(ComponentValueShape::EntityRef),
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
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
        &bundle.owners()[0],
        &Default::default(),
    )
    .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let retained = SceneEntityRecord::new(id("retained")).with_component(
        component,
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "targets",
                ComponentValue::List(
                    ["anchor", "retained", "peer", "anchor"]
                        .map(|target| ComponentValue::EntityReference(local(target)))
                        .into(),
                ),
            )]),
        )
        .with_owner(owner()),
    );
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance::root(
        source("empty"),
        prefab_key("empty"),
        id("root"),
    ));
    let document = SceneDocument::from_ordered_roots([
        anchor,
        retained.clone(),
        SceneEntityRecord::new(id("peer")),
    ]);
    let resolved = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")),
        ])),
        basis.clone(),
        prefab_key("empty"),
    );
    let limits =
        SceneCompilationLimits::default().with_materialized_value_nodes(ItemLimit::new(1).unwrap());
    let options = SceneCompilationOptions::default().with_limits(limits);
    let mut session = scene(&document, &basis);
    let before = session.shared_document();
    let revision = session.revision();
    for source_limit in [
        limits.with_reference_work_nodes(ItemLimit::new(1).unwrap()),
        limits.with_working_bytes(ByteLimit::new(1).unwrap()),
    ] {
        let mut reads = 0;
        let request = session
            .request_complete_prefab_conversion(
                revision,
                &id("anchor"),
                options.with_limits(source_limit),
            )
            .unwrap();
        let result = request.prepare_with_loader(None, |_, _| {
            reads += 1;
            Ok::<_, Infallible>(Some(resolved.clone()))
        });
        assert!(matches!(result, Err(PrefabConversionError::Source(_))));
        assert_eq!(
            reads, 0,
            "source work must refuse before dependency IO even with no input queries"
        );
        assert!(Arc::ptr_eq(&before, &session.shared_document()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
    let mut reads = 0;
    let prepared = session
        .request_complete_prefab_conversion(revision, &id("anchor"), options)
        .unwrap()
        .prepare_with_loader(None, |_, _| {
            reads += 1;
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert_eq!(reads, 1);
    assert_eq!(prepared.entities().len(), 1);
    assert_eq!(prepared.entities()[0].entity(), &id("anchor"));
    assert!(prepared.checked_retained_bytes().is_some());
    assert_eq!(prepared.document().entities.len(), document.entities.len());
    assert_eq!(
        prepared
            .document()
            .entities
            .iter()
            .find(|entity| entity.id == id("retained")),
        Some(&retained)
    );
    let reviewed = prepared.document().clone();
    let receipt = session
        .commit_complete_prefab_conversion_with_loader(prepared, |_| {
            Ok::<_, Infallible>(Some(resolved.clone()))
        })
        .unwrap();
    assert!(receipt.report().applied);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &reviewed);

    let mut second = retained.clone();
    second.id = id("second");
    let mut paired = document.clone();
    paired
        .placements
        .push(SceneEntityPlacement::root(second.id.clone()));
    paired.entities.push(second);
    paired.canonicalize();
    let aggregate =
        options.with_limits(limits.with_reference_work_nodes(ItemLimit::new(32).unwrap()));
    // Each four-leaf record fits this source-read allowance; the pair must share it.
    for omitted in ["retained", "second"] {
        let mut single = paired.clone();
        single
            .entities
            .iter_mut()
            .find(|entity| entity.id == id(omitted))
            .unwrap()
            .components
            .clear();
        let single = scene(&single, &basis);
        let mut reads = 0;
        single
            .request_complete_prefab_conversion(single.revision(), &id("anchor"), aggregate)
            .unwrap()
            .prepare_with_loader(None, |_, _| {
                reads += 1;
                Ok::<_, Infallible>(Some(resolved.clone()))
            })
            .unwrap();
        assert_eq!(reads, 1);
    }
    let paired = scene(&paired, &basis);
    let before = paired.shared_document();
    let revision = paired.revision();
    let mut reads = 0;
    let refused = paired
        .request_complete_prefab_conversion(revision, &id("anchor"), aggregate)
        .unwrap()
        .prepare_with_loader(None, |_, _| {
            reads += 1;
            Ok::<_, Infallible>(Some(resolved.clone()))
        });
    assert!(matches!(refused, Err(PrefabConversionError::Source(_))));
    assert_eq!(
        reads, 0,
        "aggregate source reads cannot reset their allowance per record"
    );
    assert!(Arc::ptr_eq(&before, &paired.shared_document()));
    assert_eq!(paired.revision(), revision);
    assert_eq!(paired.history_status().undo_depth, 0);
}
