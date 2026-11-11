use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::__private::{
    plan_declared_entity_references, validate_native_component_value_references_with_limits,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext,
    ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentMigrationError,
    ComponentRegistrySnapshot, ComponentSchema, ComponentTypeId, ComponentValue,
    ComponentValueCost, ComponentValueMaterializationLimits, DeclaredAssetReference,
    EntityReference, EntityReferenceTraversalLimits, PreparedComponent,
    collect_declared_asset_references_from_schema_with_limits,
    validate_component_value_with_limits,
};

use crate::{
    PrefabEntityOrigin, PrefabEntityTarget, SceneComponentBindings, SceneDocument, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, ScenePlacementParent, ScenePlacementSlot,
    diagnostics::{
        error as diagnostic_error, push_with_operation_index, with_codec_error,
        with_entity_reference_rewrite_error, with_migration_error, with_public_locator,
    },
    draft::SceneDraft,
    schema_authority::SchemaAuthority,
};

pub(crate) struct PreparedScene {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<PreparedSceneEntity>,
    pub(crate) diagnostics: DiagnosticReport,
}

/// Source acceptance carries diagnostics, never a prepared executable Scene.
pub(crate) struct SourceSceneValidation {
    pub(crate) diagnostics: DiagnosticReport,
}

pub(crate) struct PreparedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) components: Vec<PreparedSceneComponent>,
    pub(crate) data_components: Vec<PreparedSceneDataComponent>,
}

pub(crate) struct PreparedSceneComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) prepared: PreparedComponent,
}

pub(crate) struct PreparedSceneDataComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) owner: nara_reflect::ComponentSchemaOwnerSnapshot,
    pub(crate) record: crate::CompiledSceneComponent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedScenePlacement {
    parents: BTreeMap<SceneEntityId, Option<SceneEntityId>>,
}

impl ValidatedScenePlacement {
    pub(crate) fn parent_of(&self, entity: &SceneEntityId) -> Option<&SceneEntityId> {
        self.parents.get(entity).and_then(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlacementOwner {
    Root,
    Parent(SceneEntityId),
}

struct NormalizedSceneComponent<'a> {
    id: &'a ComponentTypeId,
    value: Cow<'a, ComponentValue>,
}

struct ScenePreflightOptions {
    operation_index: Option<usize>,
    validate_local_references: bool,
    validate_topology: bool,
    decode_native: bool,
}

pub(crate) struct EffectiveSceneValidation {
    pub(crate) diagnostics: DiagnosticReport,
    #[cfg(feature = "serde")]
    pub(crate) asset_references: Vec<DeclaredAssetReference>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectiveValueRole {
    Draft,
    Compiled,
    Runtime,
}

/// The concrete pure value batch shared by source compilation and closed flat admission.
/// These borrowed facts provide no source loading, mutation or activation authority.
struct FlatSceneComponentRef<'a> {
    entity: &'a SceneEntityId,
    component: &'a ComponentTypeId,
    schema_owner: Option<&'a nara_reflect::PersistentComponentSchemaOwnerId>,
    version: nara_reflect::ComponentSchemaVersion,
    value: &'a ComponentValue,
    authored_metadata: bool,
}

/// Admits a complete expanded value graph before asset realization or native decoding.
/// Runtime preparation still crosses the ordinary native path after the Host captures assets.
/// The callback admits Schema/value/reference validation scratch and retained asset inventory;
/// structural/default validation retains the caller's existing aggregate pre-admission.
pub(crate) fn validate_expanded_values<E>(
    document: &SceneDraft,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    admit_validation_work: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    validate_effective_values(document, authority, context, admit_validation_work)
}

fn validate_effective_values<E>(
    document: &SceneDraft,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    admit_validation_work: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    let mut diagnostics = DiagnosticReport::default();
    let placement = validate_scene_placement(
        &document.roots,
        document
            .entities
            .iter()
            .map(|entity| (&entity.id, entity.children.as_slice())),
        &mut diagnostics,
        None,
    );
    if let Some(registry) = authority.native_snapshot() {
        if let Some(entity) = placement.as_ref().and_then(|placement| {
            first_unavailable_hierarchy_projection_entity(document, placement, registry)
        }) {
            diagnostics.push(with_public_locator(
                diagnostic_error(
                    "scene.hierarchy-projection-unavailable",
                    "Scene hierarchy requires a runtime projection that is not implemented",
                ),
                "entity-id",
                entity.as_str(),
            ));
        }
    }
    let namespace = document
        .entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    let records = entities.into_iter().flat_map(|entity| {
        entity
            .components
            .iter()
            .map(move |(component, record)| FlatSceneComponentRef {
                entity: &entity.id,
                component,
                schema_owner: record.owner(),
                version: record.version,
                value: &record.value,
                authored_metadata: !record.contextual_defaults.is_empty(),
            })
    });
    validate_effective_component_values(
        records,
        &namespace,
        authority,
        context,
        EffectiveValueRole::Draft,
        diagnostics,
        admit_validation_work,
    )
}

fn validate_effective_component_values<'a, E>(
    records: impl IntoIterator<Item = FlatSceneComponentRef<'a>>,
    namespace: &BTreeSet<&SceneEntityId>,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    role: EffectiveValueRole,
    mut diagnostics: DiagnosticReport,
    mut admit_validation_work: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    let limits = context.materialization_limits();
    let traversal = reference_traversal_limits(limits);
    let mut value_budget = SceneValueBudget::new(limits);
    let mut asset_references = Vec::new();
    for record in records {
        let entity = record.entity;
        let component = record.component;
        let locate = |diagnostic| {
            with_public_locator(
                with_public_locator(diagnostic, "entity-id", entity.as_str()),
                "component-id",
                component.as_str(),
            )
        };
        let Some(schema) = authority.schema(component) else {
            diagnostics.push(locate(diagnostic_error(
                "scene.unknown-component",
                "Component type is not registered",
            )));
            continue;
        };
        if !authority.owner_claim_matches(
            component,
            record.schema_owner,
            role != EffectiveValueRole::Draft,
        ) {
            diagnostics.push(locate(diagnostic_error(
                "scene.component-owner-mismatch",
                "Persistent component owner does not match the admitted schema owner",
            )));
            continue;
        }
        if record.version != schema.version() {
            diagnostics.push(component_migration_diagnostic(
                entity.as_str(),
                component.as_str(),
                &ComponentMigrationError::UnsupportedVersion {
                    component_id: component.clone(),
                    from_version: record.version,
                    target_version: schema.version(),
                },
            ));
            continue;
        }
        if !schema.has_capability(ComponentCapability::Scene) {
            diagnostics.push(locate(diagnostic_error(
                "scene.component-not-scene-capable",
                "Component is not scene-capable",
            )));
            continue;
        }
        let invalid = |error: &ComponentCodecError| {
            locate(with_codec_error(
                diagnostic_error(
                    "scene.invalid-component-payload",
                    "Component payload is invalid",
                ),
                error,
                Some(schema),
            ))
        };
        if let Err(error) = nara_reflect::__private::validate_schema_capabilities(
            schema,
            [ComponentCapability::Scene],
        ) {
            diagnostics.push(invalid(&ComponentCodecError::Message(error.to_string())));
            continue;
        }
        if record.authored_metadata {
            diagnostics.push(invalid(&ComponentCodecError::UnresolvedAuthoredReference));
            continue;
        }
        if role != EffectiveValueRole::Draft {
            let explicit = nara_reflect::__private::component_defaults_are_explicit_with_admission(
                schema,
                record.value,
                &mut admit_validation_work,
            );
            match explicit {
                Ok(true) => {}
                Ok(false) => {
                    let (code, summary) = if role == EffectiveValueRole::Runtime {
                        (
                            "scene.runtime-incomplete-value",
                            "Runtime component defaults must be explicit in the processed value",
                        )
                    } else {
                        (
                            "scene.compiled-incomplete-value",
                            "Compiled Scene component defaults must already be explicit",
                        )
                    };
                    diagnostics.push(locate(diagnostic_error(code, summary)));
                    continue;
                }
                Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(error)) => {
                    return Err(error);
                }
                Err(nara_reflect::__private::EffectiveValueAdmissionError::Value(error)) => {
                    diagnostics.push(invalid(&error));
                    continue;
                }
            }
        }
        let effective = match if role == EffectiveValueRole::Draft {
            value_budget
                .materialize(schema, record.value, None)
                .map_err(nara_reflect::__private::EffectiveValueAdmissionError::Value)
        } else {
            value_budget
                .admit_explicit(record.value, &mut admit_validation_work)
                .map(|()| Cow::Borrowed(record.value))
        } {
            Ok(effective) => effective,
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(error)) => {
                return Err(error);
            }
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Value(error)) => {
                diagnostics.push(invalid(&error));
                continue;
            }
        };
        match nara_reflect::__private::validate_native_component_value_references_with_admission(
            &effective,
            limits,
            &mut admit_validation_work,
        ) {
            Ok(()) => {}
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(error)) => {
                return Err(error);
            }
            Err(nara_reflect::__private::EffectiveValueAdmissionError::Value(error)) => {
                diagnostics.push(invalid(&error));
                continue;
            }
        }
        // Every authority carries an admitted immutable Schema. Reuse Reflect's admitted
        // validator and account its Schema-sized path/index and shape scratch before it
        // allocates; the raw value size does not bound the number of optional fields.
        match nara_reflect::__private::validate_component_value_with_admission(
            schema,
            &effective,
            limits,
            &mut admit_validation_work,
        ) {
            Ok(()) => {}
            Err(nara_reflect::__private::ComponentValueAdmissionError::Admission(error)) => {
                return Err(error);
            }
            Err(nara_reflect::__private::ComponentValueAdmissionError::Value(error)) => {
                diagnostics.push(invalid(&error));
                continue;
            }
        }
        let references = match nara_reflect::__private::collect_declared_effective_asset_references_with_admission(
                schema, record.version, &effective, traversal, &mut admit_validation_work,
            ) {
                Ok(references) => references,
                Err(nara_reflect::__private::AssetReferenceCollectionError::Admission(error)) =>
                    return Err(error),
                Err(nara_reflect::__private::AssetReferenceCollectionError::Reference(_)) => {
                    diagnostics.push(invalid(&ComponentCodecError::Message("Schema-declared asset reference traversal failed".into())));
                    continue;
                }
            };
        if role == EffectiveValueRole::Runtime
            && let Some(reference) = references.iter().find(|reference| {
                !matches!(reference.asset_ref(), nara_asset::AssetRef::Product(_))
            })
        {
            diagnostics.push(with_public_locator(
                locate(diagnostic_error(
                    "scene.runtime-unprocessed-asset-reference",
                    "Runtime asset references require complete product identities",
                )),
                "field-id",
                reference.field_id().as_str(),
            ));
            continue;
        }
        // Scene checks the enclosing source record only. The selected product's actual
        // existence and typed domain admission belong to the root content owner.
        if let Some(asset_resolution) = context.asset_resolution() {
            let mut invalid_reference = None;
            for reference in &references {
                let bytes = asset_resolution.resolution_work_bytes(reference.asset_ref());
                if bytes.is_none_or(|bytes| bytes > limits.bytes().get()) {
                    invalid_reference = Some(reference);
                    break;
                }
                // A scoped Path lookup allocates its canonical spelling. Admit that actual
                // temporary before resolving it, independently of reference traversal work.
                admit_validation_work(0, 0, bytes.unwrap_or(0))?;
                if asset_resolution.resolve_ref(reference.asset_ref()).is_err() {
                    invalid_reference = Some(reference);
                    break;
                }
            }
            if let Some(reference) = invalid_reference {
                diagnostics.push(invalid(&ComponentCodecError::invalid_asset_ref(
                    reference.path().to_string(),
                    reference.asset_ref().to_string(),
                    "Asset reference is not declared in the project asset resolver",
                )));
                continue;
            }
        }
        let local_references =
            nara_reflect::__private::plan_declared_entity_references_with_admission::<
                SceneLocalTargetMissing,
                _,
            >(schema, &effective, traversal, &mut admit_validation_work);
        let local_references = match local_references {
            Ok(plan) => plan.visit(&effective, |_, reference| match reference {
                EntityReference::SceneLocal { entity } => namespace
                    .contains(entity)
                    .then_some(())
                    .ok_or(SceneLocalTargetMissing),
                EntityReference::Persistent { .. } | EntityReference::Authored { .. } => Ok(()),
            }),
            Err(nara_reflect::__private::EntityReferencePlanError::Admission(error)) => {
                return Err(error);
            }
            Err(nara_reflect::__private::EntityReferencePlanError::Reference(error)) => Err(error),
        };
        if let Err(error) = local_references {
            diagnostics.push(with_entity_reference_rewrite_error(
                locate(diagnostic_error(
                    "scene.local-reference-invalid",
                    "Scene component references do not close over the document namespace",
                )),
                &error,
                |_| "scene-local-target-missing",
            ));
            continue;
        }
        // Admit the complete new allocation when growing the retained inventory.
        // Doubling amortizes component-by-component collection without hidden reallocations.
        let required = asset_references.len().saturating_add(references.len());
        if required > asset_references.capacity() {
            let capacity = required.max(asset_references.capacity().saturating_mul(2));
            admit_validation_work(
                0,
                0,
                capacity.saturating_mul(size_of::<DeclaredAssetReference>()),
            )?;
            asset_references.reserve_exact(capacity - asset_references.len());
        }
        asset_references.extend(references);
    }
    Ok(EffectiveSceneValidation {
        diagnostics,
        #[cfg(feature = "serde")]
        asset_references,
    })
}

pub(crate) fn validate_flat_claims_with_admission<E>(
    claims: &crate::compiled::claims::FlatSceneClaims,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    require_runtime_products: bool,
    admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    let mut diagnostics = DiagnosticReport::default();
    let placement = validate_scene_placement(
        &claims.roots,
        claims
            .entities
            .iter()
            .map(|entity| (&entity.id, entity.children.as_slice())),
        &mut diagnostics,
        None,
    );
    if let Some(registry) = authority.native_snapshot()
        && let Some(entity) = placement.as_ref().and_then(|placement| {
            first_unavailable_flat_projection(
                placement,
                claims.entities.iter().map(|entity| &entity.id),
                claims.entities.iter().filter_map(|entity| {
                    entity
                        .components
                        .keys()
                        .any(|component| {
                            component.as_str() == "nara.scene.Visibility"
                                && registry.schema(component).is_some()
                        })
                        .then_some(&entity.id)
                }),
            )
        })
    {
        diagnostics.push(with_public_locator(
            diagnostic_error(
                "scene.hierarchy-projection-unavailable",
                "Scene hierarchy requires a runtime projection that is not implemented",
            ),
            "entity-id",
            entity.as_str(),
        ));
    }
    let namespace = claims
        .entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    let records = claims.entities.iter().flat_map(|entity| {
        entity
            .components
            .iter()
            .map(move |(component, record)| FlatSceneComponentRef {
                entity: &entity.id,
                component,
                schema_owner: record.schema_owner.as_ref(),
                version: record.version,
                value: &record.value,
                authored_metadata: false,
            })
    });
    validate_effective_component_values(
        records,
        &namespace,
        authority,
        context,
        if require_runtime_products {
            EffectiveValueRole::Runtime
        } else {
            EffectiveValueRole::Compiled
        },
        diagnostics,
        admit,
    )
}

fn validate_compiled_values_with_admission<E>(
    document: &crate::CompiledSceneDocument,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    let mut diagnostics = DiagnosticReport::default();
    let placement = validate_scene_placement(
        document.roots(),
        document
            .entities()
            .iter()
            .map(|entity| (entity.id(), entity.children())),
        &mut diagnostics,
        None,
    );
    if let Some(registry) = authority.native_snapshot()
        && let Some(entity) = placement.as_ref().and_then(|placement| {
            first_unavailable_flat_projection(
                placement,
                document.entities().iter().map(|entity| entity.id()),
                document.entities().iter().filter_map(|entity| {
                    entity
                        .components()
                        .keys()
                        .any(|component| {
                            component.as_str() == "nara.scene.Visibility"
                                && registry.schema(component).is_some()
                        })
                        .then_some(entity.id())
                }),
            )
        })
    {
        diagnostics.push(with_public_locator(
            diagnostic_error(
                "scene.hierarchy-projection-unavailable",
                "Scene hierarchy requires a runtime projection that is not implemented",
            ),
            "entity-id",
            entity.as_str(),
        ));
    }
    let namespace = document
        .entities()
        .iter()
        .map(|entity| entity.id())
        .collect::<BTreeSet<_>>();
    let records = document.entities().iter().flat_map(|entity| {
        entity
            .components()
            .iter()
            .map(move |(component, record)| FlatSceneComponentRef {
                entity: entity.id(),
                component,
                schema_owner: record.schema_owner(),
                version: record.version(),
                value: record.value(),
                authored_metadata: false,
            })
    });
    validate_effective_component_values(
        records,
        &namespace,
        authority,
        context,
        EffectiveValueRole::Compiled,
        diagnostics,
        admit,
    )
}

/// Prepares only closed values. A processed input may reuse pure admission solely after its
/// exact immutable definitions witness has been checked by the materialization owner.
pub(crate) fn preflight_compiled_scene_with_context(
    document: &crate::CompiledSceneDocument,
    registry: &ComponentRegistrySnapshot,
    bindings: Option<&SceneComponentBindings>,
    context: &mut ComponentDecodeContext<'_>,
    exact_definitions_admitted: bool,
) -> PreparedScene {
    #[cfg(feature = "work-observation")]
    let _work_span = tracing::info_span!(target: "nara::scene::work", "scene_decode").entered();
    let authority = bindings.map_or(SchemaAuthority::Native(registry), SchemaAuthority::Mixed);
    let mut diagnostics = DiagnosticReport::default();
    if bindings.is_some_and(|bindings| !registry.ptr_eq(bindings.native_snapshot())) {
        diagnostics.push(diagnostic_error(
            "scene.component-bindings-mismatch",
            "Scene bindings do not select the supplied native component registry",
        ));
    } else if !exact_definitions_admitted {
        let validation =
            validate_compiled_values_with_admission(document, authority, context, |_, _, _| {
                Ok::<_, std::convert::Infallible>(())
            });
        let validation = match validation {
            Ok(value) => value,
            Err(never) => match never {},
        };
        diagnostics.extend(validation.diagnostics);
        if !diagnostics.has_errors() {
            for entity in document.entities() {
                for (component, record) in entity.components() {
                    if authority
                        .current_flat_schema(component, record.schema_owner(), record.version())
                        .is_err()
                    {
                        diagnostics.push(with_public_locator(
                            with_public_locator(
                                diagnostic_error("scene.compiled-component-unavailable", "Compiled Scene requires an admitted current component definition"),
                                "entity-id", entity.id().as_str(),
                            ),
                            "component-id", component.as_str(),
                        ));
                    }
                }
            }
        }
    }
    if diagnostics.has_errors() {
        return PreparedScene {
            roots: Vec::new(),
            entities: Vec::new(),
            diagnostics,
        };
    }

    let mut entities = Vec::new();
    for entity in document.entities() {
        let mut components = Vec::new();
        let mut data_components = Vec::new();
        for (component, record) in entity.components() {
            if let Some(owner) = bindings.and_then(|bindings| bindings.data_owner(component)) {
                data_components.push(PreparedSceneDataComponent {
                    id: component.clone(),
                    owner: owner.clone(),
                    record: record.copy_for_native_preparation(),
                });
                continue;
            }
            match registry.preflight_component_with_context(component, record.value(), context) {
                Ok(prepared) => components.push(PreparedSceneComponent {
                    id: component.clone(),
                    prepared,
                }),
                Err(error) => {
                    diagnostics.push(component_payload_diagnostic(
                        entity.id().as_str(),
                        component,
                        &error,
                        authority,
                    ));
                }
            }
        }
        entities.push(PreparedSceneEntity {
            id: entity.id().clone(),
            children: entity.children().to_vec(),
            components,
            data_components,
        });
    }
    PreparedScene {
        roots: document.roots().to_vec(),
        entities,
        diagnostics,
    }
}

pub(crate) fn preflight_scene(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
) -> SourceSceneValidation {
    let mut context = ComponentDecodeContext::new();
    preflight_authoring_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    operation_index: usize,
) -> SourceSceneValidation {
    let mut context = ComponentDecodeContext::new();
    preflight_authoring_scene_with_context_for_patch(
        document,
        registry,
        &mut context,
        operation_index,
    )
}

pub(crate) fn preflight_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_source_native(document, registry, None, context)
}

/// Compiles source declarations before the complete Native/data admission path.
pub(crate) fn preflight_scene_with_context_and_bindings(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    bindings: &SceneComponentBindings,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_source_native(document, registry, Some(bindings), context)
}

/// Diagnostic validation has no source loader. The ordinary compiler alone interprets authored
/// placements, defaults and references; its closed result then crosses complete Native preflight.
fn preflight_source_native(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    bindings: Option<&SceneComponentBindings>,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    if bindings.is_some_and(|bindings| !registry.ptr_eq(bindings.native_snapshot())) {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic_error(
            "scene.component-bindings-mismatch",
            "Scene bindings do not select the supplied native component registry",
        ));
        return PreparedScene {
            roots: Vec::new(),
            entities: Vec::new(),
            diagnostics,
        };
    }
    let limits = crate::SceneCompilationLimits::default()
        .with_materialization_limits(context.materialization_limits());
    let options = crate::SceneCompilationOptions::default().with_limits(limits);
    let report = match bindings {
        Some(bindings) => bindings.try_compile_scene_with_loader(
            document,
            options,
            context.asset_resolution(),
            |_, _| Ok::<_, std::convert::Infallible>(None),
        ),
        None => document.try_compile_with_loader(
            registry,
            options,
            context.asset_resolution(),
            |_, _| Ok::<_, std::convert::Infallible>(None),
        ),
    };
    let mut report = match report {
        Ok(report) => report,
        Err(never) => match never {},
    };
    let Some(document) = report.candidate.take() else {
        return PreparedScene {
            roots: Vec::new(),
            entities: Vec::new(),
            diagnostics: report.diagnostics,
        };
    };
    let mut prepared =
        preflight_compiled_scene_with_context(&document, registry, bindings, context, false);
    report.diagnostics.extend(prepared.diagnostics);
    prepared.diagnostics = report.diagnostics;
    prepared
}

pub(crate) fn preflight_authoring_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    context: &mut ComponentDecodeContext<'_>,
) -> SourceSceneValidation {
    preflight_authoring_scene_with_context_at_operation(document, registry, context, None)
}

pub(crate) fn preflight_authoring_scene_with_context_at_operation(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    context: &mut ComponentDecodeContext<'_>,
    operation_index: Option<usize>,
) -> SourceSceneValidation {
    let diagnostics = preflight_source_values_with_context(
        &document.placements,
        &document.entities,
        registry,
        None,
        context,
        ScenePreflightOptions {
            operation_index,
            validate_local_references: true,
            validate_topology: true,
            decode_native: true,
        },
    );
    SourceSceneValidation { diagnostics }
}

/// Intermediate exact edits retain valid component shapes without requiring complete source
/// placement or executing a codec against a coupled field value before the batch is complete.
pub(crate) fn validate_patch_record_shapes(
    entities: &[SceneEntityRecord],
    registry: &ComponentRegistrySnapshot,
    operation_index: usize,
    limits: ComponentValueMaterializationLimits,
) -> DiagnosticReport {
    preflight_source_values_with_context(
        &[],
        entities,
        registry,
        None,
        &mut ComponentDecodeContext::new().with_materialization_limits(limits),
        ScenePreflightOptions {
            operation_index: Some(operation_index),
            validate_local_references: false,
            validate_topology: false,
            decode_native: false,
        },
    )
}

pub(crate) fn preflight_authoring_scene_with_context_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistrySnapshot,
    context: &mut ComponentDecodeContext<'_>,
    operation_index: usize,
) -> SourceSceneValidation {
    preflight_authoring_scene_with_context_at_operation(
        document,
        registry,
        context,
        Some(operation_index),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SceneLocalTargetMissing;

pub(crate) fn reference_traversal_limits(
    limits: ComponentValueMaterializationLimits,
) -> EntityReferenceTraversalLimits {
    let overhead = ComponentFieldId::MAX_BYTES
        .saturating_add(limits.depth().get().saturating_mul(size_of::<u32>()))
        .saturating_add(20);
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(limits.nodes().get().saturating_mul(2))
            .expect("non-zero value limits produce non-zero reference work limits"),
        ByteLimit::new(
            limits
                .bytes()
                .get()
                .saturating_mul(2)
                .saturating_add(limits.nodes().get().saturating_mul(overhead)),
        )
        .expect("non-zero value limits produce non-zero reference byte limits"),
        limits.depth(),
    )
}

pub(crate) fn validate_effective_local_references(
    schema: &ComponentSchema,
    effective: &ComponentValue,
    contains_entity: impl Fn(&SceneEntityId) -> bool,
    limits: EntityReferenceTraversalLimits,
) -> Result<(), ComponentEntityReferenceRewriteError<SceneLocalTargetMissing>> {
    let plan = plan_declared_entity_references(schema, effective, limits)?;
    plan.visit(effective, |_, reference| match reference {
        EntityReference::SceneLocal { entity } => contains_entity(entity)
            .then_some(())
            .ok_or(SceneLocalTargetMissing),
        // Authored targets have lexical Scene meaning; Native callers establish flatness
        // separately before using this shared Source declaration/closure check.
        EntityReference::Persistent { .. } | EntityReference::Authored { .. } => Ok(()),
    })
}

pub(crate) fn validate_scene_structure(document: &SceneDocument) -> DiagnosticReport {
    validate_source_structure(&document.placements, &document.entities)
}

pub(crate) fn validate_source_structure(
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
) -> DiagnosticReport {
    validate_source_structure_with_operation(placements, entities, None)
}

/// Checks declaration ownership without resolving routes or constructing an effective hierarchy.
/// Canonical base predicates are syntactically admitted here; the compiler checks their actual
/// selected sources, sibling membership, ordering and cycles when those occurrences are opened.
fn validate_source_structure_with_operation(
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    operation_index: Option<usize>,
) -> DiagnosticReport {
    let mut diagnostics = DiagnosticReport::default();
    let mut records = BTreeSet::new();
    for entity in entities {
        if !records.insert(&entity.id) {
            push_placement_diagnostic(
                &mut diagnostics,
                operation_index,
                "scene.duplicate-entity-id",
                "Scene entity ID is duplicated",
                &entity.id,
                None,
            );
        }
    }
    let mut targets = BTreeSet::new();
    let mut local_placements = BTreeSet::new();
    for declaration in placements {
        if !targets.insert(&declaration.target) {
            push_placement_diagnostic(
                &mut diagnostics,
                operation_index,
                "scene.duplicate-placement",
                "Scene target has more than one authored placement",
                declaration.target.entity.entity(),
                None,
            );
        }
        if declaration.target.through.is_empty()
            && let PrefabEntityOrigin::SourceEntity(entity) = &declaration.target.entity
        {
            local_placements.insert(entity);
        }
        let parent = match &declaration.placement.parent {
            ScenePlacementParent::Root => None,
            ScenePlacementParent::Entity(target) => Some(target),
        };
        let sibling = match &declaration.placement.slot {
            ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) => Some(target),
            ScenePlacementSlot::First | ScenePlacementSlot::Last => None,
        };
        for target in std::iter::once(&declaration.target)
            .chain(parent)
            .chain(sibling)
        {
            if !source_target_is_admissible(target, &records) {
                push_placement_diagnostic(
                    &mut diagnostics,
                    operation_index,
                    "scene.invalid-placement-target",
                    "Scene placement requires a local entity or a canonical-checked instance route",
                    target.entity.entity(),
                    None,
                );
            }
        }
    }
    for entity in records {
        if !local_placements.contains(entity) {
            push_placement_diagnostic(
                &mut diagnostics,
                operation_index,
                "scene.missing-placement",
                "Scene entity requires exactly one authored placement",
                entity,
                None,
            );
        }
    }
    diagnostics
}

fn source_target_is_admissible(
    target: &PrefabEntityTarget,
    records: &BTreeSet<&SceneEntityId>,
) -> bool {
    let Some(first) = target.through.first() else {
        return matches!(
            &target.entity,
            PrefabEntityOrigin::SourceEntity(entity) if records.contains(entity)
        );
    };
    let PrefabEntityOrigin::SourceEntity(anchor) = &first.anchor else {
        return false;
    };
    // An outer instance declaration can supply or replace the effective instance at this entry.
    // Existence of that selected instance is the compiler's responsibility.
    records.contains(anchor)
        && target.through.iter().all(|hop| {
            hop.expected_base.kind() == crate::SourceDocumentKind::Prefab
                && !matches!(
                    hop.expected_base.origin(),
                    crate::SourceDocumentOrigin::StableAsset { stable_id }
                        if stable_id.as_uuid().is_nil()
                )
        })
}

fn preflight_source_values_with_context(
    placements: &[SceneEntityPlacement],
    entities: &[SceneEntityRecord],
    registry: &ComponentRegistrySnapshot,
    bindings: Option<&SceneComponentBindings>,
    context: &mut ComponentDecodeContext<'_>,
    options: ScenePreflightOptions,
) -> DiagnosticReport {
    let ScenePreflightOptions {
        operation_index,
        validate_local_references,
        validate_topology,
        decode_native,
    } = options;
    let authority = bindings.map_or(SchemaAuthority::Native(registry), SchemaAuthority::Mixed);
    let mut diagnostics = if validate_topology {
        validate_source_structure_with_operation(placements, entities, operation_index)
    } else {
        DiagnosticReport::default()
    };
    if bindings.is_some_and(|bindings| !registry.ptr_eq(bindings.native_snapshot())) {
        diagnostics.push(diagnostic_error(
            "scene.component-bindings-mismatch",
            "Scene bindings do not select the supplied native component registry",
        ));
        return diagnostics;
    }

    let mut normalized_entities = Vec::new();
    let namespace = entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    let value_limits = context.materialization_limits();
    let mut value_budget = SceneValueBudget::new(value_limits);
    let traversal_limits = reference_traversal_limits(value_limits);
    for entity in sorted_entities(entities) {
        let mut normalized_components = Vec::new();
        for (component_id, component) in &entity.components {
            let data_owner = bindings.and_then(|bindings| bindings.data_owner(component_id));
            let Some(schema) = authority.schema(component_id) else {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.unknown-component",
                                "Component type is not registered",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            };
            if !authority.owner_matches(component_id, component) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-owner-mismatch",
                                "Persistent component owner does not match the admitted schema owner",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            }
            if !schema.has_capability(nara_reflect::ComponentCapability::Scene) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-not-scene-capable",
                                "Component is not scene-capable",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            }
            if component.version > schema.version()
                || (data_owner.is_some() && component.version != schema.version())
            {
                push_with_operation_index(
                    &mut diagnostics,
                    component_migration_diagnostic(
                        entity.id.as_str(),
                        component_id.as_str(),
                        &ComponentMigrationError::UnsupportedVersion {
                            component_id: component_id.clone(),
                            from_version: component.version,
                            target_version: schema.version(),
                        },
                    ),
                    operation_index,
                );
                continue;
            }

            let default_overrides = Some(&component.contextual_defaults);
            let defaults_validation = value_budget
                .admit_contextual_defaults(&component.contextual_defaults)
                .and_then(|()| {
                    crate::document::validate_contextual_defaults(schema, component, value_limits)
                });
            if let Err(error) = defaults_validation {
                push_with_operation_index(
                    &mut diagnostics,
                    component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &error,
                        authority,
                    ),
                    operation_index,
                );
                continue;
            }

            // Current-version values remain borrowed until complete effective admission. Only
            // native migration requires an owned input; admit that input before its clone.
            let migrated = if component.version == schema.version() {
                Cow::Borrowed(&component.value)
            } else {
                if let Err(error) = value_budget.admit_input(&component.value) {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
                match registry.migrate_component_value(
                    component_id,
                    component.version,
                    &component.value,
                ) {
                    Ok(migrated) if migrated.version == schema.version() => {
                        Cow::Owned(migrated.value)
                    }
                    result => {
                        let error = match result {
                            Ok(migrated) => ComponentMigrationError::UnsupportedVersion {
                                component_id: component_id.clone(),
                                from_version: migrated.version,
                                target_version: schema.version(),
                            },
                            Err(error) => error,
                        };
                        push_with_operation_index(
                            &mut diagnostics,
                            component_migration_diagnostic(
                                entity.id.as_str(),
                                component_id.as_str(),
                                &error,
                            ),
                            operation_index,
                        );
                        continue;
                    }
                }
            };
            // Keep the admitted candidate through codec preparation. A migrated value that
            // needs no defaults moves into the batch instead of borrowing a temporary or
            // repeating its migration in the second phase.
            let effective = match migrated {
                Cow::Borrowed(value) => value_budget.materialize(schema, value, default_overrides),
                Cow::Owned(value) => {
                    match value_budget.materialize(schema, &value, default_overrides) {
                        Ok(Cow::Borrowed(_)) => Ok(Cow::Owned(value)),
                        Ok(Cow::Owned(effective)) => Ok(Cow::Owned(effective)),
                        Err(error) => Err(error),
                    }
                }
            };
            let effective = match effective {
                Ok(effective) => effective,
                Err(error) => {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };

            let native_eligible = match validate_native_component_value_references_with_limits(
                &effective,
                value_limits,
            ) {
                Ok(()) => true,
                Err(ComponentCodecError::UnresolvedAuthoredReference) => false,
                Err(error) => {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };
            let validation = nara_reflect::__private::validate_schema_capabilities(
                schema,
                [ComponentCapability::Scene],
            )
            .map_err(|error| ComponentCodecError::Message(error.to_string()))
            .and_then(|()| validate_component_value_with_limits(schema, &effective, value_limits));
            if let Err(error) = validation {
                push_with_operation_index(
                    &mut diagnostics,
                    component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &error,
                        authority,
                    ),
                    operation_index,
                );
                continue;
            }

            let asset_references = match collect_declared_asset_references_from_schema_with_limits(
                schema,
                schema.version(),
                &effective,
                value_limits,
                traversal_limits,
            ) {
                Ok(references) => references,
                Err(_) => {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &ComponentCodecError::Message(
                                "Schema-declared asset reference traversal failed".to_owned(),
                            ),
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };
            // This strict source catalogue gate proves source identity, not domain product existence.
            if let Some(asset_resolution) = context.asset_resolution()
                && let Some(reference) = asset_references.iter().find(|reference| {
                    asset_resolution
                        .resolution_work_bytes(reference.asset_ref())
                        .is_none_or(|bytes| bytes > context.materialization_limits().bytes().get())
                        || asset_resolution.resolve_ref(reference.asset_ref()).is_err()
                })
            {
                push_with_operation_index(
                    &mut diagnostics,
                    component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &ComponentCodecError::invalid_asset_ref(
                            reference.path().to_string(),
                            reference.asset_ref().to_string(),
                            "Asset reference is not declared in the project asset resolver",
                        ),
                        authority,
                    ),
                    operation_index,
                );
                continue;
            }

            // Intermediate patch states may defer local target closure, but reference
            // declarations and capabilities must remain valid in every role.
            if let Err(error) = validate_effective_local_references(
                schema,
                &effective,
                |target| !validate_local_references || namespace.contains(target),
                traversal_limits,
            ) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_entity_reference_rewrite_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.local-reference-invalid",
                                    "Scene component references do not close over the document namespace",
                                ),
                                "entity-id",
                                entity.id.as_str(),
                            ),
                            "component-id",
                            component_id.as_str(),
                        ),
                        &error,
                        |_| "scene-local-target-missing",
                    ),
                    operation_index,
                );
                continue;
            }

            if decode_native && native_eligible && data_owner.is_none() {
                normalized_components.push(NormalizedSceneComponent {
                    id: component_id,
                    value: effective,
                });
            }
        }
        normalized_entities.push((entity, normalized_components));
    }

    // Source validation can consult a real codec only after the complete source value batch
    // passes. It never constructs Native entities or interprets authored placements.
    if diagnostics.has_errors() {
        return diagnostics;
    }
    for (entity, components) in normalized_entities {
        for component in components {
            let component_id = component.id;
            match registry.preflight_component_with_context(component_id, &component.value, context)
            {
                Ok(_) => {}
                Err(ComponentCodecError::MissingNativeBinding { .. }) => push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.missing-component-codec",
                                "Component has no scene codec",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                ),
                Err(error) => {
                    let diagnostic = component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &error,
                        authority,
                    );
                    push_with_operation_index(&mut diagnostics, diagnostic, operation_index);
                }
            }
        }
    }

    diagnostics
}

/// Logical effective-value admission is shared across entities and representations. Admission
/// remains charged after later validation fails, so a rejected record cannot recycle allowance.
struct SceneValueBudget {
    nodes: usize,
    bytes: usize,
    depth: DepthLimit,
}

impl SceneValueBudget {
    fn new(limits: ComponentValueMaterializationLimits) -> Self {
        Self {
            nodes: limits.nodes().get(),
            bytes: limits.bytes().get(),
            depth: limits.depth(),
        }
    }

    fn admit_explicit<E>(
        &mut self,
        value: &ComponentValue,
        admit: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
    ) -> Result<(), nara_reflect::__private::EffectiveValueAdmissionError<E>> {
        use nara_reflect::__private::EffectiveValueAdmissionError;
        let cost = value
            .cost_up_to_with_admission(self.nodes, self.bytes, |bytes| {
                admit(0, 0, bytes.unwrap_or(usize::MAX))
            })
            .map_err(EffectiveValueAdmissionError::Admission)?;
        let Some(cost) = cost else {
            let node_overflow = value
                .cost_up_to_with_admission(self.nodes, usize::MAX, |bytes| {
                    admit(0, 0, bytes.unwrap_or(usize::MAX))
                })
                .map_err(EffectiveValueAdmissionError::Admission)?
                .is_none();
            return Err(EffectiveValueAdmissionError::Value(if node_overflow {
                ComponentCodecError::MaterializationNodeLimit {
                    observed: self.nodes.saturating_add(1),
                    maximum: self.nodes,
                }
            } else {
                ComponentCodecError::MaterializationByteLimit {
                    observed: self.bytes.saturating_add(1),
                    maximum: self.bytes,
                }
            }));
        };
        // The shared admitted Native walker immediately checks depth and complete reference
        // flatness below. Do not allocate a second unadmitted depth stack for this cost check.
        self.nodes -= cost.nodes();
        self.bytes -= cost.logical_bytes();
        Ok(())
    }

    fn admit_input(
        &self,
        value: &ComponentValue,
    ) -> Result<ComponentValueCost, ComponentCodecError> {
        let Some(cost) = value.cost_up_to(self.nodes, self.bytes) else {
            return Err(if value.cost_up_to(self.nodes, usize::MAX).is_none() {
                ComponentCodecError::MaterializationNodeLimit {
                    observed: self.nodes.saturating_add(1),
                    maximum: self.nodes,
                }
            } else {
                ComponentCodecError::MaterializationByteLimit {
                    observed: self.bytes.saturating_add(1),
                    maximum: self.bytes,
                }
            });
        };
        validate_input_depth(value, self.depth)?;
        Ok(cost)
    }

    fn admit_contextual_defaults(
        &mut self,
        defaults: &BTreeMap<ComponentFieldId, ComponentValue>,
    ) -> Result<(), ComponentCodecError> {
        if defaults.is_empty() {
            return Ok(());
        }
        // The complete retained map survives Source admission even when every default is
        // hidden by an explicit value. Charge it separately from the selected effective tree.
        self.nodes =
            self.nodes
                .checked_sub(1)
                .ok_or(ComponentCodecError::MaterializationNodeLimit {
                    observed: 1,
                    maximum: 0,
                })?;
        for (field, value) in defaults {
            let bytes = field.as_str().len();
            self.bytes = self.bytes.checked_sub(bytes).ok_or(
                ComponentCodecError::MaterializationByteLimit {
                    observed: bytes,
                    maximum: self.bytes,
                },
            )?;
            let cost = self.admit_input(value)?;
            self.nodes -= cost.nodes();
            self.bytes -= cost.logical_bytes();
        }
        Ok(())
    }

    fn materialize<'value>(
        &mut self,
        schema: &ComponentSchema,
        value: &'value ComponentValue,
        default_overrides: Option<&BTreeMap<ComponentFieldId, ComponentValue>>,
    ) -> Result<Cow<'value, ComponentValue>, ComponentCodecError> {
        // Reflect owns the default plan and checks the complete resulting cost before cloning.
        // Raw usize ceilings intentionally preserve a zero residual byte allowance.
        let (effective, cost) = if let Some(defaults) = default_overrides {
            nara_reflect::__private::effective_component_value_with_default_overrides_with_budget(
                schema, value, defaults, self.nodes, self.bytes, self.depth,
            )?
        } else {
            nara_reflect::__private::effective_component_value_with_budget(
                schema, value, self.nodes, self.bytes, self.depth,
            )?
        };
        self.nodes -= cost.nodes();
        self.bytes -= cost.logical_bytes();
        Ok(effective)
    }
}

fn validate_input_depth(
    value: &ComponentValue,
    maximum: DepthLimit,
) -> Result<(), ComponentCodecError> {
    enum Frame<'value> {
        List(std::slice::Iter<'value, ComponentValue>, usize),
        Map(
            std::collections::btree_map::Iter<'value, String, ComponentValue>,
            usize,
        ),
    }

    let mut pending = Vec::new();
    let mut current = Some((value, 1_usize));
    while let Some((value, depth)) = current {
        if depth > maximum.get() {
            return Err(ComponentCodecError::MaterializationDepthLimit {
                observed: depth,
                maximum: maximum.get(),
            });
        }
        match value {
            ComponentValue::List(values) if !values.is_empty() => {
                pending.push(Frame::List(values.iter(), depth.saturating_add(1)));
            }
            ComponentValue::Map(values) if !values.is_empty() => {
                pending.push(Frame::Map(values.iter(), depth.saturating_add(1)));
            }
            _ => {}
        }
        current = loop {
            let next = match pending.last_mut() {
                Some(Frame::List(values, depth)) => values.next().map(|value| (value, *depth)),
                Some(Frame::Map(values, depth)) => values.next().map(|(_, value)| (value, *depth)),
                None => break None,
            };
            if next.is_some() {
                break next;
            }
            pending.pop();
        };
    }
    Ok(())
}

fn component_payload_diagnostic(
    entity_id: &str,
    component_id: &ComponentTypeId,
    error: &ComponentCodecError,
    authority: SchemaAuthority<'_>,
) -> Diagnostic {
    let diagnostic = with_codec_error(
        with_public_locator(
            with_public_locator(
                diagnostic_error(
                    "scene.invalid-component-payload",
                    "Component payload is invalid",
                ),
                "entity-id",
                entity_id,
            ),
            "component-id",
            component_id.as_str(),
        ),
        error,
        authority.schema(component_id),
    );
    let owner = match authority {
        SchemaAuthority::Mixed(bindings) if bindings.data_owner(component_id).is_some() => bindings
            .data_owner(component_id)
            .map(|owner| owner.owner_id().as_str()),
        _ => authority
            .native_snapshot()
            .and_then(|registry| registry.component_schema_owner(component_id))
            .map(|owner| owner.as_str()),
    };
    if let Some(owner) = owner {
        with_public_locator(diagnostic, "schema-owner", owner)
    } else {
        diagnostic
    }
}

pub(crate) fn first_unavailable_hierarchy_projection_entity<'document>(
    document: &'document SceneDraft,
    placement: &ValidatedScenePlacement,
    registry: &ComponentRegistrySnapshot,
) -> Option<&'document SceneEntityId> {
    let requires_unavailable_projection = |component_id: &ComponentTypeId| {
        component_id.as_str() == "nara.scene.Visibility" && registry.schema(component_id).is_some()
    };
    first_unavailable_flat_projection(
        placement,
        document.entities.iter().map(|entity| &entity.id),
        document.entities.iter().filter_map(|entity| {
            entity
                .components
                .keys()
                .any(requires_unavailable_projection)
                .then_some(&entity.id)
        }),
    )
}

fn first_unavailable_flat_projection<'a>(
    placement: &ValidatedScenePlacement,
    entities: impl IntoIterator<Item = &'a SceneEntityId>,
    visibility_entities: impl IntoIterator<Item = &'a SceneEntityId>,
) -> Option<&'a SceneEntityId> {
    let visibility = visibility_entities.into_iter().collect::<BTreeSet<_>>();
    entities.into_iter().find(|entity| {
        placement
            .parent_of(entity)
            .is_some_and(|parent| visibility.contains(entity) || visibility.contains(parent))
    })
}

pub(crate) fn component_migration_diagnostic(
    entity_id: &str,
    component_id: &str,
    error: &ComponentMigrationError,
) -> Diagnostic {
    match error {
        ComponentMigrationError::MigrationFailed { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.component-migration-failed",
                        "Component migration failed",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
        ComponentMigrationError::UnknownComponentId { .. }
        | ComponentMigrationError::UnsupportedVersion { .. }
        | ComponentMigrationError::MissingMigration { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.unsupported-component-version",
                        "Component version is unsupported",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
    }
}

fn sorted_entities(entities: &[SceneEntityRecord]) -> Vec<&SceneEntityRecord> {
    let mut entities = entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities
}

fn validate_scene_placement<'a>(
    roots: &[SceneEntityId],
    entities: impl Iterator<Item = (&'a SceneEntityId, &'a [SceneEntityId])> + Clone,
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
) -> Option<ValidatedScenePlacement> {
    let errors_before = diagnostics.stats().observed_errors();
    let mut ids = BTreeSet::<SceneEntityId>::new();
    for (entity, _) in entities.clone() {
        if !ids.insert(entity.clone()) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-entity-id",
                "Scene entity ID is duplicated",
                entity,
                None,
            );
        }
    }

    let mut owners = BTreeMap::<SceneEntityId, PlacementOwner>::new();
    for root in roots {
        if !ids.contains(root) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.dangling-root",
                "Scene root does not identify an entity record",
                root,
                None,
            );
            continue;
        }
        match owners.get(root) {
            None => {
                owners.insert(root.clone(), PlacementOwner::Root);
            }
            Some(PlacementOwner::Root) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-root",
                "Scene root is listed more than once",
                root,
                None,
            ),
            Some(PlacementOwner::Parent(parent)) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.root-child-overlap",
                "Scene entity cannot be both a root and a child",
                root,
                Some(parent),
            ),
        }
    }

    for (parent, children) in entities.clone() {
        let mut local_children = BTreeSet::<SceneEntityId>::new();
        for child in children {
            if !local_children.insert(child.clone()) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.duplicate-child",
                    "Scene parent lists the same child more than once",
                    child,
                    Some(parent),
                );
                continue;
            }
            if !ids.contains(child) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.dangling-child",
                    "Scene child does not identify an entity record",
                    child,
                    Some(parent),
                );
                continue;
            }
            match owners.get(child) {
                None => {
                    owners.insert(child.clone(), PlacementOwner::Parent(parent.clone()));
                }
                Some(PlacementOwner::Root) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.root-child-overlap",
                    "Scene entity cannot be both a root and a child",
                    child,
                    Some(parent),
                ),
                Some(PlacementOwner::Parent(existing)) if existing == parent => {
                    push_placement_diagnostic(
                        diagnostics,
                        operation_index,
                        "scene.duplicate-child",
                        "Scene parent lists the same child more than once",
                        child,
                        Some(parent),
                    );
                }
                Some(PlacementOwner::Parent(existing)) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.multiple-parents",
                    "Scene entity is owned by more than one parent",
                    child,
                    Some(existing),
                ),
            }
        }
    }

    for id in &ids {
        if !owners.contains_key(id) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.missing-placement",
                "Scene entity is absent from roots and every parent child list",
                id,
                None,
            );
        }
    }

    let parents = owners
        .iter()
        .map(|(entity, owner)| {
            let parent = match owner {
                PlacementOwner::Root => None,
                PlacementOwner::Parent(parent) => Some(parent.clone()),
            };
            (entity.clone(), parent)
        })
        .collect::<BTreeMap<_, _>>();

    let mut completed = BTreeSet::<SceneEntityId>::new();
    for (entity, _) in entities {
        if completed.contains(entity) {
            continue;
        }

        let mut path = Vec::<SceneEntityId>::new();
        let mut positions = BTreeMap::<SceneEntityId, usize>::new();
        let mut current = Some(entity.clone());
        while let Some(id) = current {
            if completed.contains(&id) {
                break;
            }
            if let Some(cycle_start) = positions.get(&id).copied() {
                let witness = path[cycle_start..]
                    .iter()
                    .min()
                    .expect("cycle path contains the repeated entity")
                    .clone();
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.placement-cycle",
                    "Scene placement contains a cycle",
                    &witness,
                    parents.get(&witness).and_then(Option::as_ref),
                );
                break;
            }
            positions.insert(id.clone(), path.len());
            path.push(id.clone());
            current = parents.get(&id).and_then(Clone::clone);
        }
        completed.extend(path);
    }

    (diagnostics.stats().observed_errors() == errors_before)
        .then_some(ValidatedScenePlacement { parents })
}

fn push_placement_diagnostic(
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
    parent: Option<&SceneEntityId>,
) {
    let diagnostic = with_public_locator(
        diagnostic_error(code, summary),
        "entity-id",
        entity.as_str(),
    );
    let diagnostic = match parent {
        Some(parent) => with_public_locator(diagnostic, "parent-id", parent.as_str()),
        None => diagnostic,
    };
    push_with_operation_index(diagnostics, diagnostic, operation_index);
}

#[cfg(test)]
mod tests {
    use crate::{SceneComponentRecord, ScenePlacement};
    use nara_ecs::Component;
    use nara_reflect::{
        ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;

    #[derive(Component)]
    struct NativeFlag(bool);

    fn native_id() -> ComponentTypeId {
        ComponentTypeId::new("a.NativeFlag")
    }

    fn data_id() -> ComponentTypeId {
        ComponentTypeId::new("z.Data")
    }

    fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        )
    }

    fn data_schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
        ComponentSchema::new(data_id(), "Data", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields(fields)
    }

    fn definitions(schema: ComponentSchema) -> (ComponentRegistrySnapshot, SceneComponentBindings) {
        let mut registry = nara_reflect::ComponentRegistryBuilder::new();
        registry
            .register_persistent_component_with_codec::<NativeFlag, _, _>(
                ComponentSchema::new(native_id(), "Flag", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("x"),
                        "Flag",
                        ComponentFieldPath::from_fields(["x"]),
                        ComponentValueKind::Bool,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                |value| Ok(NativeFlag(value.field_bool("x")?)),
                |flag| Ok(ComponentValue::map([("x", ComponentValue::Bool(flag.0))])),
            )
            .unwrap();
        let registry = registry.freeze().unwrap();
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("game.data").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let bindings = SceneComponentBindings::new(registry.clone(), [owner]).unwrap();
        (registry, bindings)
    }

    fn data_record(value: ComponentValue) -> SceneComponentRecord {
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
            .with_owner(PersistentComponentSchemaOwnerId::try_new("game.data").unwrap())
    }

    fn mixed_document(value: ComponentValue, separate_entities: bool) -> SceneDocument {
        let mut native = SceneEntityRecord::new(SceneEntityId::new("a").unwrap()).with_component(
            native_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("x", ComponentValue::Bool(true))]),
            ),
        );
        if separate_entities {
            SceneDocument::from_ordered_roots([
                native,
                SceneEntityRecord::new(SceneEntityId::new("z").unwrap())
                    .with_component(data_id(), data_record(value)),
            ])
        } else {
            native.components.insert(data_id(), data_record(value));
            SceneDocument::from_ordered_roots([native])
        }
    }

    fn prepare(
        document: &SceneDocument,
        registry: &ComponentRegistrySnapshot,
        bindings: &SceneComponentBindings,
        limits: ComponentValueMaterializationLimits,
    ) -> PreparedScene {
        preflight_scene_with_context_and_bindings(
            document,
            registry,
            bindings,
            &mut ComponentDecodeContext::new().with_materialization_limits(limits),
        )
    }

    #[test]
    fn source_structure_requires_one_placement_for_each_local_record() {
        let entity = SceneEntityId::new("local").unwrap();
        let record = SceneEntityRecord::new(entity.clone());
        let placement = SceneEntityPlacement::root(entity.clone());
        let has_code = |report: &DiagnosticReport, code: &str| {
            report
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == code)
        };
        assert!(has_code(
            &validate_source_structure(&[], std::slice::from_ref(&record)),
            "scene.missing-placement",
        ));
        assert!(has_code(
            &validate_source_structure(
                &[placement.clone(), placement.clone()],
                std::slice::from_ref(&record),
            ),
            "scene.duplicate-placement",
        ));
        assert!(has_code(
            &validate_source_structure(&[placement.clone()], &[record.clone(), record.clone()]),
            "scene.duplicate-entity-id",
        ));
        let mut placements = vec![placement];
        for target in [
            PrefabEntityTarget::source(SceneEntityId::new("absent").unwrap()),
            PrefabEntityTarget::addition(entity),
        ] {
            placements.push(SceneEntityPlacement::new(
                target,
                ScenePlacement::last(ScenePlacementParent::Root),
            ));
            assert!(has_code(
                &validate_source_structure(&placements, std::slice::from_ref(&record)),
                "scene.invalid-placement-target",
            ));
            placements.pop();
        }
        assert!(!validate_source_structure(&placements, &[record]).has_errors());
    }

    #[test]
    fn source_routes_require_canonical_predicates_without_effective_instance_evaluation() {
        let anchor = SceneEntityId::new("anchor").unwrap();
        let projected = SceneEntityId::new("projected").unwrap();
        let record = SceneEntityRecord::new(anchor.clone());
        let key = crate::SourceDocumentKey::named(
            "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
            "prefab.source",
            crate::SourceDocumentKind::Prefab,
        )
        .unwrap();
        let route = PrefabEntityTarget {
            through: vec![crate::PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(anchor.clone()),
                expected_base: key,
            }],
            entity: PrefabEntityOrigin::SourceEntity(projected),
        };
        // The actual instance can be supplied or replaced by effective outer intent. Admission
        // checks the local route entry and canonical predicates without opening that occurrence.
        assert!(record.prefab.is_none());
        let placements = [
            SceneEntityPlacement::root(anchor),
            SceneEntityPlacement::new(
                route.clone(),
                ScenePlacement::before(ScenePlacementParent::Entity(route.clone()), route.clone()),
            ),
        ];
        assert!(
            !validate_source_structure(&placements, std::slice::from_ref(&record)).has_errors()
        );

        let mut invalid = route;
        invalid.through[0].expected_base = crate::SourceDocumentKey::stable_asset(
            "00000000-0000-0000-0000-000000000000".parse().unwrap(),
            crate::SourceDocumentKind::Prefab,
        );
        for position in 0..3 {
            let mut declarations = placements.clone();
            match position {
                0 => declarations[1].target = invalid.clone(),
                1 => {
                    declarations[1].placement.parent =
                        ScenePlacementParent::Entity(invalid.clone());
                }
                _ => declarations[1].placement.slot = ScenePlacementSlot::After(invalid.clone()),
            }
            assert!(
                validate_source_structure(&declarations, std::slice::from_ref(&record))
                    .has_errors()
            );
        }
    }

    #[test]
    fn source_validation_does_not_construct_a_hierarchy_for_native_admission() {
        let left = SceneEntityId::new("left").unwrap();
        let right = SceneEntityId::new("right").unwrap();
        let document = SceneDocument::new(
            [
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(left.clone()),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        right.clone(),
                    ))),
                ),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(right.clone()),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        left.clone(),
                    ))),
                ),
            ],
            [SceneEntityRecord::new(left), SceneEntityRecord::new(right)],
        );
        let registry = nara_reflect::ComponentRegistryBuilder::new()
            .freeze()
            .unwrap();
        let before = document.clone();
        assert!(
            !preflight_authoring_scene(&document, &registry)
                .diagnostics
                .has_errors()
        );
        let native = preflight_scene(&document, &registry);
        assert!(native.diagnostics.has_errors());
        assert!(native.entities.is_empty());
        assert_eq!(document, before);
    }

    #[test]
    fn native_and_data_share_aggregate_allowance_including_zero_byte_residual() {
        let (registry, bindings) = definitions(data_schema([]));
        for separate_entities in [false, true] {
            let document = mixed_document(ComponentValue::Map(BTreeMap::new()), separate_entities);
            for (nodes, bytes, accepted) in [(3, 2, true), (2, 2, false), (3, 1, false)] {
                let prepared = prepare(&document, &registry, &bindings, limits(nodes, bytes, 2));
                assert_eq!(
                    !prepared.diagnostics.has_errors(),
                    accepted,
                    "{:?}",
                    prepared.diagnostics,
                );
                if accepted {
                    assert_eq!(prepared.entities[0].components.len(), 1);
                    assert_eq!(prepared.entities.last().unwrap().data_components.len(), 1);
                }
            }
        }
    }

    #[test]
    fn defaults_consume_complete_effective_cost_without_changing_source() {
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        let (registry, bindings) = definitions(schema);
        let document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
        let before = document.clone();
        for (nodes, bytes, accepted) in [(4, 16, true), (3, 16, false), (4, 15, false)] {
            let prepared = prepare(&document, &registry, &bindings, limits(nodes, bytes, 2));
            assert_eq!(
                !prepared.diagnostics.has_errors(),
                accepted,
                "{:?}",
                prepared.diagnostics
            );
            if accepted {
                let data = &prepared.entities[0].data_components[0];
                assert_eq!(data.record.value().field_u64("amount").unwrap(), 7);
                assert!(data.owner.ptr_eq(bindings.data_owner(&data_id()).unwrap()));
            }
        }
        assert_eq!(document, before);
    }

    #[test]
    fn data_requires_explicit_matching_owner_and_current_version() {
        let (registry, bindings) = definitions(data_schema([]));
        for (owner, version, code) in [
            (
                None,
                ComponentSchemaVersion::ONE,
                "scene.component-owner-mismatch",
            ),
            (
                Some(PersistentComponentSchemaOwnerId::try_new("game.other").unwrap()),
                ComponentSchemaVersion::ONE,
                "scene.component-owner-mismatch",
            ),
            (
                Some(PersistentComponentSchemaOwnerId::try_new("game.data").unwrap()),
                ComponentSchemaVersion(2),
                "scene.unsupported-component-version",
            ),
        ] {
            let mut document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
            let record = document.entities[0].components.get_mut(&data_id()).unwrap();
            record.owner = owner;
            record.version = version;
            let prepared = prepare(&document, &registry, &bindings, limits(8, 32, 3));
            assert!(prepared.entities.is_empty());
            assert!(
                prepared
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code)
            );
        }
    }

    #[test]
    fn data_default_local_references_close_over_the_shared_namespace() {
        let target = SceneEntityId::new("target").unwrap();
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: target.clone(),
            }),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
        let (registry, bindings) = definitions(schema);
        for include_target in [false, true] {
            let mut document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
            document.entities[0]
                .components
                .get_mut(&data_id())
                .unwrap()
                .contextual_defaults
                .insert(
                    ComponentFieldId::new("target"),
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: target.clone(),
                    }),
                );
            if include_target {
                document
                    .placements
                    .push(SceneEntityPlacement::root(target.clone()));
                document
                    .entities
                    .push(SceneEntityRecord::new(target.clone()));
            }
            let prepared = prepare(&document, &registry, &bindings, limits(8, 64, 3));
            assert_eq!(
                !prepared.diagnostics.has_errors(),
                include_target,
                "{:?}",
                prepared.diagnostics
            );
            if !include_target {
                assert!(prepared.entities.is_empty());
                assert!(
                    prepared.diagnostics.iter().any(|diagnostic| {
                        diagnostic.code().as_str() == "scene.prefab-entity-reference-rewrite-failed"
                    }),
                    "{:?}",
                    prepared.diagnostics
                );
            }
        }
    }

    #[test]
    fn data_default_assets_require_selected_project_metadata() {
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueKind::AssetRef,
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                (
                    "value",
                    ComponentValue::String("textures/missing.png".into()),
                ),
            ]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]);
        let (registry, bindings) = definitions(schema);
        let document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
        let asset_resolution = nara_asset::ProjectAssetDatabase::default();
        let mut context = ComponentDecodeContext::new()
            .with_asset_resolution(asset_resolution.resolver(&nara_asset::AssetPathScope::ROOT));
        let prepared = preflight_scene_with_context_and_bindings(
            &document,
            &registry,
            &bindings,
            &mut context,
        );
        assert!(prepared.diagnostics.has_errors());
        assert!(prepared.entities.is_empty());
    }

    #[test]
    fn bounded_input_and_no_default_effective_values_enforce_depth() {
        let value = ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]);
        let mut budget = SceneValueBudget::new(limits(3, 1, 2));
        assert!(matches!(
            budget.admit_input(&value),
            Err(ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2
            })
        ));
        assert!(matches!(
            budget.materialize(&data_schema([]), &value, None),
            Err(ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2
            })
        ));
    }

    #[test]
    fn zero_byte_residual_accepts_zero_cost_values_and_rejects_positive_defaults() {
        let mut budget = SceneValueBudget {
            nodes: 4,
            bytes: 0,
            depth: DepthLimit::new(2).unwrap(),
        };
        let empty_schema = data_schema([]);
        assert!(
            budget
                .materialize(&empty_schema, &ComponentValue::Null, None)
                .is_ok()
        );
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("x"),
            "Flag",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::Bool,
            ComponentValue::Null,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        assert!(matches!(
            budget.materialize(&schema, &ComponentValue::Map(BTreeMap::new()), None),
            Err(ComponentCodecError::MaterializationByteLimit { maximum: 0, .. })
        ));
        assert_eq!(budget.nodes, 3);
        assert_eq!(budget.bytes, 0);
    }

    #[test]
    fn native_migration_input_is_admitted_before_invoking_the_hook() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = nara_reflect::ComponentRegistryBuilder::new();
        registry
            .register_persistent_component_with_codec::<NativeFlag, _, _>(
                ComponentSchema::new(native_id(), "Flag", ComponentSchemaVersion(2))
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("x"),
                        "Flag",
                        ComponentFieldPath::from_fields(["x"]),
                        ComponentValueKind::Bool,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                |value| Ok(NativeFlag(value.field_bool("x")?)),
                |flag| Ok(ComponentValue::map([("x", ComponentValue::Bool(flag.0))])),
            )
            .unwrap();
        let hook_calls = Arc::clone(&calls);
        registry
            .register_component_migration(
                &native_id(),
                ComponentSchemaVersion::ONE,
                ComponentSchemaVersion(2),
                move |_| {
                    hook_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(ComponentValue::map([("x", ComponentValue::Bool(true))]))
                },
            )
            .unwrap();
        let registry = registry.freeze().unwrap();
        for (value, limits) in [
            (
                ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]),
                limits(8, 8, 2),
            ),
            (ComponentValue::String("too long".into()), limits(8, 1, 8)),
            (
                ComponentValue::List(vec![ComponentValue::Null; 2]),
                limits(2, 8, 8),
            ),
        ] {
            let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("source").unwrap(),
            )
            .with_component(
                native_id(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            )]);
            let prepared = preflight_scene_with_context(
                &document,
                &registry,
                &mut ComponentDecodeContext::new().with_materialization_limits(limits),
            );
            assert!(prepared.diagnostics.has_errors());
            assert!(prepared.entities.is_empty());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("source").unwrap(),
        )
        .with_component(
            native_id(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
        let prepared = preflight_scene_with_context(
            &document,
            &registry,
            &mut ComponentDecodeContext::new().with_materialization_limits(limits(2, 2, 2)),
        );
        assert!(
            !prepared.diagnostics.has_errors(),
            "{:?}",
            prepared.diagnostics
        );
        assert_eq!(prepared.entities[0].components.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn source_patch_validation_retains_operation_diagnostics_before_native_decode() {
        let (registry, _) = definitions(data_schema([]));
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("source").unwrap(),
        )
        .with_component(
            native_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("x", ComponentValue::U64(1))]),
            ),
        )]);
        let diagnostics = document.validate_authoring_for_patch(&registry, 7);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.invalid-component-payload"
                && diagnostic
                    .fields()
                    .iter()
                    .any(|field| field.key().as_str() == "operation-index")
        }));
    }

    #[test]
    fn source_admits_complete_authored_default_cost_and_expanded_admission_requires_flat_values() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let reference = nara_identity::AuthoredEntityReference::input(
            nara_identity::ReferenceInputId::new("required-target").unwrap(),
        );
        let default = ComponentValue::EntityReference(EntityReference::Authored { reference });
        let effective = ComponentValue::map([("target", default.clone())]);
        let retained_default_cost = effective.cost();
        let cost = retained_default_cost.saturating_add(effective.cost());
        let schema = ComponentSchema::new(native_id(), "Flag", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
                default.clone(),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::EntityRef)]);
        let calls = Arc::new(AtomicUsize::new(0));
        let codec_calls = Arc::clone(&calls);
        let mut registry = nara_reflect::ComponentRegistryBuilder::new();
        registry
            .register_persistent_component_codec::<NativeFlag, _, _>(
                schema,
                move |_| {
                    codec_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(nara_reflect::PreparedComponentCandidate::insert(
                        NativeFlag(true),
                    ))
                },
                |_| Ok(ComponentValue::map([("target", ComponentValue::Null)])),
            )
            .unwrap();
        let registry = registry.freeze().unwrap();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("source").unwrap(),
        )
        .with_component(
            native_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::Map(BTreeMap::new()),
            )
            .with_contextual_default(ComponentFieldId::new("target"), default),
        )]);
        let before = document.clone();
        for (nodes, bytes, accepted) in [
            (cost.nodes(), cost.logical_bytes(), true),
            (cost.nodes() - 1, cost.logical_bytes(), false),
            (cost.nodes(), cost.logical_bytes() - 1, false),
        ] {
            let result = preflight_authoring_scene_with_context_at_operation(
                &document,
                &registry,
                &mut ComponentDecodeContext::new()
                    .with_materialization_limits(limits(nodes, bytes, 4)),
                Some(3),
            );
            assert_eq!(
                !result.diagnostics.has_errors(),
                accepted,
                "{:?}",
                result.diagnostics,
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        let draft = SceneDraft::new(
            [SceneEntityId::new("source").unwrap()],
            [crate::draft::ExpandedSceneEntity {
                id: SceneEntityId::new("source").unwrap(),
                children: Vec::new(),
                components: [(
                    native_id(),
                    SceneComponentRecord::new(ComponentSchemaVersion::ONE, effective.clone()),
                )]
                .into_iter()
                .collect(),
            }],
        );
        let expanded = validate_expanded_values(
            &draft,
            SchemaAuthority::Native(&registry),
            &mut ComponentDecodeContext::new(),
            |_, _, _| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap();
        assert!(expanded.diagnostics.has_errors());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(document, before);

        // Source-dependent Schema defaults require their real declaration even when an
        // explicit value hides them. Admission must not manufacture that source context.
        for value in [
            ComponentValue::Map(BTreeMap::new()),
            ComponentValue::map([("target", ComponentValue::Null)]),
        ] {
            let mut missing_context = document.clone();
            let record = missing_context.entities[0]
                .components
                .get_mut(&native_id())
                .unwrap();
            record.value = value;
            record.contextual_defaults.clear();
            let source = preflight_authoring_scene(&missing_context, &registry);
            assert!(source.diagnostics.has_errors());
            assert!(
                missing_context.entities[0].components[&native_id()]
                    .contextual_defaults
                    .is_empty()
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }

        let mut contextual = document.clone();
        let record = contextual.entities[0]
            .components
            .get_mut(&native_id())
            .unwrap();
        record.contextual_defaults.insert(
            ComponentFieldId::new("target"),
            effective.get("target").unwrap().clone(),
        );
        record.value = ComponentValue::map([("target", ComponentValue::Null)]);
        // Retained metadata is one map keyed by stable field ID, independent of the
        // effective value. A hidden authored default still consumes that complete cost.
        let contextual_cost = retained_default_cost.saturating_add(record.value.cost());
        for (nodes, bytes, accepted) in [
            (
                contextual_cost.nodes(),
                contextual_cost.logical_bytes(),
                true,
            ),
            (
                contextual_cost.nodes() - 1,
                contextual_cost.logical_bytes(),
                false,
            ),
            (
                contextual_cost.nodes(),
                contextual_cost.logical_bytes() - 1,
                false,
            ),
        ] {
            calls.store(0, Ordering::SeqCst);
            let source = preflight_authoring_scene_with_context(
                &contextual,
                &registry,
                &mut ComponentDecodeContext::new()
                    .with_materialization_limits(limits(nodes, bytes, 4)),
            );
            assert_eq!(
                !source.diagnostics.has_errors(),
                accepted,
                "{:?}",
                source.diagnostics,
            );
            assert_eq!(calls.load(Ordering::SeqCst), if accepted { 1 } else { 0 });
        }
        calls.store(0, Ordering::SeqCst);
        let mut unknown_default = contextual.clone();
        unknown_default.entities[0]
            .components
            .get_mut(&native_id())
            .unwrap()
            .contextual_defaults
            .insert(
                ComponentFieldId::new("unknown"),
                effective.get("target").unwrap().clone(),
            );
        assert!(
            preflight_authoring_scene(&unknown_default, &registry)
                .diagnostics
                .has_errors()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        for value in [
            ComponentValue::map([("target", ComponentValue::String("not a reference".into()))]),
            ComponentValue::map([("target", ComponentValue::Null), ("retained", effective)]),
        ] {
            let mut invalid = document.clone();
            invalid.entities[0]
                .components
                .get_mut(&native_id())
                .unwrap()
                .value = value;
            let source = preflight_authoring_scene(&invalid, &registry);
            assert!(source.diagnostics.has_errors());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}
