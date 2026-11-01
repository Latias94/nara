use std::{collections::BTreeMap, sync::Arc};

use nara_asset::AssetRef;
use nara_ecs::Component;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistryError, ComponentSchemaCatalog,
    ComponentSchemaOwnerId, ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSource, ComponentSchemaProviderSourceError, ComponentSchemaVersion,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringSchemaBasis, SceneDocument, SceneDocumentCandidate, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind,
};

use super::*;

const NATIVE_OWNER: ComponentSchemaOwnerId = ComponentSchemaOwnerId::new("play.native");

#[derive(Component, Clone)]
struct RuntimeSettings {
    amount: u64,
    locked: u64,
    optional: Option<u64>,
    defaulted: Option<u64>,
    text: String,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            amount: 3,
            locked: 7,
            optional: Some(5),
            defaulted: None,
            text: "authored".into(),
        }
    }
}

#[derive(Clone, Copy)]
enum Policy {
    Editable,
    ReadOnlyField,
    NoTemplate,
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("play.Settings")
}

fn entity_id() -> SceneEntityId {
    SceneEntityId::new("player").unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(NATIVE_OWNER.as_str()).unwrap()
}

fn encode_settings(settings: &RuntimeSettings) -> ComponentValue {
    let mut fields = BTreeMap::from([
        (
            "settings".to_owned(),
            ComponentValue::map([
                ("wire_amount", ComponentValue::U64(settings.amount)),
                (
                    "wire_locked",
                    ComponentValue::map([("value", ComponentValue::U64(settings.locked))]),
                ),
            ]),
        ),
        (
            "text".to_owned(),
            ComponentValue::String(settings.text.clone()),
        ),
    ]);
    if let Some(value) = settings.optional {
        fields.insert("optional".into(), ComponentValue::U64(value));
    }
    if let Some(value) = settings.defaulted {
        fields.insert("defaulted".into(), ComponentValue::U64(value));
    }
    ComponentValue::Map(fields)
}

fn decode_settings(value: &ComponentValue) -> Result<RuntimeSettings, ComponentCodecError> {
    let settings = value.field("settings")?;
    let optional = |name| {
        value
            .get(name)
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| ComponentCodecError::invalid_field(name, "u64"))
            })
            .transpose()
    };
    Ok(RuntimeSettings {
        amount: settings.field_u64("wire_amount")?,
        locked: settings.field("wire_locked")?.field_u64("value")?,
        optional: optional("optional")?,
        defaulted: optional("defaulted")?,
        text: value.field_str("text")?.to_owned(),
    })
}

fn settings_schema(editable_locked: bool, template: bool) -> ComponentSchema {
    let required = |id, path| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(id),
            id,
            ComponentFieldPath::from_fields(path),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let locked = ComponentFieldSchema::required(
        ComponentFieldId::new("locked"),
        "Locked",
        ComponentFieldPath::from_fields(["settings", "wire_locked"]),
        ComponentValueKind::Map,
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]);
    let schema = ComponentSchema::new(component(), "Settings", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            required("amount", vec!["settings", "wire_amount"]),
            if editable_locked {
                locked.with_capability(ComponentCapability::Edit)
            } else {
                locked
            },
            ComponentFieldSchema::optional(
                ComponentFieldId::new("optional"),
                "Optional",
                ComponentFieldPath::from_fields(["optional"]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("defaulted"),
                "Defaulted",
                ComponentFieldPath::from_fields(["defaulted"]),
                ComponentValueKind::U64,
                ComponentValue::U64(11),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::required(
                ComponentFieldId::new("text"),
                "Text",
                ComponentFieldPath::from_fields(["text"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    if template {
        schema.with_authoring_initial_value(encode_settings(&RuntimeSettings {
            defaulted: Some(11),
            ..Default::default()
        }))
    } else {
        schema
    }
}

fn editable_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![settings_schema(true, true)],
        ..Default::default()
    })
}

fn readonly_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![settings_schema(false, true)],
        ..Default::default()
    })
}

fn no_template_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![settings_schema(true, false)],
        ..Default::default()
    })
}

fn register_settings(
    registry: &mut ComponentRegistryBuilder,
    schema: ComponentSchema,
) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_with_codec::<RuntimeSettings, _, _>(
        schema,
        decode_settings,
        |settings| Ok(encode_settings(settings)),
    )?;
    Ok(())
}

fn register_editable(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    register_settings(registry, settings_schema(true, true))
}

fn register_readonly(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    register_settings(registry, settings_schema(false, true))
}

fn register_no_template(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    register_settings(registry, settings_schema(true, false))
}

fn definitions(policy: Policy) -> (ComponentRegistrySnapshot, AuthoringSchemaBasis) {
    let (catalog, register): (
        ComponentSchemaProviderSource,
        fn(&mut ComponentRegistryBuilder) -> Result<(), ComponentRegistryError>,
    ) = match policy {
        Policy::Editable => (editable_catalog, register_editable),
        Policy::ReadOnlyField => (readonly_catalog, register_readonly),
        Policy::NoTemplate => (no_template_catalog, register_no_template),
    };
    let provider =
        nara_reflect::__private::resolve_schema_provider(ComponentSchemaProviderDefinition::new(
            NATIVE_OWNER,
            nara_app::PluginSchemaProviderId::new("play.native.provider"),
            ComponentSchemaProviderBindingId::new("play.native.binding", 1),
            catalog,
            register,
        ))
        .unwrap();
    let data_owner = PersistentComponentSchemaOwnerId::try_new("play.data").unwrap();
    let bundle = AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(owner(), provider.current().catalog().clone(), None),
        AuthoringSchemaBundleOwner::new(
            data_owner.clone(),
            ComponentSchemaCatalog {
                components: vec![
                    ComponentSchema::new(
                        ComponentTypeId::new("play.Data"),
                        "Data",
                        ComponentSchemaVersion::ONE,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("data"),
                        "Data",
                        ComponentFieldPath::empty(),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                ],
                ..Default::default()
            },
            None,
        ),
    ])
    .unwrap();
    let adapters = [
        AuthoringAdapterReceipt::from_trusted_provider(
            provider.contribution_receipt(),
            "play.native.authoring",
            1,
            1,
            [component()],
        )
        .unwrap(),
        AuthoringAdapterReceipt::from_schema_owner(
            bundle
                .owners()
                .iter()
                .find(|entry| entry.owner_id() == &data_owner)
                .unwrap(),
            &Default::default(),
        )
        .unwrap(),
    ];
    let basis = AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap();
    let mut registry = ComponentRegistryBuilder::new();
    nara_reflect::__private::register_resolved_schema_provider(&mut registry, provider).unwrap();
    (registry.freeze().unwrap(), basis)
}

fn record(settings: &RuntimeSettings) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, encode_settings(settings))
        .with_owner(owner())
}

fn document(settings: Option<&RuntimeSettings>) -> SceneDocument {
    let mut entity = SceneEntityRecord::new(entity_id());
    if let Some(settings) = settings {
        entity.components.insert(component(), record(settings));
    }
    SceneDocument::from_ordered_roots([entity])
}

fn open(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/apply.scene.json").unwrap()),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

fn runtime(registry: &ComponentRegistrySnapshot, settings: &RuntimeSettings) -> (World, Entity) {
    let mut world = World::new();
    let mut source = document(Some(settings));
    source.entities[0].components.insert(
        component(),
        record(settings).with_owner(
            PersistentComponentSchemaOwnerId::try_new(
                registry
                    .component_schema_owner(&component())
                    .unwrap()
                    .as_str(),
            )
            .unwrap(),
        ),
    );
    let report = nara_scene::spawn_scene(
        &mut world,
        registry,
        &nara_scene::ExpandedSceneCandidate::plain(
            SourceDocumentKey::named(
                "47c4e2a0-b8d6-4d0b-94c3-b264a3695e73".parse().unwrap(),
                "apply-changes-tests",
                SourceDocumentKind::Scene,
            )
            .unwrap(),
            Arc::new(source),
        ),
        &SceneOccurrenceKey::new("play.tests", "runtime").unwrap(),
        None,
    );
    assert!(report.instance.is_some(), "{:?}", report.diagnostics);
    let entity = world
        .iter_entities()
        .find(|entity| entity.get::<SceneEntitySource>().is_some())
        .unwrap()
        .id();
    (world, entity)
}

fn export(
    world: &World,
    session: &SceneAuthoringSession,
    registry: &ComponentRegistrySnapshot,
) -> SceneApplyChangesReport {
    __export_apply_changes_from_world(
        world,
        session,
        registry,
        session.revision(),
        SceneApplyChangesRequest::new(entity_id(), [component()]),
        ComponentEncodeContext::new(),
    )
}

fn mixed_document() -> SceneDocument {
    let mut document = document(Some(&RuntimeSettings::default()));
    let entity = &mut document.entities[0];
    entity
        .components
        .get_mut(&component())
        .unwrap()
        .value
        .set_path(
            &ComponentFieldPath::from_fields(["settings", "wire_locked", "unknown_native_member"]),
            ComponentValue::String("preserve codec-opaque read-only data".into()),
        )
        .unwrap();
    entity.components.insert(
        ComponentTypeId::new("play.Data"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(19))
            .with_owner(PersistentComponentSchemaOwnerId::try_new("play.data").unwrap()),
    );
    entity.components.insert(
        ComponentTypeId::new("missing.Future"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(9),
            ComponentValue::map([("opaque", ComponentValue::String("retain exactly".into()))]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new("missing.owner").unwrap()),
    );
    document
}

#[test]
fn source_apply_changes_edits_declared_roots_and_undo_preserves_mixed_degraded_source() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let mut session = open(&mixed_document(), &basis);
    assert_eq!(
        session.authoring_readiness(),
        Some(AuthoringDocumentReadiness::Degraded)
    );
    let original = session.document().clone();
    let original_bytes = original.to_json_string().unwrap();
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let revision = session.revision();
    let content = session.content_state();
    let (mut world, entity) = runtime(&registry, &RuntimeSettings::default());
    {
        let mut settings = world.get_mut::<RuntimeSettings>(entity).unwrap();
        settings.amount = 42;
        settings.locked = 99;
        settings.optional = None;
        settings.defaulted = Some(11);
    }
    let report = export(&world, &session, &registry);
    assert!(report.supported, "{:?}", report.diagnostics);
    assert!(!report.applied);
    assert_eq!(
        report.components[0].status,
        SceneApplyChangesComponentStatus::Pending
    );
    assert_eq!(report.components[0].operation_count, 2);
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
    let patch = report.patch.unwrap();
    assert_eq!(patch.operations.len(), 2);
    assert!(matches!(
        &patch.operations[0],
        ScenePatchOperation::SetField { field, value: ComponentValue::U64(42), .. }
            if field == &ComponentFieldId::new("amount")
    ));
    assert!(matches!(
        &patch.operations[1],
        ScenePatchOperation::RemoveField { field, .. }
            if field == &ComponentFieldId::new("optional")
    ));
    let mut expected = original.clone();
    let value = &mut expected.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value;
    value
        .set_path(
            &ComponentFieldPath::from_fields(["settings", "wire_amount"]),
            ComponentValue::U64(42),
        )
        .unwrap();
    value
        .remove_path(&ComponentFieldPath::from_fields(["optional"]))
        .unwrap();
    let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
    assert_eq!(prepared.document(), &expected);
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &expected);
    assert_eq!(session.authoring_schema_basis(), Some(basis.clone()));
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(session.history_status().undo_depth, 1);
    assert_ne!(session.content_state(), content);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document().to_json_string().unwrap(), original_bytes);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &expected);
}

#[test]
fn source_apply_changes_keeps_omitted_defaults_and_read_only_differences_as_no_ops() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let session = open(&mixed_document(), &basis);
    let original = session.document().clone();
    let (world, _) = runtime(
        &registry,
        &RuntimeSettings {
            locked: 99,
            defaulted: Some(11),
            ..Default::default()
        },
    );
    let report = export(&world, &session, &registry);
    assert!(report.supported, "{:?}", report.diagnostics);
    assert!(report.patch.is_none());
    assert_eq!(
        report.components[0].status,
        SceneApplyChangesComponentStatus::NoOp
    );
    assert_eq!(report.components[0].operation_count, 0);
    assert_eq!(session.document(), &original);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn source_apply_changes_adds_the_exact_template_then_fields_and_removes_through_source_history() {
    let (registry, basis) = definitions(Policy::Editable);
    let mut session = open(&document(None), &basis);
    let original = session.document().clone();
    let (mut world, entity) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            defaulted: Some(11),
            ..Default::default()
        },
    );
    let report = export(&world, &session, &registry);
    assert!(report.supported, "{:?}", report.diagnostics);
    assert_eq!(report.components[0].operation_count, 2);
    let patch = report.patch.unwrap();
    assert!(matches!(
        &patch.operations[0],
        ScenePatchOperation::AddComponent { value, .. }
            if Some(value) == session.authoring_initial_component(&component()).as_ref()
    ));
    assert!(matches!(
        patch.operations[1],
        ScenePatchOperation::SetField { .. }
    ));
    assert!(session.apply_authoring_patch(&patch).applied);
    let added = session.document().clone();
    assert_eq!(
        added.entities[0].components[&component()].owner(),
        Some(&owner())
    );
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &added);

    world.entity_mut(entity).remove::<RuntimeSettings>();
    let report = export(&world, &session, &registry);
    assert!(report.supported, "{:?}", report.diagnostics);
    assert_eq!(report.components[0].operation_count, 1);
    let patch = report.patch.unwrap();
    assert!(matches!(
        patch.operations.as_slice(),
        [ScenePatchOperation::RemoveComponent { .. }]
    ));
    assert!(session.apply_authoring_patch(&patch).applied);
    assert_eq!(session.document(), &original);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &added);
}

#[test]
fn source_apply_changes_refuses_creation_without_a_template_and_read_only_component_removal() {
    for policy in [Policy::NoTemplate, Policy::ReadOnlyField] {
        let (registry, basis) = definitions(policy);
        let settings = RuntimeSettings::default();
        let mut source = document(None);
        let (mut world, entity) = runtime(&registry, &settings);
        if matches!(policy, Policy::ReadOnlyField) {
            source = document(Some(&settings));
            world.entity_mut(entity).remove::<RuntimeSettings>();
        }
        let session = open(&source, &basis);
        let revision = session.revision();
        let report = export(&world, &session, &registry);
        assert!(!report.supported, "{:?}", report);
        assert!(report.patch.is_none());
        assert_eq!(
            report.components[0].status,
            SceneApplyChangesComponentStatus::Rejected
        );
        assert_eq!(report.components[0].operation_count, 0);
        assert_eq!(session.document(), &source);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn source_apply_changes_refuses_native_owner_and_same_version_schema_disagreement() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let session = open(&mixed_document(), &basis);
    let (other_schema, _) = definitions(Policy::Editable);
    let mut other_owner = ComponentRegistryBuilder::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("foreign.native"),
        registry.catalog().clone(),
        None,
    )
    .unwrap();
    other_owner
        .register_native_component_with_codec::<RuntimeSettings, _, _>(
            &component(),
            decode_settings,
            |settings| Ok(encode_settings(settings)),
        )
        .unwrap();
    let other_owner = other_owner.freeze().unwrap();
    let revision = session.revision();
    for native in [&other_schema, &other_owner] {
        let (world, _) = runtime(native, &RuntimeSettings::default());
        let report = export(&world, &session, native);
        assert!(!report.supported);
        assert!(report.patch.is_none());
        assert_eq!(report.components[0].operation_count, 0);
        assert!(
            report.diagnostics.iter().any(
                |entry| entry.code().as_str() == "tooling.apply-changes-source-schema-mismatch"
            )
        );
    }
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn source_apply_changes_rejects_unavailable_selected_records_without_native_migration() {
    let (registry, basis) = definitions(Policy::Editable);
    let mut source = mixed_document();
    source.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .version = ComponentSchemaVersion(2);
    let session = open(&source, &basis);
    let (world, _) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            ..Default::default()
        },
    );
    let report = export(&world, &session, &registry);
    assert!(!report.supported);
    assert!(report.patch.is_none());
    assert_eq!(report.components[0].operation_count, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str()
                == "tooling.apply-changes-source-component-unavailable")
    );
    assert_eq!(session.document(), &source);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn source_apply_changes_discards_partial_exports_and_reports_zero_published_operations() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let session = open(&mixed_document(), &basis);
    let original = session.document().clone();
    let (world, _) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            ..Default::default()
        },
    );
    let valid = export(&world, &session, &registry);
    assert!(valid.supported, "{:?}", valid.diagnostics);
    assert_eq!(valid.components[0].operation_count, 1);
    for components in [
        vec![component(), ComponentTypeId::new("play.Data")],
        vec![component(), component()],
    ] {
        let report = __export_apply_changes_from_world(
            &world,
            &session,
            &registry,
            session.revision(),
            SceneApplyChangesRequest::new(entity_id(), components),
            ComponentEncodeContext::new(),
        );
        assert!(!report.supported);
        assert!(report.patch.is_none());
        assert_eq!(report.components.len(), 2);
        assert!(report.components.iter().all(|component| component.status
            == SceneApplyChangesComponentStatus::Rejected
            && component.operation_count == 0));
    }
    assert_eq!(session.document(), &original);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn source_apply_changes_rejects_oversized_fields_before_copying_the_patch() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let session = open(&mixed_document(), &basis);
    let original = session.document().clone();
    let revision = session.revision();
    let (world, _) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            text: "x".repeat(70 * 1024),
            ..Default::default()
        },
    );
    let report = export(&world, &session, &registry);
    assert!(!report.supported);
    assert!(report.patch.is_none());
    assert_eq!(
        report.components[0].status,
        SceneApplyChangesComponentStatus::Rejected
    );
    assert_eq!(report.components[0].operation_count, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| { entry.code().as_str() == "tooling.apply-changes-field-value-limit" })
    );
    assert_eq!(session.document(), &original);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn source_apply_changes_rejects_a_stale_revision_and_owner_prepared_commit() {
    let (registry, basis) = definitions(Policy::ReadOnlyField);
    let mut session = open(&mixed_document(), &basis);
    let revision = session.revision();
    let (world, _) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            ..Default::default()
        },
    );
    let patch = export(&world, &session, &registry).patch.unwrap();
    let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
    assert!(
        session
            .apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: entity_id(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("amount"),
                value: ComponentValue::U64(17),
            },]))
            .applied
    );
    let current = session.document().clone();
    let current_revision = session.revision();
    let history = session.history_status();
    let report = __export_apply_changes_from_world(
        &world,
        &session,
        &registry,
        revision,
        SceneApplyChangesRequest::new(entity_id(), [component()]),
        ComponentEncodeContext::new(),
    );
    assert!(!report.supported);
    assert!(report.patch.is_none());
    assert!(!session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &current);
    assert_eq!(session.revision(), current_revision);
    assert_eq!(session.history_status(), history);
}

#[test]
fn independent_native_scene_sessions_keep_native_patch_export_and_history() {
    let (registry, _) = definitions(Policy::NoTemplate);
    let mut session = SceneAuthoringSession::new(document(Some(&RuntimeSettings::default())));
    let original = session.document().clone();
    let (mut world, entity) = runtime(
        &registry,
        &RuntimeSettings {
            amount: 42,
            ..Default::default()
        },
    );
    let report = export(&world, &session, &registry);
    assert!(report.supported);
    assert_eq!(report.components[0].operation_count, 1);
    let patch = report.patch.unwrap();
    assert!(matches!(
        patch.operations.as_slice(),
        [ScenePatchOperation::ReplaceComponent { .. }]
    ));
    assert!(session.apply_patch(&patch, &registry).applied);
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &original);

    world.entity_mut(entity).remove::<RuntimeSettings>();
    let patch = export(&world, &session, &registry).patch.unwrap();
    assert!(matches!(
        patch.operations.as_slice(),
        [ScenePatchOperation::RemoveComponent { .. }]
    ));
    assert!(session.apply_patch(&patch, &registry).applied);
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &original);

    let mut empty = SceneAuthoringSession::new(document(None));
    world.entity_mut(entity).insert(RuntimeSettings::default());
    let patch = export(&world, &empty, &registry).patch.unwrap();
    assert!(matches!(
        patch.operations.as_slice(),
        [ScenePatchOperation::AddComponent { .. }]
    ));
    assert!(empty.apply_patch(&patch, &registry).applied);
    assert!(empty.undo(&registry).applied);
    assert_eq!(empty.document(), &document(None));
}
