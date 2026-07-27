use nara_app::PluginSchemaProviderId;
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentProjectionError, ComponentRegistry, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
    ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSourceError, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};

#[derive(Clone, Component)]
struct Position {
    x: f32,
    debug_label: String,
}

#[derive(Clone, Component)]
struct Velocity {
    dx: f32,
}

#[derive(Clone, Component)]
struct CoverageProbe;

const TEST_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("nara.test.registry-provider");
const TEST_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("nara.test.registry-owner");
const TEST_PROVIDER_BINDING: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("nara.test.registry-provider.native", 1);
const OWNER_A: ComponentSchemaOwnerId = ComponentSchemaOwnerId::new("nara.test.owner-a");
const OWNER_B: ComponentSchemaOwnerId = ComponentSchemaOwnerId::new("nara.test.owner-b");
const PROVIDER_P: PluginSchemaProviderId = PluginSchemaProviderId::new("nara.test.provider-p");
const PROVIDER_Q: PluginSchemaProviderId = PluginSchemaProviderId::new("nara.test.provider-q");
const PROVIDER_P_BINDING: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("nara.test.provider-p.native", 1);
const PROVIDER_Q_BINDING: ComponentSchemaProviderBindingId =
    ComponentSchemaProviderBindingId::new("nara.test.provider-q.native", 1);

fn empty_provider(
    owner: ComponentSchemaOwnerId,
    provider: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
) -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::new(
        owner,
        provider,
        binding,
        empty_catalog_source,
        register_empty_provider,
    )
}

fn owner_successor_candidate(
    predecessor: ComponentSchemaCatalog,
    components: impl IntoIterator<Item = ComponentSchema>,
) -> Result<ComponentRegistry, ComponentRegistryError> {
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the test predecessor generation has a successor");
    current.components.extend(components);
    ComponentRegistry::from_owner_catalog_candidate(TEST_OWNER_ID, current, Some(predecessor))
}

fn empty_catalog_source() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog::default())
}

fn empty_successor_catalog_source()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    ComponentSchemaCatalog::successor_of(&ComponentSchemaCatalog::default())
        .map_err(|_| ComponentSchemaProviderSourceError::new("empty-successor-exhausted"))
}

fn invalid_source_error_code() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    Err(ComponentSchemaProviderSourceError::new(
        "invalid source error with spaces",
    ))
}

fn panic_if_provider_source_runs()
-> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    panic!("a nested provider source must not run")
}

fn position_catalog_source() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![position_schema("x", "Position X")],
        ..ComponentSchemaCatalog::default()
    })
}

fn register_empty_provider(
    _registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    Ok(())
}

fn panic_if_provider_callback_runs(
    _registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    panic!("a frozen registry must validate receipts without invoking provider code")
}

fn reject_provider_callback(
    _registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    Err(ComponentRegistryError::Frozen)
}

fn register_position_provider(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema)?;
    register_position_binding(registry, &id)
}

fn register_nested_provider(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("nara.test.nested-owner"),
        PluginSchemaProviderId::new("nara.test.nested-provider"),
        ComponentSchemaProviderBindingId::new("nara.test.nested-provider.native", 1),
        panic_if_provider_source_runs,
        panic_if_provider_callback_runs,
    )
    .register_or_validate_into(registry)
}

fn replace_candidate_and_register_nested_provider(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    *registry = ComponentRegistry::new();
    ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("nara.test.smuggled-owner"),
        PluginSchemaProviderId::new("nara.test.smuggled-provider"),
        ComponentSchemaProviderBindingId::new("nara.test.smuggled-provider.native", 1),
        empty_catalog_source,
        register_empty_provider,
    )
    .register_or_validate_into(registry)
}

fn register_position_then_reject(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema)?;
    register_position_binding(registry, &id)?;
    Err(ComponentRegistryError::Frozen)
}

fn register_position_then_panic(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema)?;
    register_position_binding(registry, &id)?;
    panic!("provider panic after private candidate mutation")
}

fn authoring_capabilities() -> [ComponentCapability; 3] {
    [
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::Edit,
    ]
}

fn position_schema(path: &str, alias: &str) -> ComponentSchema {
    position_schema_at_version(path, alias, ComponentSchemaVersion::ONE)
}

fn position_schema_at_version(
    path: &str,
    alias: &str,
    version: ComponentSchemaVersion,
) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.Position"),
        "Position",
        version,
    )
    .with_capabilities(authoring_capabilities())
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("position.x"),
            alias,
            ComponentFieldPath::from_fields([path]),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities()),
        ComponentFieldSchema::optional(
            ComponentFieldId::new("position.debug_label"),
            "Debug label",
            ComponentFieldPath::from_fields(["debug_label"]),
            ComponentValueKind::String,
        )
        .with_capabilities([ComponentCapability::Inspect]),
    ])
}

fn frozen_position_catalog(schema: ComponentSchema) -> ComponentSchemaCatalog {
    let id = schema.id().clone();
    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    registry.freeze().unwrap();
    registry.catalog().unwrap().clone()
}

fn register_position_binding(
    registry: &mut ComponentRegistry,
    id: &ComponentTypeId,
) -> Result<(), ComponentRegistryError> {
    registry.register_native_component_with_codec::<Position, _, _>(
        id,
        |value| {
            Ok(Position {
                x: value.field_f64("x")? as f32,
                debug_label: value
                    .get("debug_label")
                    .and_then(ComponentValue::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        },
        |position| {
            Ok(ComponentValue::map([
                ("x", ComponentValue::f64(f64::from(position.x))?),
                (
                    "debug_label",
                    ComponentValue::String(position.debug_label.clone()),
                ),
            ]))
        },
    )?;
    Ok(())
}

#[test]
fn stable_identifiers_are_bounded_and_schema_versions_are_non_zero() {
    assert!(ComponentTypeId::try_new("").is_err());
    assert!(ComponentFieldId::try_new("").is_err());
    assert!(ComponentTypeId::try_new("x".repeat(ComponentTypeId::MAX_BYTES + 1)).is_err());
    assert!(ComponentFieldId::try_new("x".repeat(ComponentFieldId::MAX_BYTES + 1)).is_err());
    assert!(ComponentSchemaVersion::new(0).is_none());
}

#[test]
fn freeze_is_atomic_and_missing_binding_can_be_repaired() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    let candidate_before = registry.catalog_candidate().clone();

    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::MissingNativeBinding { component_id })
            if component_id == id
    ));
    assert!(!registry.is_frozen());
    assert_eq!(registry.catalog_candidate(), &candidate_before);

    register_position_binding(&mut registry, &id).unwrap();
    registry.freeze().unwrap();
    let first = registry.snapshot().unwrap();
    registry.freeze().unwrap();
    let second = registry.snapshot().unwrap();
    assert!(first.ptr_eq(&second));
}

#[test]
fn frozen_snapshots_share_provider_behavior_receipts_without_replaying_callbacks() {
    let provider = ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        empty_catalog_source,
        register_empty_provider,
    );
    let mut registry = ComponentRegistry::new();
    provider.register_or_validate_into(&mut registry).unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    let receipt = snapshot.provider_receipt(TEST_PROVIDER_ID).unwrap();
    assert_eq!(receipt.binding(), TEST_PROVIDER_BINDING);
    assert_eq!(snapshot.provider_receipts().count(), 1);

    let mut shared = ComponentRegistry::from_snapshot(snapshot.clone());
    assert!(shared.shares_snapshot(&snapshot));
    ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        empty_catalog_source,
        panic_if_provider_callback_runs,
    )
    .register_or_validate_into(&mut shared)
    .unwrap();
    assert!(shared.shares_snapshot(&snapshot));

    for binding in [
        ComponentSchemaProviderBindingId::new("nara.test.registry-provider.native", 2),
        TEST_PROVIDER_BINDING.with_codec_version(2),
        TEST_PROVIDER_BINDING.with_migration_version(2),
    ] {
        assert!(matches!(
            ComponentSchemaProviderDefinition::new(
                TEST_OWNER_ID,
                TEST_PROVIDER_ID,
                binding,
                empty_catalog_source,
                register_empty_provider,
            )
            .preflight(&shared),
            Err(ComponentRegistryError::DivergentSchemaProviderReceipt { provider })
                if provider == TEST_PROVIDER_ID
        ));
    }

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            ComponentSchemaOwnerId::new("nara.test.missing-owner"),
            PluginSchemaProviderId::new("nara.test.missing-provider"),
            ComponentSchemaProviderBindingId::new("nara.test.missing-provider.native", 1),
            empty_catalog_source,
            register_empty_provider,
        )
        .preflight(&shared),
        Err(ComponentRegistryError::MissingSchemaProviderReceipt { .. })
    ));
}

#[test]
fn failed_provider_registration_publishes_no_behavior_receipt() {
    let mut registry = ComponentRegistry::new();
    let Err(error) = ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        empty_catalog_source,
        reject_provider_callback,
    )
    .register_or_validate_into(&mut registry) else {
        panic!("the rejected provider unexpectedly registered");
    };
    assert_eq!(error, ComponentRegistryError::Frozen);

    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.provider_receipt(TEST_PROVIDER_ID), None);
    assert_eq!(snapshot.provider_receipts().count(), 0);
    assert_eq!(snapshot.owner_receipts().count(), 0);
}

#[test]
fn direct_registry_rejects_a_second_owner_definition_before_its_callback() {
    let mut registry = ComponentRegistry::new();
    ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        empty_catalog_source,
        register_empty_provider,
    )
    .register_or_validate_into(&mut registry)
    .unwrap();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            PluginSchemaProviderId::new("nara.test.alternate-provider"),
            ComponentSchemaProviderBindingId::new("nara.test.alternate-provider.native", 1),
            empty_catalog_source,
            panic_if_provider_callback_runs,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner })
            if owner == TEST_OWNER_ID
    ));
}

#[test]
fn provider_receipt_cannot_be_repaired_with_an_unrelated_owner_receipt() {
    let mut registry = ComponentRegistry::new();
    empty_provider(OWNER_A, PROVIDER_P, PROVIDER_P_BINDING)
        .register_or_validate_into(&mut registry)
        .unwrap();
    empty_provider(OWNER_B, PROVIDER_Q, PROVIDER_Q_BINDING)
        .register_or_validate_into(&mut registry)
        .unwrap();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            OWNER_B,
            PROVIDER_P,
            PROVIDER_P_BINDING,
            empty_catalog_source,
            panic_if_provider_callback_runs,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::DivergentSchemaProviderReceipt { provider })
            if provider == PROVIDER_P
    ));

    registry.freeze().unwrap();
    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            OWNER_B,
            PROVIDER_P,
            PROVIDER_P_BINDING,
            empty_catalog_source,
            panic_if_provider_callback_runs,
        )
        .preflight(&registry),
        Err(ComponentRegistryError::DivergentSchemaOwnerReceipt { owner })
            if owner == OWNER_B
    ));
}

#[test]
fn executable_fingerprint_commits_the_provider_to_owner_mapping() {
    fn snapshot(
        contributions: [(
            ComponentSchemaOwnerId,
            PluginSchemaProviderId,
            ComponentSchemaProviderBindingId,
        ); 2],
    ) -> nara_reflect::ComponentRegistrySnapshot {
        let mut registry = ComponentRegistry::new();
        for (owner, provider, binding) in contributions {
            empty_provider(owner, provider, binding)
                .register_or_validate_into(&mut registry)
                .unwrap();
        }
        registry.freeze().unwrap();
        registry.snapshot().unwrap()
    }

    let first = snapshot([
        (OWNER_A, PROVIDER_P, PROVIDER_P_BINDING),
        (OWNER_B, PROVIDER_Q, PROVIDER_Q_BINDING),
    ]);
    let swapped = snapshot([
        (OWNER_B, PROVIDER_P, PROVIDER_P_BINDING),
        (OWNER_A, PROVIDER_Q, PROVIDER_Q_BINDING),
    ]);

    assert_eq!(
        first.schema_composition_fingerprint().unwrap(),
        swapped.schema_composition_fingerprint().unwrap(),
    );
    assert_eq!(
        first
            .owner_receipts()
            .map(|receipt| receipt.owner())
            .collect::<Vec<_>>(),
        swapped
            .owner_receipts()
            .map(|receipt| receipt.owner())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        first
            .provider_receipts()
            .map(|receipt| receipt.provider())
            .collect::<Vec<_>>(),
        swapped
            .provider_receipts()
            .map(|receipt| receipt.provider())
            .collect::<Vec<_>>(),
    );
    assert_ne!(
        first.executable_registry_fingerprint().unwrap(),
        swapped.executable_registry_fingerprint().unwrap(),
    );
}

#[test]
fn direct_registry_rejects_a_known_cross_owner_claim_before_its_callback() {
    let mut registry = ComponentRegistry::new();
    ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        position_catalog_source,
        register_position_provider,
    )
    .register_or_validate_into(&mut registry)
    .unwrap();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            ComponentSchemaOwnerId::new("nara.test.conflicting-owner"),
            PluginSchemaProviderId::new("nara.test.conflicting-provider"),
            ComponentSchemaProviderBindingId::new("nara.test.conflicting-provider.native", 1),
            position_catalog_source,
            panic_if_provider_callback_runs,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::DuplicateComponentId(component_id))
            if component_id == ComponentTypeId::new("nara.test.Position")
    ));
}

#[test]
fn owner_candidate_rejects_nested_provider_registration_before_source_loading() {
    let mut registry = ComponentRegistry::new();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            empty_catalog_source,
            register_nested_provider,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::NestedSchemaProviderRegistration { provider })
            if provider == PluginSchemaProviderId::new("nara.test.nested-provider")
    ));
    assert_eq!(
        registry.catalog_candidate(),
        &ComponentSchemaCatalog::default(),
    );
}

#[test]
fn owner_candidate_rejects_registry_replacement_without_publishing_smuggled_receipts() {
    let mut registry = ComponentRegistry::new();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            empty_catalog_source,
            replace_candidate_and_register_nested_provider,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::SchemaProviderCandidateAuthorityChanged { provider })
            if provider == TEST_PROVIDER_ID
    ));
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.owner_receipts().count(), 0);
    assert_eq!(snapshot.provider_receipts().count(), 0);
}

#[test]
fn provider_error_after_candidate_mutation_preserves_the_aggregate_registry() {
    let mut registry = ComponentRegistry::new();
    let before = registry.catalog_candidate().clone();

    let Err(error) = ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        position_catalog_source,
        register_position_then_reject,
    )
    .register_or_validate_into(&mut registry) else {
        panic!("the rejected private candidate unexpectedly merged");
    };
    assert_eq!(error, ComponentRegistryError::Frozen);
    assert_eq!(registry.catalog_candidate(), &before);

    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.catalog().components().len(), 0);
    assert_eq!(snapshot.provider_receipts().count(), 0);
    assert_eq!(snapshot.owner_receipts().count(), 0);
}

#[test]
fn provider_panic_after_candidate_mutation_preserves_the_aggregate_registry() {
    let mut registry = ComponentRegistry::new();
    let before = registry.catalog_candidate().clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            position_catalog_source,
            register_position_then_panic,
        )
        .register_or_validate_into(&mut registry)
        .unwrap();
    }));

    assert!(outcome.is_err());
    assert_eq!(registry.catalog_candidate(), &before);
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.catalog().components().len(), 0);
    assert_eq!(snapshot.provider_receipts().count(), 0);
    assert_eq!(snapshot.owner_receipts().count(), 0);
}

#[test]
fn raw_owner_local_and_provider_composition_modes_cannot_be_mixed() {
    let mut raw = ComponentRegistry::new();
    raw.register_component_schema(position_schema("x", "Position X"))
        .unwrap();
    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            panic_if_provider_source_runs,
            panic_if_provider_callback_runs,
        )
        .register_or_validate_into(&mut raw),
        Err(ComponentRegistryError::MixedSchemaRegistrationModes { provider })
            if provider == TEST_PROVIDER_ID
    ));

    let mut composed = ComponentRegistry::new();
    empty_provider(TEST_OWNER_ID, TEST_PROVIDER_ID, TEST_PROVIDER_BINDING)
        .register_or_validate_into(&mut composed)
        .unwrap();
    assert!(matches!(
        composed.register_component_schema(position_schema("x", "Position X")),
        Err(ComponentRegistryError::RawSchemaRegistrationInProviderComposition)
    ));
}

#[test]
fn raw_owner_local_snapshot_cannot_publish_product_fingerprints() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();

    assert!(matches!(
        snapshot.schema_composition_fingerprint(),
        Err(ComponentRegistryError::SchemaCompositionUnavailable)
    ));
    assert!(matches!(
        snapshot.executable_registry_fingerprint(),
        Err(ComponentRegistryError::SchemaCompositionUnavailable)
    ));
    assert_eq!(snapshot.contribution_receipts().count(), 0);
    assert_eq!(snapshot.provider_receipts().count(), 0);
    assert_eq!(snapshot.owner_receipts().count(), 0);
}

#[test]
fn rejected_raw_registration_does_not_poison_an_undecided_registry() {
    let mut registry = ComponentRegistry::new();
    let unknown = ComponentTypeId::new("nara.test.Unknown");
    assert!(matches!(
        registry.register_native_component_with_codec::<Position, _, _>(
            &unknown,
            |_| {
                Ok(Position {
                    x: 0.0,
                    debug_label: String::new(),
                })
            },
            |_| Ok(ComponentValue::Null),
        ),
        Err(ComponentRegistryError::UnknownComponentId(component_id)) if component_id == unknown
    ));

    empty_provider(TEST_OWNER_ID, TEST_PROVIDER_ID, TEST_PROVIDER_BINDING)
        .register_or_validate_into(&mut registry)
        .unwrap();
}

#[test]
fn declared_owner_head_must_match_the_frozen_executable_candidate() {
    let mut registry = ComponentRegistry::new();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            empty_catalog_source,
            |registry| {
                let schema = position_schema("x", "Position X");
                let id = schema.id().clone();
                registry.register_component_schema(schema)?;
                register_position_binding(registry, &id)
            },
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::SchemaProviderCatalogMismatch { provider })
            if provider == TEST_PROVIDER_ID
    ));
    assert_eq!(
        registry.catalog_candidate(),
        &ComponentSchemaCatalog::default()
    );
}

#[test]
fn semantic_and_executable_registry_fingerprints_are_distinct_domains() {
    fn snapshot(
        owner: ComponentSchemaOwnerId,
        binding: ComponentSchemaProviderBindingId,
    ) -> nara_reflect::ComponentRegistrySnapshot {
        let mut registry = ComponentRegistry::new();
        ComponentSchemaProviderDefinition::new(
            owner,
            TEST_PROVIDER_ID,
            binding,
            empty_catalog_source,
            register_empty_provider,
        )
        .register_or_validate_into(&mut registry)
        .unwrap();
        registry.freeze().unwrap();
        registry.snapshot().unwrap()
    }

    let first = snapshot(TEST_OWNER_ID, TEST_PROVIDER_BINDING);
    let changed_binding = snapshot(TEST_OWNER_ID, TEST_PROVIDER_BINDING.with_codec_version(2));
    let changed_owner = snapshot(
        ComponentSchemaOwnerId::new("nara.test.other-owner"),
        TEST_PROVIDER_BINDING,
    );

    assert_eq!(
        first.schema_composition_fingerprint().unwrap(),
        changed_binding.schema_composition_fingerprint().unwrap()
    );
    assert_ne!(
        first.executable_registry_fingerprint().unwrap(),
        changed_binding.executable_registry_fingerprint().unwrap()
    );
    assert_ne!(
        first.schema_composition_fingerprint().unwrap(),
        changed_owner.schema_composition_fingerprint().unwrap()
    );
    assert_eq!(first.owner_receipts().count(), 1);
}

#[test]
fn equal_runtime_fingerprints_do_not_replace_owner_lineage_receipts() {
    fn snapshot(
        provider: ComponentSchemaProviderDefinition,
    ) -> nara_reflect::ComponentRegistrySnapshot {
        let mut registry = ComponentRegistry::new();
        provider.register_or_validate_into(&mut registry).unwrap();
        registry.freeze().unwrap();
        registry.snapshot().unwrap()
    }

    let initial = snapshot(ComponentSchemaProviderDefinition::new(
        TEST_OWNER_ID,
        TEST_PROVIDER_ID,
        TEST_PROVIDER_BINDING,
        empty_catalog_source,
        register_empty_provider,
    ));
    let successor = snapshot(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            empty_successor_catalog_source,
            register_empty_provider,
        )
        .with_predecessor(empty_catalog_source),
    );

    assert_eq!(
        initial.schema_composition_fingerprint().unwrap(),
        successor.schema_composition_fingerprint().unwrap(),
    );
    assert_eq!(
        initial.executable_registry_fingerprint().unwrap(),
        successor.executable_registry_fingerprint().unwrap(),
    );
    assert_ne!(
        initial.owner_receipts().collect::<Vec<_>>(),
        successor.owner_receipts().collect::<Vec<_>>(),
    );
}

#[test]
fn schema_source_error_codes_are_bounded_public_identifiers() {
    let mut registry = ComponentRegistry::new();

    assert!(matches!(
        ComponentSchemaProviderDefinition::new(
            TEST_OWNER_ID,
            TEST_PROVIDER_ID,
            TEST_PROVIDER_BINDING,
            invalid_source_error_code,
            register_empty_provider,
        )
        .register_or_validate_into(&mut registry),
        Err(ComponentRegistryError::SchemaProviderSourceRejected { source, .. })
            if source.code() == "schema-provider-source-invalid-error-code"
    ));
    assert_eq!(
        registry.catalog_candidate(),
        &ComponentSchemaCatalog::default(),
    );
}

#[test]
fn building_registry_does_not_expose_runtime_query_surface() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();

    assert!(registry.schema(&id).is_none());
    assert!(matches!(
        registry.schemas(),
        Err(ComponentRegistryError::NotFrozen)
    ));
    assert!(
        registry
            .resolve_field(&id, &ComponentFieldId::new("position.x"))
            .is_none()
    );
    assert!(registry.schema_for_type::<Position>().is_none());
    assert!(registry.native_rust_type_path(&id).is_none());
    assert!(matches!(
        registry.type_registry(),
        Err(ComponentRegistryError::NotFrozen)
    ));

    registry.freeze().unwrap();
    assert!(registry.schema(&id).is_some());
    assert_eq!(registry.schemas().unwrap().count(), 1);
    assert!(registry.schema_for_type::<Position>().is_some());
    assert!(registry.native_rust_type_path(&id).is_some());
    assert!(registry.type_registry().is_ok());
}

#[test]
fn building_registry_cannot_prepare_an_applicable_component() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    let value = ComponentValue::map([("x", ComponentValue::f64(1.0).unwrap())]);

    assert!(registry.preflight_component(&id, &value).is_none());

    registry.freeze().unwrap();
    assert!(registry.preflight_component(&id, &value).is_some());
}

#[test]
fn catalog_generation_exhaustion_is_explicit() {
    let lineage_root = ComponentSchemaCatalog::default();
    let predecessor = ComponentSchemaCatalog {
        generation: u64::MAX,
        predecessor: Some(lineage_root.fingerprint()),
        ..ComponentSchemaCatalog::default()
    };

    assert!(matches!(
        ComponentSchemaCatalog::successor_of(&predecessor),
        Err(error) if error.generation() == u64::MAX
    ));
    assert!(matches!(
        ComponentRegistry::from_owner_catalog_candidate(
            TEST_OWNER_ID,
            predecessor.clone(),
            Some(predecessor),
        ),
        Err(ComponentRegistryError::CatalogGenerationExhausted {
            generation: u64::MAX
        })
    ));
}

#[test]
fn frozen_registry_rejects_all_mutations_without_changing_snapshot() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    let catalog = registry.catalog().unwrap().clone();

    let velocity_id = ComponentTypeId::new("nara.test.Velocity");
    let velocity_schema = ComponentSchema::new(
        velocity_id.clone(),
        "Velocity",
        ComponentSchemaVersion::new(1).unwrap(),
    )
    .with_capabilities(authoring_capabilities())
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("velocity.dx"),
        "Velocity X",
        ComponentFieldPath::from_fields(["dx"]),
        ComponentValueKind::F64,
    )
    .with_capabilities(authoring_capabilities())]);

    assert!(matches!(
        registry.register_component_schema(velocity_schema),
        Err(ComponentRegistryError::Frozen)
    ));
    assert!(matches!(
        registry.register_native_component_with_codec::<Velocity, _, _>(
            &velocity_id,
            |_value| Ok(Velocity { dx: 0.0 }),
            |velocity| Ok(ComponentValue::f64(f64::from(velocity.dx))?),
        ),
        Err(ComponentRegistryError::Frozen)
    ));
    assert!(matches!(
        registry.register_component_migration(
            &id,
            ComponentSchemaVersion::new(1).unwrap(),
            ComponentSchemaVersion::new(2).unwrap(),
            Ok,
        ),
        Err(ComponentRegistryError::Frozen)
    ));
    assert!(matches!(
        registry.declare_type_tombstone(ComponentTypeId::new("nara.test.Removed")),
        Err(ComponentRegistryError::Frozen)
    ));

    assert_eq!(registry.catalog().unwrap(), &catalog);
    assert!(snapshot.ptr_eq(&registry.snapshot().unwrap()));
}

#[test]
fn catalog_lineage_requires_tombstones_and_never_allows_reactivation() {
    let mut first = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    first.register_component_schema(schema).unwrap();
    register_position_binding(&mut first, &id).unwrap();
    first.freeze().unwrap();
    let predecessor = first.catalog().unwrap().clone();

    let renamed_schema = position_schema("x", "Horizontal position");
    let mut renamed = owner_successor_candidate(predecessor.clone(), [renamed_schema]).unwrap();
    register_position_binding(&mut renamed, &id).unwrap();
    renamed.freeze().unwrap();
    assert_eq!(
        renamed
            .resolve_field(&id, &ComponentFieldId::new("position.x"))
            .unwrap()
            .path()
            .to_string(),
        "x"
    );

    let schema_without_x = ComponentSchema::new(
        id.clone(),
        "Position",
        ComponentSchemaVersion::new(2).unwrap(),
    )
    .with_capabilities(authoring_capabilities())
    .with_fields([ComponentFieldSchema::optional(
        ComponentFieldId::new("position.debug_label"),
        "Debug label",
        ComponentFieldPath::from_fields(["debug_label"]),
        ComponentValueKind::String,
    )
    .with_capabilities([ComponentCapability::Inspect])]);
    assert!(matches!(
        owner_successor_candidate(predecessor.clone(), [schema_without_x.clone()]),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("position.x")
    ));

    let removed_schema =
        schema_without_x.with_field_tombstones([ComponentFieldId::new("position.x")]);
    let mut removed = owner_successor_candidate(predecessor, [removed_schema]).unwrap();
    register_position_binding(&mut removed, &id).unwrap();
    removed
        .register_component_migration(
            &id,
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            Ok,
        )
        .unwrap();
    removed.freeze().unwrap();

    assert!(matches!(
        owner_successor_candidate(
            removed.catalog().unwrap().clone(),
            [position_schema("horizontal", "Horizontal position")],
        ),
        Err(ComponentRegistryError::ReactivatedFieldId { field_id, .. })
            if field_id == ComponentFieldId::new("position.x")
    ));
}

#[test]
fn catalog_fingerprint_distinguishes_nested_list_and_map_boundaries() {
    fn catalog_with_default(default_value: ComponentValue) -> ComponentSchemaCatalog {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("nara.test.Fingerprint"),
            "Fingerprint",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::empty(),
            default_value.kind(),
            default_value,
        )]);
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        }
    }

    let list_a = ComponentValue::List(vec![ComponentValue::List(Vec::new()), ComponentValue::Null]);
    let list_b = ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]);
    let map_a = ComponentValue::map([
        ("a", ComponentValue::Map(Default::default())),
        ("b", ComponentValue::Null),
    ]);
    let map_b = ComponentValue::map([("a", ComponentValue::map([("b", ComponentValue::Null)]))]);

    assert_ne!(list_a, list_b);
    assert_ne!(map_a, map_b);
    assert_ne!(
        catalog_with_default(list_a).fingerprint(),
        catalog_with_default(list_b).fingerprint()
    );
    assert_ne!(
        catalog_with_default(map_a).fingerprint(),
        catalog_with_default(map_b).fingerprint()
    );
}

#[test]
fn catalog_lineage_rejects_same_version_semantic_drift() {
    let predecessor = frozen_position_catalog(position_schema("x", "Position X"));
    let mut changed_schemas = Vec::new();

    let mut changed_path = position_schema("x", "Position X");
    changed_path.fields[0].path = ComponentFieldPath::from_fields(["horizontal"]);
    changed_schemas.push(changed_path);

    let mut changed_kind = position_schema("x", "Position X");
    changed_kind.fields[0].value_kind = ComponentValueKind::I64;
    changed_schemas.push(changed_kind);

    let mut changed_default = position_schema("x", "Position X");
    changed_default.fields[0].default_value = Some(ComponentValue::f64(0.0).unwrap());
    changed_schemas.push(changed_default);

    let mut changed_field_capability = position_schema("x", "Position X");
    changed_field_capability.fields[0]
        .capabilities
        .remove(&ComponentCapability::Edit);
    changed_schemas.push(changed_field_capability);

    let mut changed_component_capability = position_schema("x", "Position X");
    changed_component_capability
        .capabilities
        .remove(&ComponentCapability::Edit);
    for field in &mut changed_component_capability.fields {
        field.capabilities.remove(&ComponentCapability::Edit);
    }
    changed_schemas.push(changed_component_capability);

    let mut added_field = position_schema("x", "Position X");
    added_field.fields.push(
        ComponentFieldSchema::optional(
            ComponentFieldId::new("position.extra"),
            "Extra",
            ComponentFieldPath::from_fields(["extra"]),
            ComponentValueKind::Bool,
        )
        .with_capabilities(authoring_capabilities()),
    );
    changed_schemas.push(added_field);

    for schema in changed_schemas {
        let id = schema.id().clone();
        assert!(matches!(
            owner_successor_candidate(predecessor.clone(), [schema]),
            Err(ComponentRegistryError::ComponentSchemaChangedWithoutVersionBump {
                component_id,
            }) if component_id == id
        ));
    }
}

#[test]
fn catalog_lineage_rejects_component_schema_version_regression() {
    let v2 = ComponentSchemaVersion::new(2).unwrap();
    let predecessor = frozen_position_catalog(position_schema_at_version("x", "Position X", v2));
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    assert!(matches!(
        owner_successor_candidate(predecessor, [schema]),
        Err(ComponentRegistryError::ComponentSchemaVersionRegressed {
            component_id,
            previous: ComponentSchemaVersion(2),
            current: ComponentSchemaVersion(1),
        }) if component_id == id
    ));
}

#[test]
fn successor_freeze_requires_a_complete_predecessor_migration_chain() {
    let predecessor = frozen_position_catalog(position_schema("x", "Position X"));
    let v2 = ComponentSchemaVersion::new(2).unwrap();
    let v3 = ComponentSchemaVersion::new(3).unwrap();
    let schema = position_schema_at_version("x", "Position X", v3);
    let id = schema.id().clone();
    let mut registry = owner_successor_candidate(predecessor, [schema]).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    registry
        .register_component_migration(&id, ComponentSchemaVersion::ONE, v2, Ok)
        .unwrap();

    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::MissingComponentMigrationChain {
            component_id,
            from_version: ComponentSchemaVersion(2),
            target_version: ComponentSchemaVersion(3),
        }) if component_id == id
    ));
    assert!(!registry.is_frozen());

    registry
        .register_component_migration(&id, v2, v3, Ok)
        .unwrap();
    registry.freeze().unwrap();
}

#[test]
fn schema_paths_must_not_overlap() {
    let id = ComponentTypeId::new("nara.test.Overlapping");
    let schema = ComponentSchema::new(id.clone(), "Overlapping", ComponentSchemaVersion::ONE)
        .with_capabilities(authoring_capabilities())
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("settings"),
                "Settings",
                ComponentFieldPath::from_fields(["settings"]),
                ComponentValueKind::Map,
            )
            .with_capabilities(authoring_capabilities()),
            ComponentFieldSchema::required(
                ComponentFieldId::new("settings.secret"),
                "Secret",
                ComponentFieldPath::from_fields(["settings", "secret"]),
                ComponentValueKind::String,
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect]),
        ]);
    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(schema).unwrap();
    registry
        .register_native_component_with_codec::<Position, _, _>(
            &id,
            |_value| {
                Ok(Position {
                    x: 0.0,
                    debug_label: String::new(),
                })
            },
            |_position| Ok(ComponentValue::Map(Default::default())),
        )
        .unwrap();

    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::OverlappingComponentFieldPaths { component_id, .. })
            if component_id == id
    ));
}

#[test]
fn asset_and_entity_reference_capabilities_are_field_only() {
    for capability in [
        ComponentCapability::AssetRef,
        ComponentCapability::EntityRef,
    ] {
        let schema = position_schema("x", "Position X")
            .with_capabilities(authoring_capabilities().into_iter().chain([capability]));
        let id = schema.id().clone();
        let mut registry = ComponentRegistry::new();
        registry.register_component_schema(schema).unwrap();
        register_position_binding(&mut registry, &id).unwrap();

        assert!(matches!(
            registry.freeze(),
            Err(ComponentRegistryError::InvalidComponentCapability {
                component_id,
                capability: actual,
            }) if component_id == id && actual == capability
        ));
        assert!(!registry.is_frozen());
    }
}

#[test]
fn schema_coverage_rejects_unregistered_payload_and_codec_fields() {
    let id = ComponentTypeId::new("nara.test.StrictPosition");
    let schema = ComponentSchema::new(id.clone(), "Strict position", ComponentSchemaVersion::ONE)
        .with_capabilities(authoring_capabilities())
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("x"),
            "X",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities())]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<Position, _, _>(
            schema,
            |value| {
                Ok(Position {
                    x: value.field_f64("x")? as f32,
                    debug_label: String::new(),
                })
            },
            |position| {
                Ok(ComponentValue::map([
                    ("x", ComponentValue::f64(f64::from(position.x))?),
                    (
                        "ignored",
                        ComponentValue::String(position.debug_label.clone()),
                    ),
                ]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();

    let unexpected_input = ComponentValue::map([
        ("x", ComponentValue::f64(1.0).unwrap()),
        ("ignored", ComponentValue::String("lost".to_owned())),
    ]);
    let input_error = registry
        .preflight_component(&id, &unexpected_input)
        .unwrap()
        .err()
        .expect("unexpected schema field must fail preflight");
    assert!(input_error.to_string().contains("not declared by schema"));

    let mut world = World::new();
    let entity = world
        .spawn(Position {
            x: 1.0,
            debug_label: "lost".to_owned(),
        })
        .id();
    let output_error = registry
        .encode_component(&id, &world, entity)
        .unwrap()
        .unwrap_err();
    assert!(output_error.to_string().contains("not declared by schema"));
}

#[test]
fn schema_coverage_preserves_declared_container_and_nested_field_semantics() {
    let id = ComponentTypeId::new("nara.test.CoverageProbe");
    let schema = ComponentSchema::new(id.clone(), "Coverage probe", ComponentSchemaVersion::ONE)
        .with_capabilities(authoring_capabilities())
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("opaque"),
                "Opaque",
                ComponentFieldPath::from_fields(["opaque"]),
                ComponentValueKind::Map,
            )
            .with_capabilities(authoring_capabilities()),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("enabled"),
                "Enabled",
                ComponentFieldPath::from_fields(["settings", "enabled"]),
                ComponentValueKind::Bool,
            )
            .with_capabilities(authoring_capabilities()),
        ]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<CoverageProbe, _, _>(
            schema,
            |_value| Ok(CoverageProbe),
            |_probe| {
                Ok(ComponentValue::map([
                    (
                        "opaque",
                        ComponentValue::map([("arbitrary", ComponentValue::Bool(true))]),
                    ),
                    ("settings", ComponentValue::Map(Default::default())),
                ]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();

    let valid = ComponentValue::map([
        (
            "opaque",
            ComponentValue::map([("arbitrary", ComponentValue::Bool(true))]),
        ),
        ("settings", ComponentValue::Map(Default::default())),
    ]);
    assert!(registry.preflight_component(&id, &valid).unwrap().is_ok());

    let undeclared_sibling = ComponentValue::map([
        ("opaque", ComponentValue::Map(Default::default())),
        (
            "settings",
            ComponentValue::map([("undeclared", ComponentValue::Bool(true))]),
        ),
    ]);
    let error = registry
        .preflight_component(&id, &undeclared_sibling)
        .unwrap()
        .err()
        .expect("undeclared nested sibling must be rejected");
    assert!(error.to_string().contains("settings.undeclared"));

    let scalar_ancestor = ComponentValue::map([
        ("opaque", ComponentValue::Map(Default::default())),
        ("settings", ComponentValue::Bool(true)),
    ]);
    assert!(
        registry
            .preflight_component(&id, &scalar_ancestor)
            .unwrap()
            .is_err()
    );
}

#[test]
fn mixed_capability_whole_value_requires_an_explicit_projection() {
    let mut registry = ComponentRegistry::new();
    let schema = position_schema("x", "Position X");
    let id = schema.id().clone();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    registry.freeze().unwrap();

    let error = registry
        .validate_whole_value_capabilities(
            &id,
            [ComponentCapability::Scene, ComponentCapability::Edit],
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ComponentProjectionError::ProjectionRequired { field_id, .. }
            if field_id == ComponentFieldId::new("position.debug_label")
    ));
}

#[test]
fn runtime_only_components_need_no_schema_or_reflection_registration() {
    #[derive(Component)]
    struct RuntimeOnly(u32);

    let mut world = World::new();
    let entity = world.spawn(RuntimeOnly(7)).id();
    assert_eq!(
        world.get::<RuntimeOnly>(entity).map(|value| value.0),
        Some(7)
    );

    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    assert!(registry.catalog().unwrap().components().is_empty());
}

#[test]
fn duplicate_field_identity_and_locator_fail_before_snapshot_publication() {
    let id = ComponentTypeId::new("nara.test.Position");
    let duplicate_id = ComponentFieldId::new("position.x");
    let duplicate_path = ComponentFieldPath::from_fields(["x"]);
    let schema = ComponentSchema::new(
        id.clone(),
        "Position",
        ComponentSchemaVersion::new(1).unwrap(),
    )
    .with_capabilities(authoring_capabilities())
    .with_fields([
        ComponentFieldSchema::required(
            duplicate_id.clone(),
            "X",
            duplicate_path.clone(),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities()),
        ComponentFieldSchema::required(
            duplicate_id.clone(),
            "Other X",
            ComponentFieldPath::from_fields(["other_x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities()),
    ]);

    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::DuplicateComponentFieldId { field_id, .. })
            if field_id == duplicate_id
    ));
    assert!(!registry.is_frozen());

    let schema = ComponentSchema::new(
        id.clone(),
        "Position",
        ComponentSchemaVersion::new(1).unwrap(),
    )
    .with_capabilities(authoring_capabilities())
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("position.x"),
            "X",
            duplicate_path.clone(),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities()),
        ComponentFieldSchema::required(
            ComponentFieldId::new("position.other_x"),
            "Other X",
            duplicate_path.clone(),
            ComponentValueKind::F64,
        )
        .with_capabilities(authoring_capabilities()),
    ]);
    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(schema).unwrap();
    register_position_binding(&mut registry, &id).unwrap();
    assert!(matches!(
        registry.freeze(),
        Err(ComponentRegistryError::DuplicateComponentFieldPath { path, .. })
            if path == duplicate_path
    ));
}

fn _codec_error_is_send_sync(error: ComponentCodecError) -> ComponentCodecError {
    error
}
