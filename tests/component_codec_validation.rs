#![cfg(feature = "runtime-core")]

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara::{
    app::PluginSchemaProviderId,
    core::ItemLimit,
    diagnostic::{DiagnosticCode, SafeSummary},
    ecs::{Component, World},
    identity::{AuthoredEntityReference, EntityReference, ReferenceInputId},
    reflect::{
        __private::{
            diagnose_component_with_context, freeze_component_defaults,
            preflight_admitted_component_with_context, validate_admitted_component_with_context,
        },
        ComponentCapability, ComponentCodecError, ComponentDecodeContext, ComponentDomainError,
        ComponentEncodeContext, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        ComponentValueMaterializationLimits, ComponentValueRef, PreparedComponentCandidate,
    },
};

const COMPONENT: &str = "test.CodecValidation";

#[derive(Component)]
struct Count(i64);

fn schema(id: &str) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(id), id, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("count"),
                "Count",
                ComponentFieldPath::from_fields(["count"]),
                ComponentValueKind::I64,
                ComponentValue::I64(7),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ComponentFieldSchema::optional(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueKind::Map,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
}

fn value(count: i64) -> ComponentValue {
    ComponentValue::map([("count", ComponentValue::I64(count))])
}

fn encode(
    count: &Count,
    _: &World,
    _: &ComponentEncodeContext<'_>,
) -> Result<ComponentValue, ComponentCodecError> {
    Ok(value(count.0))
}

fn domain_error() -> ComponentCodecError {
    ComponentDomainError::new(
        DiagnosticCode::new("test.count.negative").unwrap(),
        SafeSummary::new("Count must be nonnegative").unwrap(),
    )
    .with_field(ComponentFieldId::new("count"))
    .into()
}

fn nonnegative(value: ComponentValueRef<'_>) -> Result<i64, ComponentCodecError> {
    let count = value.field_i64("count")?;
    if count < 0 {
        return Err(domain_error());
    }
    Ok(count)
}

#[test]
fn fallback_validation_preserves_immediate_and_scene_batch_candidate_lifetimes() {
    #[derive(Component)]
    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut registry = ComponentRegistryBuilder::new();
    let preflight_calls = calls.clone();
    let candidate_drops = drops.clone();
    registry
        .register_persistent_component_codec_with_context::<DropProbe, _, _>(
            schema(COMPONENT),
            move |value, _| {
                preflight_calls.fetch_add(1, Ordering::SeqCst);
                nonnegative(value)?;
                Ok(PreparedComponentCandidate::insert(DropProbe(
                    candidate_drops.clone(),
                )))
            },
            |_, _, _| Ok(value(0)),
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let id = ComponentTypeId::new(COMPONENT);
    let valid = value(1);
    let invalid = value(-1);
    let mut context = ComponentDecodeContext::new();

    registry
        .validate_component_with_context(&id, &valid, &mut context)
        .unwrap();
    assert_eq!(
        (calls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst)),
        (1, 1),
    );
    let admitted =
        validate_admitted_component_with_context(&registry, &id, (&valid).into(), &mut context)
            .unwrap()
            .expect("fallback must retain the original bound candidate");
    let diagnosed = diagnose_component_with_context(&registry, &id, (&valid).into(), &mut context)
        .unwrap()
        .expect("fresh admission must retain the original bound candidate");
    assert_eq!(
        (calls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst)),
        (3, 1),
    );
    assert_eq!(
        registry.validate_component_with_context(&id, &invalid, &mut context),
        Err(domain_error()),
    );
    assert_eq!(
        validate_admitted_component_with_context(&registry, &id, (&invalid).into(), &mut context)
            .err(),
        Some(domain_error()),
    );
    assert_eq!(
        diagnose_component_with_context(&registry, &id, (&invalid).into(), &mut context).err(),
        Some(domain_error()),
    );
    assert_eq!(
        (calls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst)),
        (6, 1),
    );
    let prepared = registry
        .preflight_component_with_context(&id, &valid, &mut context)
        .unwrap();
    assert_eq!(
        (calls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst)),
        (7, 1),
    );
    drop(prepared);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    drop((admitted, diagnosed));
    assert_eq!(drops.load(Ordering::SeqCst), 4);
}

#[test]
fn explicit_validation_is_separate_from_preparation_and_preserves_domain_errors() {
    let validations = Arc::new(AtomicUsize::new(0));
    let preflights = Arc::new(AtomicUsize::new(0));
    let applications = Arc::new(AtomicUsize::new(0));
    let mut registry = ComponentRegistryBuilder::new();
    let validate_calls = validations.clone();
    let preflight_calls = preflights.clone();
    let apply_calls = applications.clone();
    registry
        .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
            schema(COMPONENT),
            move |value, _| {
                validate_calls.fetch_add(1, Ordering::SeqCst);
                nonnegative(value).map(|_| ())
            },
            move |value, _| {
                preflight_calls.fetch_add(1, Ordering::SeqCst);
                let count = nonnegative(value)?;
                let apply_calls = apply_calls.clone();
                Ok(PreparedComponentCandidate::deferred(move || {
                    apply_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Count(count))
                }))
            },
            encode,
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let id = ComponentTypeId::new(COMPONENT);
    let valid = value(1);
    let invalid = value(-1);
    let mut context = ComponentDecodeContext::new();

    registry
        .validate_component_with_context(&id, &valid, &mut context)
        .unwrap();
    assert!(
        validate_admitted_component_with_context(&registry, &id, (&valid).into(), &mut context)
            .unwrap()
            .is_none()
    );
    assert!(
        diagnose_component_with_context(&registry, &id, (&valid).into(), &mut context)
            .unwrap()
            .is_none()
    );
    assert_eq!(validations.load(Ordering::SeqCst), 3);
    assert_eq!(preflights.load(Ordering::SeqCst), 0);
    assert_eq!(
        registry.validate_component_with_context(&id, &invalid, &mut context),
        Err(domain_error()),
    );
    assert_eq!(
        validate_admitted_component_with_context(&registry, &id, (&invalid).into(), &mut context)
            .err(),
        Some(domain_error()),
    );
    assert_eq!(
        diagnose_component_with_context(&registry, &id, (&invalid).into(), &mut context).err(),
        Some(domain_error()),
    );
    drop(
        registry
            .preflight_component_with_context(&id, &valid, &mut context)
            .unwrap(),
    );
    assert_eq!(
        registry
            .preflight_component_with_context(&id, &invalid, &mut context)
            .err(),
        Some(domain_error()),
    );
    drop(
        preflight_admitted_component_with_context(&registry, &id, (&valid).into(), &mut context)
            .unwrap(),
    );
    assert_eq!(
        preflight_admitted_component_with_context(&registry, &id, (&invalid).into(), &mut context)
            .err(),
        Some(domain_error()),
    );
    assert_eq!(validations.load(Ordering::SeqCst), 6);
    assert_eq!(preflights.load(Ordering::SeqCst), 4);
    assert_eq!(applications.load(Ordering::SeqCst), 0);
}

#[test]
fn validation_and_preflight_share_borrowed_defaults_and_admission_refusals() {
    let schema = schema(COMPONENT);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let validation_seen = seen.clone();
    let preflight_seen = seen.clone();
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
            schema.clone(),
            move |value, _| {
                validation_seen.lock().unwrap().push((
                    value.field_i64("count")?,
                    value.field("payload")?.as_value().unwrap() as *const ComponentValue as usize,
                ));
                Ok(())
            },
            move |value, _| {
                let count = value.field_i64("count")?;
                preflight_seen.lock().unwrap().push((
                    count,
                    value.field("payload")?.as_value().unwrap() as *const ComponentValue as usize,
                ));
                Ok(PreparedComponentCandidate::insert(Count(count)))
            },
            encode,
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let id = ComponentTypeId::new(COMPONENT);
    let payload = ComponentValue::map([("text", ComponentValue::String("borrowed".into()))]);
    let missing = ComponentValue::map([("payload", payload.clone())]);
    let explicit = ComponentValue::map([("count", ComponentValue::I64(9)), ("payload", payload)]);
    let defaults = freeze_component_defaults(
        &schema,
        &missing,
        &BTreeMap::from([(ComponentFieldId::new("count"), ComponentValue::I64(11))]),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let mut context = ComponentDecodeContext::new();

    for (source, read, count) in [
        (&explicit, ComponentValueRef::from(&explicit), 9),
        (&missing, ComponentValueRef::from(&missing), 7),
        (&missing, defaults.read(&missing), 11),
    ] {
        registry
            .validate_component_with_context(&id, read, &mut context)
            .unwrap();
        assert!(
            diagnose_component_with_context(&registry, &id, read, &mut context)
                .unwrap()
                .is_none()
        );
        drop(
            registry
                .preflight_component_with_context(&id, read, &mut context)
                .unwrap(),
        );
        let expected = (
            count,
            source.field("payload").unwrap() as *const ComponentValue as usize,
        );
        let seen = seen.lock().unwrap();
        assert_eq!(&seen[seen.len() - 3..], &[expected, expected, expected]);
    }
    assert!(missing.field("count").is_err());
    let hidden = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new("socket").unwrap()),
    });
    for invalid in [
        ComponentValue::String("invalid root".into()),
        ComponentValue::map([("count", ComponentValue::Bool(true))]),
        ComponentValue::map([
            ("count", ComponentValue::I64(1)),
            ("unknown", ComponentValue::Null),
        ]),
        ComponentValue::map([("payload", ComponentValue::map([("hidden", hidden)]))]),
    ] {
        let validation = registry.validate_component_with_context(&id, &invalid, &mut context);
        assert!(validation.is_err());
        let refusal = validation.err();
        assert_eq!(
            diagnose_component_with_context(&registry, &id, (&invalid).into(), &mut context).err(),
            refusal,
        );
        assert_eq!(
            registry
                .preflight_component_with_context(&id, &invalid, &mut context)
                .err(),
            refusal,
        );
    }
    context.set_materialization_limits(
        ComponentValueMaterializationLimits::default().with_nodes(ItemLimit::ONE),
    );
    let validation = registry.validate_component_with_context(&id, &explicit, &mut context);
    assert!(matches!(
        validation,
        Err(ComponentCodecError::MaterializationNodeLimit { .. }),
    ));
    let refusal = validation.err();
    assert_eq!(
        diagnose_component_with_context(&registry, &id, (&explicit).into(), &mut context).err(),
        refusal,
    );
    assert_eq!(
        registry
            .preflight_component_with_context(&id, &explicit, &mut context)
            .err(),
        refusal,
    );
    assert_eq!(seen.lock().unwrap().len(), 9);
}

#[test]
fn validation_observes_current_context_and_preserves_callback_mutations() {
    fn policy(
        _: ComponentValueRef<'_>,
        context: &mut ComponentDecodeContext<'_>,
    ) -> Result<(), ComponentCodecError> {
        if context.materialization_limits().nodes() == 16 {
            return Err(ComponentCodecError::Message(
                "current policy refused".into(),
            ));
        }
        context.set_materialization_limits(
            context
                .materialization_limits()
                .with_nodes(ItemLimit::new(16).unwrap()),
        );
        Ok(())
    }

    let validations = Arc::new(AtomicUsize::new(0));
    let validate_calls = validations.clone();
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
            schema(COMPONENT),
            move |value, context| {
                validate_calls.fetch_add(1, Ordering::SeqCst);
                policy(value, context)
            },
            |value, context| {
                policy(value, context)?;
                Ok(PreparedComponentCandidate::insert(Count(
                    value.field_i64("count")?,
                )))
            },
            encode,
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let id = ComponentTypeId::new(COMPONENT);
    let valid = value(1);
    let mut context = ComponentDecodeContext::new();
    registry
        .validate_component_with_context(&id, &valid, &mut context)
        .unwrap();
    assert_eq!(context.materialization_limits().nodes(), 16);
    let refusal = ComponentCodecError::Message("current policy refused".into());
    assert_eq!(
        registry.validate_component_with_context(&id, &valid, &mut context),
        Err(refusal.clone()),
    );
    assert_eq!(
        registry
            .preflight_component_with_context(&id, &valid, &mut context)
            .err(),
        Some(refusal),
    );
    assert_eq!(validations.load(Ordering::SeqCst), 2);
    context.set_materialization_limits(context.materialization_limits().with_nodes(ItemLimit::ONE));
    assert!(matches!(
        registry.validate_component_with_context(&id, &valid, &mut context),
        Err(ComponentCodecError::MaterializationNodeLimit { .. }),
    ));
    assert!(matches!(
        diagnose_component_with_context(&registry, &id, (&valid).into(), &mut context),
        Err(ComponentCodecError::MaterializationNodeLimit { .. }),
    ));
    assert_eq!(validations.load(Ordering::SeqCst), 2);
    let mut fresh = ComponentDecodeContext::new();
    registry
        .validate_component_with_context(&id, &valid, &mut fresh)
        .unwrap();
    assert_eq!(validations.load(Ordering::SeqCst), 3);
}

#[test]
fn invalid_registration_is_atomic_and_schema_capability_admission_precedes_callbacks() {
    let mut registry = ComponentRegistryBuilder::new();
    let before = registry.catalog_candidate().clone();
    let mut invalid = schema(COMPONENT);
    invalid.fields.push(invalid.fields[0].clone());
    assert!(
        registry
            .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
                invalid,
                |_, _| panic!("invalid registration must not run validation"),
                |_, _| panic!("invalid registration must not run preflight"),
                encode,
            )
            .is_err()
    );
    assert_eq!(registry.catalog_candidate(), &before);
    let mut inspect_only = schema(COMPONENT);
    inspect_only.capabilities = [ComponentCapability::Inspect].into();
    for field in &mut inspect_only.fields {
        field.capabilities = [ComponentCapability::Inspect].into();
    }
    registry
        .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
            inspect_only,
            |_, _| panic!("Scene capability must be admitted before validation"),
            |_, _| panic!("Scene capability must be admitted before preflight"),
            encode,
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let mut context = ComponentDecodeContext::new();
    let valid = value(1);
    let id = ComponentTypeId::new(COMPONENT);
    let validation = registry.validate_component_with_context(&id, &valid, &mut context);
    assert!(validation.is_err());
    assert_eq!(
        validation.err(),
        registry
            .preflight_component_with_context(&id, &valid, &mut context)
            .err(),
    );
    let unknown = ComponentTypeId::new("test.Unknown");
    let refusal = ComponentCodecError::UnknownComponent {
        component_id: unknown.clone(),
    };
    assert_eq!(
        registry.validate_component_with_context(&unknown, &valid, &mut context),
        Err(refusal.clone()),
    );
    assert_eq!(
        validate_admitted_component_with_context(
            &registry,
            &unknown,
            (&valid).into(),
            &mut context,
        )
        .err(),
        Some(refusal),
    );
}

#[test]
fn provider_composition_retains_validation_in_each_original_binding() {
    #[derive(Component)]
    struct OtherCount(i64);

    let first =
        ComponentSchemaProviderDefinition::new(
            ComponentSchemaOwnerId::new("test.validation.first"),
            PluginSchemaProviderId::new("test.validation.first.schema"),
            ComponentSchemaProviderBindingId::new("test.validation.first.native", 1),
            || {
                Ok(ComponentSchemaCatalog {
                    components: vec![schema(COMPONENT)],
                    ..Default::default()
                })
            },
            |registry| {
                registry
                .register_persistent_component_codec_with_context_and_validation::<Count, _, _, _>(
                    schema(COMPONENT),
                    |value, _| nonnegative(value).map(|_| ()),
                    |value, _| Ok(PreparedComponentCandidate::insert(Count(nonnegative(value)?))),
                    encode,
                )
                .map(|_| ())
            },
        );
    let second = ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("test.validation.second"),
        PluginSchemaProviderId::new("test.validation.second.schema"),
        ComponentSchemaProviderBindingId::new("test.validation.second.native", 3),
        || {
            Ok(ComponentSchemaCatalog {
                components: vec![schema("test.OtherCount")],
                ..Default::default()
            })
        },
        |registry| {
            registry
                .register_persistent_component_codec_with_context_and_validation::<
                    OtherCount,
                    _,
                    _,
                    _,
                >(
                    schema("test.OtherCount"),
                    |value, _| {
                        if value.field_i64("count")? <= 0 {
                            return Err(ComponentCodecError::invalid_field("count", "positive"));
                        }
                        Ok(())
                    },
                    |value, _| {
                        let count = value.field_i64("count")?;
                        if count <= 0 {
                            return Err(ComponentCodecError::invalid_field("count", "positive"));
                        }
                        Ok(PreparedComponentCandidate::insert(OtherCount(count)))
                    },
                    |count, _, _| Ok(value(count.0)),
                )
                .map(|_| ())
        },
    );
    let mut registry = ComponentRegistryBuilder::new();
    first.register_into(&mut registry).unwrap();
    second.register_into(&mut registry).unwrap();
    let registry = registry.freeze().unwrap();
    let retained = registry.clone();
    assert!(retained.ptr_eq(&registry));
    let fingerprint = retained.executable_registry_fingerprint().unwrap();
    let catalog = retained.catalog().clone();
    drop(registry);
    let invalid = value(-1);
    let valid = value(3);
    let mut context = ComponentDecodeContext::new();
    for (id, provider, refusal) in [
        (ComponentTypeId::new(COMPONENT), first, domain_error()),
        (
            ComponentTypeId::new("test.OtherCount"),
            second,
            ComponentCodecError::invalid_field("count", "positive"),
        ),
    ] {
        assert_eq!(retained.component_schema_owner(&id), Some(provider.owner()));
        assert_eq!(
            retained.provider_receipt(provider.id()),
            Some(provider.receipt()),
        );
        assert!(
            diagnose_component_with_context(&retained, &id, (&valid).into(), &mut context,)
                .unwrap()
                .is_none(),
        );
        assert_eq!(
            retained.validate_component_with_context(&id, &invalid, &mut context),
            Err(refusal.clone()),
        );
        assert_eq!(
            validate_admitted_component_with_context(
                &retained,
                &id,
                (&invalid).into(),
                &mut context,
            )
            .err(),
            Some(refusal.clone()),
        );
        assert_eq!(
            retained
                .preflight_component_with_context(&id, &invalid, &mut context)
                .err(),
            Some(refusal),
        );
        provider.validate_snapshot(&retained).unwrap();
    }
    assert_eq!(retained.catalog(), &catalog);
    assert_eq!(
        retained.executable_registry_fingerprint().unwrap(),
        fingerprint
    );
}
