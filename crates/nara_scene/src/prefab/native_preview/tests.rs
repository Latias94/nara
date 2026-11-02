use super::*;
use nara_app::PluginSchemaProviderId;
use nara_diagnostic::{Diagnostic, DiagnosticValueRef};
use nara_reflect::{
    __private::{register_resolved_schema_provider, resolve_schema_provider},
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
    ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    ComponentSchemaProviderSourceError, ComponentSchemaVersion, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    __private::AuthoringAdapterReceipt,
    AuthoringRecordReadiness, AuthoringSchemaBasis, PrefabInstance, ResolvedPrefabSource,
    SceneComponentRecord,
    authoring_preview::{AuthoringPreviewOmissionLocation, AuthoringPreviewOmissionReason},
};

const NATIVE_OWNER: ComponentSchemaOwnerId = ComponentSchemaOwnerId::new("test.preview-native");

#[derive(nara_ecs::Component)]
struct NativeValue(u64);

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn native_id() -> ComponentTypeId {
    ComponentTypeId::new("test.preview.Native")
}

fn data_id() -> ComponentTypeId {
    ComponentTypeId::new("test.preview.Data")
}

fn value_schema(component: ComponentTypeId, label: &str) -> ComponentSchema {
    ComponentSchema::new(component, label, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("score"),
            "Score",
            ComponentFieldPath::from_fields(["score"]),
            ComponentValueKind::U64,
            ComponentValue::U64(0),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn native_schema() -> ComponentSchema {
    value_schema(native_id(), "Native value")
}

fn changed_schema() -> ComponentSchema {
    value_schema(native_id(), "Different native declaration")
}

fn native_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![native_schema()],
        ..Default::default()
    })
}

fn changed_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![changed_schema()],
        ..Default::default()
    })
}

fn register_value(
    registry: &mut ComponentRegistryBuilder,
    schema: ComponentSchema,
) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_with_codec::<NativeValue, _, _>(
        schema,
        |value| Ok(NativeValue(value.field_u64("score")?)),
        |value| {
            Ok(ComponentValue::map([(
                "score",
                ComponentValue::U64(value.0),
            )]))
        },
    )?;
    Ok(())
}

fn register_native(registry: &mut ComponentRegistryBuilder) -> Result<(), ComponentRegistryError> {
    register_value(registry, native_schema())
}

fn register_changed(registry: &mut ComponentRegistryBuilder) -> Result<(), ComponentRegistryError> {
    register_value(registry, changed_schema())
}

fn provider(
    owner: ComponentSchemaOwnerId,
    binding: u32,
    changed: bool,
) -> ComponentSchemaProviderDefinition {
    ComponentSchemaProviderDefinition::new(
        owner,
        PluginSchemaProviderId::new("test.preview-native"),
        ComponentSchemaProviderBindingId::new("test.preview-native.binding", binding),
        if changed {
            changed_catalog
        } else {
            native_catalog
        },
        if changed {
            register_changed
        } else {
            register_native
        },
    )
}

fn registry(provider: Option<ComponentSchemaProviderDefinition>) -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
    if let Some(provider) = provider {
        register_resolved_schema_provider(
            &mut registry,
            resolve_schema_provider(provider).unwrap(),
        )
        .unwrap();
    }
    registry.freeze().unwrap()
}

fn basis(provider: ComponentSchemaProviderDefinition, include_data: bool) -> AuthoringSchemaBasis {
    let native = AuthoringSchemaBundle::from_trusted_provider_definitions([provider]).unwrap();
    let resolved = resolve_schema_provider(provider).unwrap();
    let coverage: BTreeSet<_> = resolved
        .current()
        .catalog()
        .components()
        .iter()
        .map(|schema| schema.id().clone())
        .collect();
    let mut owners: Vec<_> = native
        .owners()
        .iter()
        .map(AuthoringSchemaBundleOwner::from)
        .collect();
    if include_data {
        owners.push(AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("test.preview-data").unwrap(),
            ComponentSchemaCatalog {
                components: vec![value_schema(data_id(), "Data value")],
                ..Default::default()
            },
            None,
        ));
    }
    let bundle = AuthoringSchemaBundle::new(owners).unwrap();
    let mut adapters = vec![
        AuthoringAdapterReceipt::from_trusted_provider(
            resolved.contribution_receipt(),
            "test.native-preview",
            1,
            1,
            coverage.iter().cloned(),
        )
        .unwrap(),
    ];
    adapters.extend(
        bundle
            .owners()
            .iter()
            .filter_map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, &coverage)),
    );
    AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
}

fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("score", ComponentValue::U64(7))]),
    )
}

fn compile(basis: &AuthoringSchemaBasis, scene: &SceneDocument) -> AuthoringSceneCompilationReport {
    compile_with_limits(basis, scene, SceneCompilationLimits::default())
}

fn compile_with_limits(
    basis: &AuthoringSchemaBasis,
    scene: &SceneDocument,
    limits: SceneCompilationLimits,
) -> AuthoringSceneCompilationReport {
    try_compile_authoring_scene_with_loader(
        scene,
        basis,
        SceneCompilationOptions::default().with_limits(limits),
        None,
        |_, _| Ok::<_, Infallible>(None),
    )
    .unwrap()
}

fn assert_flat_values(
    preview: &CompiledSceneDocument,
    source: &SceneDocument,
    basis: &AuthoringSchemaBasis,
) {
    assert_eq!(preview.roots(), source.roots.as_slice());
    assert_eq!(preview.entities().len(), source.entities.len());
    assert_eq!(preview.canonical_provenance().len(), source.entities.len());
    for (compiled, source) in preview.entities().iter().zip(&source.entities) {
        assert_eq!(compiled.id(), &source.id);
        assert_eq!(compiled.children(), source.children.as_slice());
        assert_eq!(compiled.components().len(), source.components.len());
        for (component, record) in &source.components {
            let compiled = &compiled.components()[component];
            assert_eq!(compiled.version(), record.version);
            assert_eq!(compiled.value(), &record.value);
            assert_eq!(
                compiled.schema_owner(),
                basis.component_schema_owner(component)
            );
        }
    }
}

#[test]
fn installed_registry_keeps_ownerless_builtin_native_record() {
    let provider = crate::SCENE_COMPONENTS_SCHEMA_PROVIDER;
    let basis = basis(provider, false);
    let name = ComponentTypeId::new("nara.scene.Name");
    let record = SceneComponentRecord::new(
        basis.schema(&name).unwrap().version(),
        ComponentValue::String("Ownerless native source".into()),
    );
    assert!(record.owner().is_none());
    let scene = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(name, record)
    ]);
    let report = compile(&basis, &scene);
    let installed = registry(Some(provider));
    assert_eq!(
        report.checked_native_preview_work_bytes_for(&installed),
        report.checked_work_bytes(),
    );
    let AuthoringNativePreview {
        candidate: preview,
        omissions,
        ..
    } = report.into_native_preview_for(&installed).unwrap();
    assert!(omissions.is_empty());
    assert_flat_values(&preview, &scene, &basis);
}

#[test]
fn restriction_preserves_data_and_unavailable_omissions_without_duplicates() {
    let basis = basis(provider(NATIVE_OWNER, 1, false), true);
    let unknown = ComponentTypeId::new("test.preview.Unknown");
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
        .with_component(native_id(), record())
        .with_component(data_id(), record())
        .with_component(
            unknown.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserved opaque source".into()),
            ),
        )]);
    let AuthoringNativePreview {
        candidate: ordinary,
        omissions: original,
        ..
    } = compile(&basis, &scene).into_native_preview().unwrap();
    assert_eq!(ordinary.entities()[0].components().len(), 1);
    assert!(
        ordinary.entities()[0]
            .components()
            .contains_key(&native_id())
    );
    assert_eq!(original.len(), 2);
    assert!(original.iter().any(|fact| {
        fact.reason()
            == AuthoringPreviewOmissionReason::RecordUnavailable(
                AuthoringRecordReadiness::UnknownSchema,
            )
    }));

    let report = compile(&basis, &scene);
    let installed = registry(None);
    assert!(
        report
            .checked_native_preview_work_bytes_for(&installed)
            .unwrap()
            > report.checked_work_bytes().unwrap()
    );
    let AuthoringNativePreview {
        candidate: preview,
        omissions,
        ..
    } = report.into_native_preview_for(&installed).unwrap();
    assert!(preview.entities()[0].components().is_empty());
    assert_eq!(&omissions[..original.len()], original.as_slice());
    assert_eq!(omissions.len(), 3);
    assert_eq!(
        omissions.last().unwrap().location(),
        &AuthoringPreviewOmissionLocation::DirectEntityComponent {
            entity: id("root"),
            component: native_id(),
        },
    );
    assert_eq!(
        omissions.last().unwrap().reason(),
        AuthoringPreviewOmissionReason::NativeBindingUnavailable,
    );
    assert_eq!(scene.entities[0].components.len(), 3);

    // Even an actual Native registration cannot promote a source classified by the Data Adapter.
    let data_schema_provider = provider(NATIVE_OWNER, 1, false);
    let native_bundle =
        AuthoringSchemaBundle::from_trusted_provider_definitions([data_schema_provider]).unwrap();
    let data_adapter =
        AuthoringAdapterReceipt::from_schema_owner(&native_bundle.owners()[0], &BTreeSet::new())
            .unwrap();
    let data_basis = AuthoringSchemaBasis::admit(&native_bundle, &[data_adapter]).unwrap();
    let data_scene = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(native_id(), record())
    ]);
    let AuthoringNativePreview {
        candidate: preview,
        omissions,
        ..
    } = compile(&data_basis, &data_scene)
        .into_native_preview_for(&registry(Some(data_schema_provider)))
        .unwrap();
    assert!(preview.entities()[0].components().is_empty());
    assert_eq!(omissions.len(), 1);
}

#[test]
fn native_preview_requires_matching_owner_schema_and_executable_receipt() {
    let source = provider(NATIVE_OWNER, 1, false);
    let basis = basis(source, false);
    let scene = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(native_id(), record())
    ]);
    let AuthoringNativePreview {
        candidate: preview,
        omissions,
        ..
    } = compile(&basis, &scene)
        .into_native_preview_for(&registry(Some(source)))
        .unwrap();
    assert_flat_values(&preview, &scene, &basis);
    assert!(omissions.is_empty());

    let mut raw = ComponentRegistryBuilder::from_owner_catalog_candidate(
        NATIVE_OWNER,
        ComponentSchemaCatalog::default(),
        None,
    )
    .unwrap();
    register_native(&mut raw).unwrap();
    for (case, installed) in [
        (
            "owner",
            registry(Some(provider(
                ComponentSchemaOwnerId::new("test.foreign-preview-native"),
                1,
                false,
            ))),
        ),
        ("schema", registry(Some(provider(NATIVE_OWNER, 1, true)))),
        ("receipt", registry(Some(provider(NATIVE_OWNER, 2, false)))),
        ("missing-receipt", raw.freeze().unwrap()),
    ] {
        assert!(installed.native_rust_type_path(&native_id()).is_some());
        let report = compile(&basis, &scene);
        assert!(report.omissions().is_empty(), "{case}");
        let AuthoringNativePreview {
            candidate: preview,
            omissions,
            ..
        } = report.into_native_preview_for(&installed).unwrap();
        assert!(preview.entities()[0].components().is_empty(), "{case}");
        assert_eq!(omissions.len(), 1, "{case}");
        assert_eq!(
            omissions[0].reason(),
            AuthoringPreviewOmissionReason::NativeBindingUnavailable,
            "{case}",
        );
    }
}

#[test]
fn native_preview_sealing_keeps_the_original_reference_work_allowance() {
    let basis = basis(provider(NATIVE_OWNER, 1, false), false);
    let scene = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(native_id(), record())
    ]);
    let mut report = compile(&basis, &scene);
    let initial = report.reference_work;
    let preview = report.finish_native_preview(None).unwrap();
    assert_flat_values(&preview.candidate, &scene, &basis);
    let complete = report.reference_work;
    assert!(complete.0 > initial.0 && complete.1 > initial.1);

    for (nodes, bytes, failure) in [
        (complete.0, complete.1, None),
        (complete.0 - 1, complete.1, Some("reference-work-nodes")),
        (complete.0, complete.1 - 1, Some("reference-work-bytes")),
    ] {
        let limits = SceneCompilationLimits::default()
            .with_reference_work_nodes(ItemLimit::new(nodes).unwrap())
            .with_reference_work_bytes(ByteLimit::new(bytes).unwrap());
        let report = compile_with_limits(&basis, &scene, limits);
        assert!(report.candidate().is_some(), "{:?}", report.diagnostics());
        assert_eq!(report.reference_work, initial);
        match (report.into_native_preview(), failure) {
            (Ok(preview), None) => assert_flat_values(&preview.candidate, &scene, &basis),
            (Err(diagnostics), Some(kind)) => {
                let diagnostic = diagnostics
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
                    })
                    .unwrap();
                assert_eq!(
                    field(diagnostic, "budget-kind"),
                    DiagnosticValueRef::Identifier(kind),
                );
                assert_eq!(
                    field(diagnostic, "observed"),
                    DiagnosticValueRef::Unsigned(if kind == "reference-work-nodes" {
                        complete.0 as u64
                    } else {
                        complete.1 as u64
                    }),
                );
                assert_eq!(
                    field(diagnostic, "maximum"),
                    DiagnosticValueRef::Unsigned(if kind == "reference-work-nodes" {
                        nodes as u64
                    } else {
                        bytes as u64
                    }),
                );
            }
            (result, failure) => panic!("unexpected preview admission: {result:?}, {failure:?}"),
        }
    }
}

fn projected_report(
    basis: &AuthoringSchemaBasis,
    working_limit: ByteLimit,
) -> AuthoringSceneCompilationReport {
    let source = AssetRef::path("prefabs/native-preview.ron").unwrap();
    let key = SourceDocumentKey::named(
        "dc807516-225d-4e25-9cbd-de8e54805c73".parse().unwrap(),
        "native-preview",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let prefab = Arc::new(PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("native-child")).with_component(native_id(), record()),
    ]));
    let mut anchor = SceneEntityRecord::new(id("prefab-instance"));
    anchor.prefab = Some(PrefabInstance {
        source: source.clone(),
        arguments: None,
        overrides: None,
    });
    try_compile_authoring_scene_with_loader(
        &SceneDocument::from_ordered_roots([anchor]),
        basis,
        SceneCompilationOptions::default(),
        None,
        |request, working| {
            assert_eq!(request, &source);
            working.restrict_working_bytes(working_limit);
            Ok::<_, Infallible>(Some(ResolvedPrefabSource::bound_authoring_source(
                Arc::clone(&prefab),
                basis.clone(),
                key.clone(),
            )))
        },
    )
    .unwrap()
}

fn field<'a>(diagnostic: &'a Diagnostic, key: &str) -> DiagnosticValueRef<'a> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .unwrap()
        .value()
}

#[test]
fn native_preview_admits_projected_omissions_and_sealing_under_the_original_allowance() {
    let basis = basis(provider(NATIVE_OWNER, 1, false), false);
    let installed = registry(None);
    let mut report = projected_report(&basis, SceneCompilationLimits::default().working_bytes());
    assert!(report.candidate().is_some(), "{:?}", report.diagnostics());
    assert!(report.omissions().is_empty());
    let initial = report.checked_work_bytes().unwrap();
    let restriction = report
        .checked_native_preview_work_bytes_for(&installed)
        .unwrap();
    assert!(restriction > initial);
    let AuthoringNativePreview {
        candidate: preview,
        omissions,
        ..
    } = report.finish_native_preview(Some(&installed)).unwrap();
    let complete = report.checked_work_bytes().unwrap();
    assert!(
        complete > restriction,
        "terminal sealing must retain its original allowance"
    );
    assert_eq!(omissions.len(), 1);
    assert!(matches!(
        omissions[0].location(),
        AuthoringPreviewOmissionLocation::ProjectedEntityComponent { provenance, .. }
            if provenance.projection_path().is_some()
    ));
    assert!(
        preview
            .entities()
            .iter()
            .all(|entity| entity.components().is_empty())
    );

    let exact = projected_report(&basis, ByteLimit::new(complete).unwrap());
    assert!(exact.candidate().is_some(), "{:?}", exact.diagnostics());
    assert_eq!(exact.checked_work_bytes(), Some(initial));
    assert!(exact.into_native_preview_for(&installed).is_ok());

    let short = projected_report(&basis, ByteLimit::new(complete - 1).unwrap());
    assert!(short.candidate().is_some(), "{:?}", short.diagnostics());
    assert_eq!(short.checked_work_bytes(), Some(initial));
    let diagnostics = short.into_native_preview_for(&installed).unwrap_err();
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .unwrap();
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("working-bytes")
    );
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(complete as u64)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned((complete - 1) as u64)
    );

    let short = projected_report(&basis, ByteLimit::new(restriction - 1).unwrap());
    assert!(short.candidate().is_some(), "{:?}", short.diagnostics());
    assert_eq!(short.checked_work_bytes(), Some(initial));
    let diagnostics = short.into_native_preview_for(&installed).unwrap_err();
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded")
        .unwrap();
    assert_eq!(
        field(diagnostic, "budget-kind"),
        DiagnosticValueRef::Identifier("working-bytes")
    );
    assert_eq!(
        field(diagnostic, "observed"),
        DiagnosticValueRef::Unsigned(restriction as u64)
    );
    assert_eq!(
        field(diagnostic, "maximum"),
        DiagnosticValueRef::Unsigned((restriction - 1) as u64)
    );
}
