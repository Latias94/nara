#[path = "support/nested_schema.rs"]
mod support;

use std::collections::BTreeSet;

use nara_asset::{AssetPathScope, AssetRef, AssetRefExportPolicy, StableAssetId};
use nara_ecs::World;
use nara_identity::{EntityLookup, EntityReference};
use nara_reflect::{
    ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    PersistentComponentProvider, collect_declared_asset_references,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, InMemoryPrefabSourceResolver,
    PrefabDocument, PrefabInstance, SceneAuthoringSession, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, SceneExportOptions, SourceDocumentKey, SourceDocumentKind,
    export_scene_with_options,
};
use nara_tooling::{SceneInspectorCommand, SceneInspectorState};
use support::{COMPONENT_ID, IMAGE_ID, PREFAB_ID, PREFAB_PATH, Relationships};

#[test]
fn nested_component_survives_inspector_save_prefab_remap_and_export() {
    let registry = support::registry();
    let database = support::database();
    let bundle = support::bundle();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new())
        .expect("a fully typed recursive schema must support data authoring");
    let source =
        AuthoringDocumentSource::new(AssetRef::path("scenes/relationships.scene.json").unwrap());
    let candidate = SceneDocumentCandidate::decode_json_bytes(
        support::document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let published = candidate
        .admit_authoring(source.clone(), &bundle, std::slice::from_ref(&adapter))
        .unwrap()
        .publish();
    assert_eq!(published.unavailable_manifest().total_count(), 0);
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let actor_id = SceneEntityId::new("actor").unwrap();
    let inspector = SceneInspectorState::new();
    let model = inspector.authoring_model_with_selection(&session, Some(&actor_id));
    let component = model
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == COMPONENT_ID)
        .unwrap();
    let targets = component
        .fields
        .iter()
        .find(|field| field.id.as_str() == "targets")
        .unwrap();
    assert_eq!(
        targets.value.as_ref().unwrap(),
        session.document().entities[0].components[&ComponentTypeId::new(COMPONENT_ID)]
            .value
            .field("targets")
            .unwrap()
    );

    let before = session.document().clone();
    let revision = session.revision();
    let history = session.history_status();
    let rejected = inspector.apply_authoring_command(
        &mut session,
        set_field(
            "weights",
            ComponentValue::List(vec![ComponentValue::U64(256)]),
        ),
    );
    assert!(!rejected.applied);
    assert_eq!(session.document(), &before);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);

    let replacement = ComponentValue::List(vec![ComponentValue::U64(7), ComponentValue::U64(255)]);
    let applied =
        inspector.apply_authoring_command(&mut session, set_field("weights", replacement));
    assert!(applied.applied, "{:?}", applied.diagnostics);
    let accepted = session.document().clone();
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &before);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);

    let reopened = SceneDocumentCandidate::decode_json_bytes(
        session.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring(source, &bundle, &[adapter])
    .unwrap()
    .publish();
    assert_eq!(reopened.document(), &accepted);
    let prefab = PrefabDocument::new(
        reopened.document().roots.clone(),
        reopened.document().entities.clone(),
    );
    let prefab_source = AssetRef::path(PREFAB_PATH).unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        prefab_source.clone(),
        SourceDocumentKey::stable_asset(
            StableAssetId::parse_str(PREFAB_ID).unwrap(),
            SourceDocumentKind::Prefab,
        ),
        prefab,
    );
    let anchors = ["first", "second"].map(|id| {
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: prefab_source.clone(),
            overrides: None,
        });
        anchor
    });
    let scene = SceneDocument::from_ordered_roots(anchors);
    let expansion = scene.expand_prefabs_with_asset_resolution(
        &registry,
        &resolver,
        database.resolver(&AssetPathScope::ROOT),
    );
    assert!(
        !expansion.diagnostics.has_errors(),
        "{:?}",
        expansion.diagnostics
    );
    let expanded = expansion.document().unwrap();
    let mut remapped_targets = BTreeSet::new();
    let component_id = ComponentTypeId::new(COMPONENT_ID);
    for entity in &expanded.entities {
        let Some(record) = entity.components.get(&component_id) else {
            continue;
        };
        let value = Relationships::__decode_persistent_component(&record.value).unwrap();
        assert_eq!(value.weights, [7, 255]);
        let Some(EntityReference::SceneLocal { entity: target }) = &value.targets["friends"][0]
        else {
            panic!("prefab remap must preserve the local reference kind");
        };
        assert_ne!(target.as_str(), "friend");
        assert!(expanded.entities.iter().any(|entity| &entity.id == target));
        assert_eq!(
            expansion.provenance().unwrap()[&entity.id]
                .projection_path()
                .unwrap()
                .root_anchor(),
            expansion.provenance().unwrap()[target]
                .projection_path()
                .unwrap()
                .root_anchor()
        );
        assert_eq!(
            expansion.provenance().unwrap()[target]
                .local_entity_id()
                .as_str(),
            "friend"
        );
        assert!(remapped_targets.insert(target.clone()));
        let assets = collect_declared_asset_references(
            &registry,
            &component_id,
            ComponentSchemaVersion::ONE,
            &record.value,
        )
        .unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets[0].asset_ref(),
            &AssetRef::stable_id(IMAGE_ID).unwrap()
        );
    }
    assert_eq!(remapped_targets.len(), 2);

    let mut world = World::new();
    let spawned = nara_scene::spawn_scene(
        &mut world,
        &registry,
        &expansion.candidate.unwrap().bind_source(scene_source_key(
            "nested_component_survives_inspector_save_prefab_remap_and_export/source",
        )),
        &scene_occurrence(
            "nested_component_survives_inspector_save_prefab_remap_and_export/spawned",
        ),
        Some(database.resolver(&AssetPathScope::ROOT)),
    );
    assert!(
        !spawned.diagnostics.has_errors(),
        "{:?}",
        spawned.diagnostics
    );
    let instance = spawned.instance.unwrap();
    for target in remapped_targets {
        assert!(matches!(
            instance.resolve(&world, &target),
            EntityLookup::Resolved(_)
        ));
    }
    let exported = export_scene_with_options(
        &world,
        &registry,
        SceneExportOptions {
            asset_ref_export_policy: AssetRefExportPolicy::StableIdWhenKnown,
            ..Default::default()
        },
    );
    assert!(
        !exported.diagnostics.has_errors(),
        "{:?}",
        exported.diagnostics
    );
    let exported = exported.into_output().unwrap().document;
    let exported_ids = exported
        .entities
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<BTreeSet<_>>();
    let mut exported_components = 0;
    for entity in &exported.entities {
        let Some(record) = entity.components.get(&component_id) else {
            continue;
        };
        exported_components += 1;
        let value = Relationships::__decode_persistent_component(&record.value).unwrap();
        assert_eq!(value.weights, [7, 255]);
        let Some(EntityReference::SceneLocal { entity: target }) = &value.targets["friends"][0]
        else {
            panic!("export must emit document-local nested references");
        };
        assert!(exported_ids.contains(target));
        assert!(value.targets["friends"][1].is_none());
        assert_eq!(
            value.portraits,
            [Some(AssetRef::stable_id(IMAGE_ID).unwrap()), None]
        );
    }
    assert_eq!(exported_components, 2);
    let reloaded =
        SceneDocumentCandidate::decode_ron_bytes(exported.to_ron_string().unwrap().as_bytes())
            .unwrap()
            .canonicalize(&registry)
            .unwrap()
            .publish_with_asset_resolution(&registry, database.resolver(&AssetPathScope::ROOT))
            .unwrap();
    assert_eq!(reloaded.document(), &exported);
}

fn set_field(field: &str, value: ComponentValue) -> SceneInspectorCommand {
    SceneInspectorCommand::SetField {
        entity: SceneEntityId::new("actor").unwrap(),
        component: ComponentTypeId::new(COMPONENT_ID),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value,
    }
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
