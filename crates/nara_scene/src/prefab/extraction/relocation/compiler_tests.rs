//! Private relocation proposals consumed by the real authoring compiler.
//! The P/Q split below is a manual fixture, not an extraction builder or publication proof.

use std::{borrow::Cow, collections::BTreeMap, convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};

use super::{AddressRole, BranchRelocation};
use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringSchemaBasis, ExpandedSceneDocument, PrefabComponentIntent, PrefabDocument,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim, PrefabInstanceHop,
    PrefabOverlay, PrefabProjectionStep, PrefabPropertyClaim, PrefabPropertyIntent,
    ResolvedPrefabSource, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityPlacement,
    SceneEntityRecord, ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
    prefab::{
        SourceDocumentInput, extraction::try_capture_source_cut_with_loader,
        try_compile_authoring_scene_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_key(value: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "692e0c62-b4e6-4473-8131-9dcd978351be".parse().unwrap(),
        value,
        kind,
    )
    .unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    source_key(value, SourceDocumentKind::Prefab)
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/relocation-compiler/{value}.ron")).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.RelocationLink")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.relocation-links").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let references = ["target", "root"].map(|name| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)
    });
    let score = ComponentFieldSchema::required(
        ComponentFieldId::new("score"),
        "Score",
        ComponentFieldPath::from_fields(["score"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    let schema = ComponentSchema::new(component(), "Relocation link", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(references.into_iter().chain([score]));
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
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    assert!(basis.has_authoring_adapter(&component()));
    assert!(!basis.has_native_preview_adapter(&component()));
    basis
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

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal { entity: id(entity) }
}

fn record(target: EntityReference, root: EntityReference, score: u64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("target", ComponentValue::EntityReference(target)),
            ("root", ComponentValue::EntityReference(root)),
            ("score", ComponentValue::U64(score)),
        ]),
    )
    .with_owner(owner())
}

fn root_instance(base: &str, root: &str) -> PrefabInstance {
    PrefabInstance::root(request(base), key(base), target(&[], root))
}

fn bound(entity: &str, base: &str, root: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(entity));
    record.prefab = Some(root_instance(base, root));
    record
}

fn pin(target: PrefabEntityTarget, score: u64) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: component(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("score")),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(score)),
        }],
    }
}

fn load(
    reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, &Arc<PrefabDocument>)],
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

fn compile(
    scene: &SceneDocument,
    basis: &AuthoringSchemaBasis,
    sources: &[(&str, &Arc<PrefabDocument>)],
) -> ExpandedSceneDocument {
    let original_scene = scene.clone();
    let originals = sources
        .iter()
        .map(|(_, source)| source.as_ref().clone())
        .collect::<Vec<_>>();
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
    assert_eq!(scene, &original_scene);
    for ((_, source), original) in sources.iter().zip(&originals) {
        assert_eq!(source.as_ref(), original);
    }
    report.into_parts().candidate.unwrap()
}

// Correspondence is explicit typed provenance, never a decoded or compared generated locator.
fn snapshot(
    document: &ExpandedSceneDocument,
    b_path: &[(&str, &str)],
) -> BTreeMap<&'static str, (Vec<&'static str>, ComponentValue)> {
    let steps = b_path
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
    let b = matches
        .next()
        .expect("the exact b occurrence must exist")
        .0
        .clone();
    assert!(matches.next().is_none());
    let correspondence =
        BTreeMap::from([(id("host"), "host"), (id("observer"), "observer"), (b, "b")]);
    assert_eq!(document.entities().len(), correspondence.len());
    assert_eq!(document.provenance().len(), correspondence.len());
    assert_eq!(document.roots(), &[id("host"), id("observer")]);
    let host = &document.provenance()[&id("host")];
    assert!(
        host.projection_path().is_none(),
        "the containing host owns canonical identity"
    );
    assert_eq!(host.local_entity_id(), &id("host"));
    document
        .entities()
        .iter()
        .map(|entity| {
            assert_eq!(entity.components().len(), 1);
            let ComponentValue::Map(mut fields) = entity.components()[&component()].value.clone()
            else {
                panic!("the real schema admits a map");
            };
            for field in ["target", "root"] {
                let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) =
                    &fields[field]
                else {
                    panic!("the compiler must lower each typed reference");
                };
                let normalized = ComponentValue::EntityReference(local(correspondence[entity]));
                fields.insert(field.into(), normalized);
            }
            let children = entity
                .children()
                .iter()
                .map(|child| correspondence[child])
                .collect();
            (
                correspondence[entity.id()],
                (children, ComponentValue::Map(fields)),
            )
        })
        .collect()
}

#[test]
fn proposed_relocation_preserves_compiled_values_references_and_distinct_root_slots() {
    let basis = basis();
    let p = Arc::new(PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("k")),
            SceneEntityPlacement::new(
                target(&[], "b"),
                ScenePlacement::last(ScenePlacementParent::Entity(target(&[], "k"))),
            ),
        ],
        [
            SceneEntityRecord::new(id("b"))
                .with_component(component(), record(local("k"), local("k"), 2)),
            bound("k", "r", "r"),
        ],
    ));
    let r = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("r"))
            .with_component(component(), record(local("r"), local("r"), 1)),
    ]));
    let originals = (p.as_ref().clone(), r.as_ref().clone());
    let report = try_capture_source_cut_with_loader(
        SourceDocumentInput::Prefab(&p),
        &id("k"),
        &basis,
        Default::default(),
        None,
        |reference, _| Ok::<_, Infallible>(load(reference, &basis, &[("r", &r)])),
    )
    .unwrap();
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert!(!report.budget_failed);
    let cut = report.cut.unwrap();
    assert_eq!(cut.entities().len(), 2);
    assert!(std::ptr::eq(cut.root(), &p.entities[1]));
    let (p_key, q_key) = (key("p"), key("q"));
    let relocation = BranchRelocation::new(&cut, &p_key, &q_key).unwrap();
    // Manually authored AFTER sources: Q owns the cut; P retains only its root anchor.
    let q = Arc::new(PrefabDocument::new(
        p.placements.clone(),
        cut.entities().cloned(),
    ));
    let p_after = Arc::new(PrefabDocument::from_ordered_roots([bound("k", "q", "k")]));
    let scene_key = source_key("scene", SourceDocumentKind::Scene);

    // Both selections denote P:k's effective entity. The routed binding must change too.
    for root in [target(&[], "k"), target(&[("k", "r")], "r")] {
        let routed_binding = !root.through.is_empty();
        let mut selected_r = root_instance("r", "r");
        let mut r_overlay = PrefabOverlay::new(key("r"));
        r_overlay.components.push(pin(target(&[], "r"), 91));
        selected_r.overrides = Some(r_overlay);
        let mut overlay = PrefabOverlay::new(p_key.clone());
        overlay.components.push(pin(target(&[], "b"), 77));
        overlay.instances.push(PrefabInstanceClaim {
            target: target(&[], "k"),
            value: Some(selected_r),
        });
        let mut host = bound("host", "p", "k");
        let instance = host.prefab.as_mut().unwrap();
        instance.binding = crate::PrefabInstanceBinding::Root {
            base: p_key.clone(),
            root,
        };
        instance.overrides = Some(overlay);
        let scene = SceneDocument::from_ordered_roots([
            host,
            SceneEntityRecord::new(id("observer")).with_component(
                component(),
                record(
                    reference(&[("host", "p")], "b"),
                    reference(&[("host", "p"), ("k", "r")], "r"),
                    0,
                ),
            ),
        ]);
        let original_scene = scene.clone();
        let before = snapshot(
            &compile(&scene, &basis, &[("p", &p), ("r", &r)]),
            &[("p", "b")],
        );
        let mut after = scene.clone();
        let mut remaining = 64 * 1024usize;
        let mut admit = |bytes: Option<usize>| -> Result<(), &'static str> {
            remaining = bytes
                .and_then(|bytes| remaining.checked_sub(bytes))
                .ok_or("owner allowance exhausted")?;
            Ok(())
        };
        let instance = after.entities[0].prefab.as_mut().unwrap();
        let binding = relocation
            .retained_binding(&instance.binding, &mut admit)
            .unwrap();
        assert_eq!(matches!(&binding, Cow::Owned(_)), routed_binding);
        instance.binding = binding.into_owned();
        let overlay = instance.overrides.as_mut().unwrap();
        let PrefabComponentIntent::Properties { target, .. } = &mut overlay.components[0] else {
            unreachable!()
        };
        *target = relocation
            .retained_target(&p_key, target, AddressRole::Entity, &mut admit)
            .unwrap()
            .into_owned();
        let claim = &mut overlay.instances[0];
        claim.target = relocation
            .retained_target(&p_key, &claim.target, AddressRole::InstanceSlot, &mut admit)
            .unwrap()
            .into_owned();
        let ComponentValue::Map(fields) = &mut after.entities[1]
            .components
            .get_mut(&component())
            .unwrap()
            .value
        else {
            unreachable!()
        };
        for field in ["target", "root"] {
            let ComponentValue::EntityReference(reference) = fields.get_mut(field).unwrap() else {
                unreachable!()
            };
            *reference = relocation
                .retained_reference(&scene_key, reference, &mut admit)
                .unwrap()
                .into_owned();
        }
        assert!(remaining < 64 * 1024);
        let after = snapshot(
            &compile(&after, &basis, &[("p", &p_after), ("q", &q), ("r", &r)]),
            &[("p", "k"), ("q", "b")],
        );
        assert_eq!(before, after);
        assert_eq!(
            before["host"],
            (vec!["b"], record(local("host"), local("host"), 91).value)
        );
        assert_eq!(
            before["b"],
            (vec![], record(local("host"), local("host"), 77).value)
        );
        assert_eq!(
            before["observer"],
            (vec![], record(local("b"), local("host"), 0).value)
        );
        assert_eq!(scene, original_scene);
    }
    assert_eq!((p.as_ref(), r.as_ref()), (&originals.0, &originals.1));
}
