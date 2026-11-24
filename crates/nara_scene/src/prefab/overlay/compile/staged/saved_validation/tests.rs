use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_identity::{AuthoredEntityReference, EntityReference, ReferenceInputId};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringSchemaBasis, PrefabComponentIntent, PrefabComponentSetMode, PrefabDeclarationSource,
    PrefabDocument, PrefabEntityTarget, PrefabInstance, PrefabInstancePath, PrefabOverlay,
    PrefabOverlaySlot, PrefabReferenceArguments, ResolvedPrefabSource, SceneCompilationFailureKind,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
    SourceDocumentKind,
    prefab::{
        try_compile_authoring_scene_with_loader, try_validate_authoring_saved_prefab_with_loader,
    },
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn request(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{value}.ron")).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "f73d6199-d74f-47e7-9c54-fb256354271d".parse().unwrap(),
        value,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.SavedValidation")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.saved-validation").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let capabilities = ComponentCapability::SCENE_AUTHORING;
    let schema = ComponentSchema::new(component(), "Saved validation", ComponentSchemaVersion::ONE)
        .with_capabilities(capabilities)
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities(capabilities)
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

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

fn reference_record(reference: AuthoredEntityReference) -> SceneComponentRecord {
    record(ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(EntityReference::Authored { reference }),
    )]))
}

fn anchor(value: &str, instance: PrefabInstance) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(value));
    entity.prefab = Some(instance);
    entity
}

#[test]
fn closed_prefab_reports_descendant_add_conflict_from_the_saved_lower_record() {
    let basis = basis();
    let empty = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let target = PrefabEntityTarget::source(id("leaf-root"));
    let mut overlay = PrefabOverlay::new(key("leaf"));
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: target.clone(),
        component: component(),
        record: record(empty.clone()),
    });
    let intermediate = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([anchor(
            "leaf-mount",
            PrefabInstance {
                overrides: Some(overlay),
                ..PrefabInstance::root(request("leaf"), key("leaf"), target)
            },
        )])),
        basis.clone(),
        key("intermediate"),
    );
    let closed = PrefabDocument::from_ordered_roots([anchor(
        "intermediate-mount",
        PrefabInstance::forest(request("intermediate")),
    )]);

    for lower_present in [false, true] {
        let mut leaf_root = SceneEntityRecord::new(id("leaf-root"));
        if lower_present {
            leaf_root = leaf_root.with_component(component(), record(empty.clone()));
        }
        let sources = [
            (request("intermediate"), intermediate.clone()),
            (
                request("leaf"),
                ResolvedPrefabSource::bound_authoring_source(
                    Arc::new(PrefabDocument::from_ordered_roots([leaf_root])),
                    basis.clone(),
                    key("leaf"),
                ),
            ),
        ];
        let report = try_validate_authoring_saved_prefab_with_loader(
            &closed,
            &basis,
            Default::default(),
            |reference, _| {
                Ok::<_, Infallible>(
                    sources
                        .iter()
                        .find(|(asset, _)| asset == reference)
                        .map(|(_, source)| source.clone()),
                )
            },
        )
        .unwrap();
        assert!(report.work_bytes.is_some_and(|bytes| bytes > 0));
        if lower_present {
            assert!(report.diagnostics.has_errors());
            let failure = report.failure.unwrap();
            assert_eq!(failure.reason(), &SceneCompilationFailureKind::Component);
            assert_eq!(
                failure.slot(),
                Some(PrefabOverlaySlot::Component { index: 0 })
            );
            let location = failure.location().unwrap();
            assert_eq!(
                location.source(),
                &PrefabDeclarationSource::Prefab(key("intermediate"))
            );
            assert_eq!(location.path(), &PrefabInstancePath::new(id("leaf-mount")));
        } else {
            assert!(
                report.failure.is_none() && !report.diagnostics.has_errors(),
                "{report:?}"
            );
        }
    }
}

#[test]
fn closed_prefab_rejects_a_promoted_reference_to_a_suppressed_target() {
    let basis = basis();
    let receiver = id("receiver");
    let mut overlay = PrefabOverlay::new(key("destination"));
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(receiver.clone()));
    let request = request("destination");
    let bridge = PrefabDocument::from_ordered_roots([anchor(
        "mount",
        PrefabInstance {
            overrides: Some(overlay),
            ..PrefabInstance::forest(request.clone())
        },
    )]);

    for promoted in [false, true] {
        let mut holder = SceneEntityRecord::new(id("holder"));
        if promoted {
            holder = holder.with_component(
                component(),
                reference_record(AuthoredEntityReference::source(receiver.clone())),
            );
        }
        let destination = ResolvedPrefabSource::bound_authoring_source(
            Arc::new(PrefabDocument::from_ordered_roots([
                holder,
                SceneEntityRecord::new(receiver.clone()),
            ])),
            basis.clone(),
            key("destination"),
        );
        let report = try_validate_authoring_saved_prefab_with_loader(
            &bridge,
            &basis,
            Default::default(),
            |asset, _| {
                assert_eq!(asset, &request);
                Ok::<_, Infallible>(Some(destination.clone()))
            },
        )
        .unwrap();
        if promoted {
            assert!(report.diagnostics.has_errors());
            assert_eq!(
                report.failure.unwrap().reason(),
                &SceneCompilationFailureKind::Reference,
            );
        } else {
            assert!(
                report.failure.is_none() && !report.diagnostics.has_errors(),
                "{report:?}"
            );
        }
    }
}

#[test]
fn saved_prefab_defers_only_a_declared_root_input_reached_through_nested_arguments() {
    let basis = basis();
    let input = ReferenceInputId::new("target").unwrap();
    let child_input = ReferenceInputId::new("child-target").unwrap();
    let mut child = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("child-root"))
        .with_component(
            component(),
            reference_record(AuthoredEntityReference::input(child_input.clone())),
        )]);
    child.inputs.insert(child_input.clone());
    let child =
        ResolvedPrefabSource::bound_authoring_source(Arc::new(child), basis.clone(), key("child"));
    let child_request = request("child");
    let mut template = PrefabDocument::from_ordered_roots([anchor(
        "child-mount",
        PrefabInstance::forest(child_request.clone()),
    )]);
    let validate = |template: &PrefabDocument| {
        try_validate_authoring_saved_prefab_with_loader(
            template,
            &basis,
            Default::default(),
            |asset, _| {
                assert_eq!(asset, &child_request);
                Ok::<_, Infallible>(Some(child.clone()))
            },
        )
        .unwrap()
    };
    let missing_child = validate(&template);
    assert!(missing_child.diagnostics.has_errors());
    let failure = missing_child.failure.unwrap();
    assert_eq!(failure.reason(), &SceneCompilationFailureKind::Reference);
    assert_eq!(
        failure.location().unwrap().path(),
        &PrefabInstancePath::new(id("child-mount")),
    );

    template.entities[0].prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: key("child"),
        values: [(
            child_input,
            EntityReference::Authored {
                reference: AuthoredEntityReference::input(input.clone()),
            },
        )]
        .into(),
    });
    let undeclared_root = validate(&template);
    assert!(undeclared_root.diagnostics.has_errors());
    assert_eq!(
        undeclared_root.failure.unwrap().reason(),
        &SceneCompilationFailureKind::Reference,
    );

    template.inputs.insert(input.clone());
    let deferred = validate(&template);
    assert!(
        deferred.failure.is_none() && !deferred.diagnostics.has_errors(),
        "{deferred:?}"
    );

    let source = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(template),
        basis.clone(),
        key("template"),
    );
    let template_request = request("template");
    let mut scene = SceneDocument::from_ordered_roots([
        anchor("mount", PrefabInstance::forest(template_request.clone())),
        SceneEntityRecord::new(id("receiver")),
    ]);
    let compile = |scene: &SceneDocument| {
        try_compile_authoring_scene_with_loader(
            scene,
            &basis,
            Default::default(),
            None,
            |asset, _| {
                if asset == &child_request {
                    return Ok::<_, Infallible>(Some(child.clone()));
                }
                assert_eq!(asset, &template_request);
                Ok(Some(source.clone()))
            },
        )
        .unwrap()
    };
    let missing = compile(&scene);
    assert!(missing.candidate().is_none() && missing.diagnostics().has_errors());
    assert_eq!(
        missing.failure().unwrap().reason(),
        &SceneCompilationFailureKind::Reference,
    );

    scene.entities[0].prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: key("template"),
        values: [(
            input,
            EntityReference::SceneLocal {
                entity: id("receiver"),
            },
        )]
        .into(),
    });
    let bound = compile(&scene);
    assert!(
        bound.candidate().is_some() && !bound.diagnostics().has_errors(),
        "{bound:?}"
    );
}

#[test]
fn saved_prefab_validation_preserves_loader_error_and_suspended_work_limit() {
    let basis = basis();
    let closed = PrefabDocument::from_ordered_roots([anchor(
        "mount",
        PrefabInstance::forest(request("leaf")),
    )]);
    assert_eq!(
        try_validate_authoring_saved_prefab_with_loader(
            &closed,
            &basis,
            Default::default(),
            |_, _| { Err("loader refused") }
        )
        .unwrap_err(),
        "loader refused"
    );
    let source = ResolvedPrefabSource::bound_authoring_source(
        Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")),
        ])),
        basis.clone(),
        key("leaf"),
    );
    let limited = try_validate_authoring_saved_prefab_with_loader(
        &closed,
        &basis,
        Default::default(),
        |_, work| {
            work.restrict_working_bytes(ByteLimit::new(1).unwrap());
            Ok::<_, Infallible>(Some(source.clone()))
        },
    )
    .unwrap();
    assert!(limited.diagnostics.has_errors());
    assert_eq!(
        limited.failure.unwrap().reason(),
        &SceneCompilationFailureKind::WorkLimit
    );
}
