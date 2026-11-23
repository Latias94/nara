use nara_asset::AssetRef;
use nara_diagnostic::DiagnosticValueRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue,
};

use crate::{
    AuthoringDocumentSource, PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
};

fn instance(configure: impl FnOnce(&mut PrefabOverlay)) -> PrefabInstance {
    let base = SourceDocumentKey::named(
        "6b691227-a68a-4b11-8d1c-a9d5d8c7e952".parse().unwrap(),
        "base",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(base);
    configure(&mut overlay);
    let mut instance = PrefabInstance::forest(AssetRef::path("base.prefab.json").unwrap());
    instance.overrides = Some(overlay);
    instance
}

fn declarations(zero: f64) -> [(&'static str, PrefabInstance); 4] {
    let value = ComponentValue::map([(
        "items",
        ComponentValue::List(vec![ComponentValue::f64(zero).unwrap()]),
    )]);
    let target = PrefabEntityTarget::source(SceneEntityId::new("target").unwrap());
    let component = ComponentTypeId::new("test.Sample");
    let version = ComponentSchemaVersion::ONE;
    let record =
        |value| SceneComponentRecord::new(version, ComponentValue::map([("sample", value)]));
    let with_addition = |record, prefab| {
        instance(|overlay| {
            let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
            if let Some(record) = record {
                addition.components.insert(component.clone(), record);
            }
            addition.prefab = prefab;
            overlay.placements.push(SceneEntityPlacement::new(
                PrefabEntityTarget::addition(addition.id.clone()),
                ScenePlacement::last(ScenePlacementParent::Root),
            ));
            overlay.additions.push(addition);
        })
    };
    let set = instance(|overlay| {
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Add,
            target: target.clone(),
            component: component.clone(),
            record: record(value.clone()),
        });
    });
    let properties = instance(|overlay| {
        overlay.components.push(PrefabComponentIntent::Properties {
            target: target.clone(),
            component: component.clone(),
            version,
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("sample")),
                intent: PrefabPropertyIntent::Set(value.clone()),
            }],
        });
    });
    let addition_record = with_addition(Some(record(value.clone())), None);
    let mut with_default = record(ComponentValue::Null);
    with_default
        .contextual_defaults
        .insert(ComponentFieldId::new("sample"), value);
    let claim = instance(|overlay| {
        overlay.instances.push(PrefabInstanceClaim {
            target,
            value: Some(with_addition(Some(with_default), None)),
        });
    });
    [
        ("component set", set),
        (
            "properties under a nested addition",
            with_addition(None, Some(properties)),
        ),
        ("addition record", addition_record),
        ("contextual default under an instance claim", claim),
    ]
}

#[test]
fn set_prefab_instance_checks_signed_zero_and_restores_the_actual_declaration() {
    let anchor = SceneEntityId::new("anchor").unwrap();
    let document = |prefab| {
        let mut entity = SceneEntityRecord::new(anchor.clone());
        entity.prefab = Some(prefab);
        SceneDocument::from_ordered_roots([entity])
            .to_json_string()
            .unwrap()
    };
    let patch = |expected, value| {
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: anchor.clone(),
            expected: Some(expected),
            value,
        }])
    };
    let source_json =
        |session: &SceneAuthoringSession| session.document().to_json_string().unwrap();
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    for ((case, previous), (_, replacement)) in
        declarations(-0.0).into_iter().zip(declarations(0.0))
    {
        assert_eq!(previous, replacement, "{case}");
        let original = document(previous.clone());
        let changed = document(replacement.clone());
        assert_ne!(original, changed, "{case}");
        let published = SceneDocumentCandidate::decode_json_bytes(original.as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scene.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let mut session = SceneAuthoringSession::from_published_authoring_document(published);
        let revision = session.revision();
        let history = session.history_status();
        let rejected = session.apply_authoring_patch(&patch(replacement.clone(), None));
        assert!(!rejected.applied && rejected.inverse.is_none(), "{case}");
        let diagnostic = rejected.diagnostics.iter().next().unwrap();
        assert_eq!(
            diagnostic.code().as_str(),
            "scene.patch-prefab-instance-context-mismatch",
            "{case}"
        );
        for (key, value) in [
            ("operation-index", DiagnosticValueRef::Unsigned(0)),
            ("entity-id", DiagnosticValueRef::Identifier(anchor.as_str())),
        ] {
            assert!(
                diagnostic
                    .fields()
                    .iter()
                    .any(|field| { field.key().as_str() == key && field.value() == value }),
                "{case}: {key}"
            );
        }
        assert_eq!(session.revision(), revision, "{case}");
        assert_eq!(session.history_status(), history, "{case}");
        assert_eq!(source_json(&session), original, "{case}");

        let forward = patch(previous.clone(), Some(replacement.clone()));
        let inverse = patch(replacement, Some(previous));
        let applied = session.apply_authoring_patch(&forward);
        assert!(applied.applied, "{case}: {:?}", applied.diagnostics);
        assert_eq!(
            applied.inverse.unwrap().to_json_string().unwrap(),
            inverse.to_json_string().unwrap(),
            "{case}"
        );
        assert_eq!(source_json(&session), changed, "{case}");
        assert!(session.undo_authoring().applied, "{case}");
        assert_eq!(source_json(&session), original, "{case}");
        assert!(session.redo_authoring().applied, "{case}");
        assert_eq!(source_json(&session), changed, "{case}");
    }
}
