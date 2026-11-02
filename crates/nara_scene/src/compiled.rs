//! Closed flat Scene storage, admitted before any native codec participates.
//!
//! Canonical entity provenance identifies materialized entities. Source declaration contributors,
//! instance slots and editing authority remain with the separate source compilation owner.

pub(crate) mod claims;

use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
};

use nara_asset::AssetReferenceResolver;
use nara_diagnostic::DiagnosticReport;
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueMaterializationLimits,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    PrefabProjectionStep, SceneEntityId, SceneEntityProvenanceMap, SourceDocumentKey,
    SourceDocumentKind, SourceDocumentOrigin,
    candidate::validate_flat_scene_provenance,
    diagnostics::{error, with_codec_error, with_public_locator},
    draft::SceneDraft,
    provenance::CanonicalSceneEntityAddress,
    schema_authority::SchemaAuthority,
};

/// One current, explicit effective component value with its actual selected Schema owner.
///
/// A missing owner means that compilation selected an unowned native definition. It is not a
/// source omission or permission to reinterpret this value under an arbitrary receiving owner.
/// Receiving definitions and native codecs still require their ordinary complete admission.
#[derive(Debug, PartialEq)]
pub struct CompiledSceneComponent {
    schema_owner: Option<PersistentComponentSchemaOwnerId>,
    version: ComponentSchemaVersion,
    value: ComponentValue,
}

impl CompiledSceneComponent {
    /// The direct data owner calls this only after complete current-value and World admission.
    pub(crate) fn from_admitted_data(
        owner: PersistentComponentSchemaOwnerId,
        version: ComponentSchemaVersion,
        value: ComponentValue,
    ) -> Self {
        Self {
            schema_owner: Some(owner),
            version,
            value,
        }
    }

    /// Native preparation has admitted its independent retained values before making this copy.
    pub(crate) fn copy_for_native_preparation(&self) -> Self {
        Self {
            schema_owner: self.schema_owner.clone(),
            version: self.version,
            value: self.value.clone(),
        }
    }

    #[must_use]
    pub const fn schema_owner(&self) -> Option<&PersistentComponentSchemaOwnerId> {
        self.schema_owner.as_ref()
    }

    #[must_use]
    pub const fn version(&self) -> ComponentSchemaVersion {
        self.version
    }

    #[must_use]
    pub const fn value(&self) -> &ComponentValue {
        &self.value
    }
}

/// One materialized entity, with ordered children and no executable source declarations.
#[derive(Debug, PartialEq)]
pub struct CompiledSceneEntity {
    id: SceneEntityId,
    children: Box<[SceneEntityId]>,
    components: BTreeMap<ComponentTypeId, CompiledSceneComponent>,
}

impl CompiledSceneEntity {
    #[must_use]
    pub const fn id(&self) -> &SceneEntityId {
        &self.id
    }

    #[must_use]
    pub fn children(&self) -> &[SceneEntityId] {
        &self.children
    }

    #[must_use]
    pub const fn components(&self) -> &BTreeMap<ComponentTypeId, CompiledSceneComponent> {
        &self.components
    }
}

/// A complete flat closure with explicit values and one canonical origin per final entity.
///
/// Only the concrete Scene sealer creates this move-only storage. It carries neither source
/// declaration correspondence nor an exact receiving-definition or World admission witness.
///
/// Callers cannot turn source records into compiled data by wrapping them:
///
/// ```compile_fail
/// use nara_scene::{CompiledSceneDocument, SceneDocument};
/// fn unchecked(source: SceneDocument) -> CompiledSceneDocument {
///     CompiledSceneDocument::from(source)
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct CompiledSceneDocument {
    roots: Box<[SceneEntityId]>,
    entities: Box<[CompiledSceneEntity]>,
    canonical_provenance: SceneEntityProvenanceMap,
}

impl CompiledSceneDocument {
    #[must_use]
    pub fn roots(&self) -> &[SceneEntityId] {
        &self.roots
    }

    /// Borrows records in entity-ID order. Root and child order remain independently authored.
    #[must_use]
    pub fn entities(&self) -> &[CompiledSceneEntity] {
        &self.entities
    }

    #[must_use]
    pub const fn canonical_provenance(&self) -> &SceneEntityProvenanceMap {
        &self.canonical_provenance
    }

    /// Binds the containing source supplied by its owner, without inferring a runtime occurrence.
    #[must_use]
    pub fn bind_source(self, source_key: SourceDocumentKey) -> CompiledSceneCandidate {
        CompiledSceneCandidate {
            source_key,
            document: self,
        }
    }
}

/// A move-only flat materialization input bound to its actual containing source.
///
/// A retained borrow cannot extend the allocation independently of its owner:
///
/// ```compile_fail
/// use nara_scene::CompiledSceneCandidate;
/// fn escape(candidate: &CompiledSceneCandidate) -> CompiledSceneCandidate {
///     candidate.clone()
/// }
/// ```
///
/// Source identity cannot be replaced independently of the candidate:
///
/// ```compile_fail
/// use nara_scene::{CompiledSceneCandidate, SourceDocumentKey};
/// fn replace_source(candidate: &mut CompiledSceneCandidate, source: SourceDocumentKey) {
///     candidate.source_key = source;
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct CompiledSceneCandidate {
    source_key: SourceDocumentKey,
    document: CompiledSceneDocument,
}

impl CompiledSceneCandidate {
    #[must_use]
    pub const fn source_key(&self) -> &SourceDocumentKey {
        &self.source_key
    }

    #[must_use]
    pub const fn document(&self) -> &CompiledSceneDocument {
        &self.document
    }

    #[must_use]
    pub fn canonical_provenance(&self) -> &SceneEntityProvenanceMap {
        self.document.canonical_provenance()
    }
}

/// Owner-budget refusal remains distinct from rejection of the complete flat document contract.
#[derive(Debug)]
pub(crate) enum CompiledSceneAdmissionError<E> {
    Admission(E),
    Diagnostics(DiagnosticReport),
}

/// Consumes an unpublished compiler draft after complete codec-free flat admission.
///
/// The caller's ledger admits cumulative modeled allocation requests, with `None` denoting an
/// overflow. Existing input allocations move into the result. Validation scratch, replacement
/// record containers and selected-owner strings are admitted before their respective allocations.
/// Source-dependent defaults must already have been lowered and removed by their owning compiler.
pub(crate) fn admit_draft<E>(
    document: SceneDraft,
    canonical_provenance: SceneEntityProvenanceMap,
    authority: SchemaAuthority<'_>,
    limits: ComponentValueMaterializationLimits,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut admit: impl FnMut(usize, usize, Option<usize>) -> Result<(), E>,
) -> Result<CompiledSceneDocument, CompiledSceneAdmissionError<E>> {
    use claims::{FlatSceneClaims, FlatSceneComponentClaim, FlatSceneEntityClaim};

    // Instance declarations cannot enter a flat draft. Validate the remaining source metadata
    // and owner rule before stamping the actual selection into an exact flat claim.
    for entity in &document.entities {
        for (component, record) in &entity.components {
            if !record.contextual_defaults.is_empty() {
                return Err(match authority.schema(component) {
                    Some(schema) => component_value_refused(
                        &entity.id,
                        component,
                        schema,
                        &ComponentCodecError::UnresolvedAuthoredReference,
                    ),
                    None => component_refused(
                        &entity.id,
                        component,
                        "scene.unknown-component",
                        "Component type is not registered",
                    ),
                });
            }
            if authority.schema(component).is_some() && !authority.owner_matches(component, record)
            {
                return Err(component_refused(
                    &entity.id,
                    component,
                    "scene.component-owner-mismatch",
                    "Persistent component owner does not match the admitted schema owner",
                ));
            }
            if record.owner().is_none()
                && let Some(owner) = authority.selected_owner_name(component)
            {
                charge(
                    owner.len().checked_add(2 * size_of::<usize>()),
                    &mut |bytes| admit(0, 0, bytes),
                )?;
            }
        }
    }
    charge(
        document
            .entities
            .len()
            .checked_mul(size_of::<FlatSceneEntityClaim>())
            .and_then(|bytes| {
                document.entities.iter().try_fold(bytes, |bytes, entity| {
                    bytes.checked_add(entity.components.len().checked_mul(
                        size_of::<(ComponentTypeId, FlatSceneComponentClaim)>()
                            + 6 * size_of::<usize>(),
                    )?)
                })
            }),
        &mut |bytes| admit(0, 0, bytes),
    )?;
    let mut entities = Vec::with_capacity(document.entities.len());
    for entity in document.entities {
        let mut components = BTreeMap::new();
        for (component, record) in entity.components {
            let schema_owner = match record.owner {
                Some(owner) => Some(owner),
                None => authority
                    .selected_owner_name(&component)
                    .map(PersistentComponentSchemaOwnerId::try_new)
                    .transpose()
                    .map_err(|_| {
                        component_refused(
                            &entity.id,
                            &component,
                            "scene.compiled-owner-invalid",
                            "Compiled Scene selected Schema owner is invalid",
                        )
                    })?,
            };
            components.insert(
                component,
                FlatSceneComponentClaim {
                    schema_owner,
                    version: record.version,
                    value: record.value,
                },
            );
        }
        entities.push(FlatSceneEntityClaim {
            id: entity.id,
            children: entity.children,
            components,
        });
    }
    admit_claims(
        FlatSceneClaims {
            roots: document.roots,
            entities,
            canonical_provenance,
        },
        authority,
        limits,
        asset_resolution,
        false,
        admit,
    )
}

/// Consumes bounded flat claims from a concrete producer. No source record or default evaluator
/// participates; the same pure value and hierarchy kernel serves both producers.
pub(crate) fn admit_claims<E>(
    mut claims: claims::FlatSceneClaims,
    authority: SchemaAuthority<'_>,
    limits: ComponentValueMaterializationLimits,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    require_runtime_products: bool,
    mut admit: impl FnMut(usize, usize, Option<usize>) -> Result<(), E>,
) -> Result<CompiledSceneDocument, CompiledSceneAdmissionError<E>> {
    admit_validation_scratch(&claims, &mut |bytes| admit(0, 0, bytes))?;
    admit_canonical_provenance(&claims, &claims.canonical_provenance, &mut |bytes| {
        admit(0, 0, bytes)
    })?;
    claims
        .entities
        .sort_unstable_by(|left, right| left.id.cmp(&right.id));
    let mut context = ComponentDecodeContext::new().with_materialization_limits(limits);
    if let Some(resolver) = asset_resolution {
        context = context.with_asset_resolution(resolver);
    }
    let validation = crate::validation::validate_flat_claims_with_admission(
        &claims,
        authority,
        &mut context,
        require_runtime_products,
        |nodes, bytes, allocation| admit(nodes, bytes, Some(allocation)),
    )
    .map_err(CompiledSceneAdmissionError::Admission)?;
    if validation.diagnostics.has_errors() {
        return Err(CompiledSceneAdmissionError::Diagnostics(
            validation.diagnostics,
        ));
    }
    for entity in &claims.entities {
        for (component, record) in &entity.components {
            if authority
                .current_flat_schema(component, record.schema_owner.as_ref(), record.version)
                .is_err()
            {
                return Err(component_refused(
                    &entity.id,
                    component,
                    "scene.compiled-component-unavailable",
                    "Compiled Scene requires an admitted current component definition",
                ));
            }
        }
    }
    charge(
        claims
            .entities
            .len()
            .checked_mul(size_of::<CompiledSceneEntity>())
            .and_then(|bytes| {
                claims.entities.iter().try_fold(bytes, |bytes, entity| {
                    bytes.checked_add(entity.components.len().checked_mul(
                        size_of::<(ComponentTypeId, CompiledSceneComponent)>()
                            + 6 * size_of::<usize>(),
                    )?)
                })
            }),
        &mut |bytes| admit(0, 0, bytes),
    )?;
    // Immutable hierarchy arrays have no spare capacity. Moving a caller-owned Vec can otherwise
    // retain arbitrary unobservable capacity behind a slice. Admit potential buffer compaction
    // before converting those arrays; their IDs and values still move without cloning.
    let compaction = claims.entities.iter().fold(
        if claims.roots.capacity() == claims.roots.len() {
            Some(0)
        } else {
            claims.roots.len().checked_mul(size_of::<SceneEntityId>())
        },
        |bytes, entity| {
            bytes.and_then(|bytes| {
                if entity.children.capacity() == entity.children.len() {
                    Some(bytes)
                } else {
                    bytes.checked_add(
                        entity
                            .children
                            .len()
                            .checked_mul(size_of::<SceneEntityId>())?,
                    )
                }
            })
        },
    );
    charge(compaction, &mut |bytes| admit(0, 0, bytes))?;
    let entities = claims
        .entities
        .into_iter()
        .map(|entity| CompiledSceneEntity {
            id: entity.id,
            children: entity.children.into_boxed_slice(),
            components: entity
                .components
                .into_iter()
                .map(|(component, claim)| {
                    (
                        component,
                        CompiledSceneComponent {
                            schema_owner: claim.schema_owner,
                            version: claim.version,
                            value: claim.value,
                        },
                    )
                })
                .collect(),
        })
        .collect::<Box<[_]>>();
    Ok(CompiledSceneDocument {
        roots: claims.roots.into_boxed_slice(),
        entities,
        canonical_provenance: claims.canonical_provenance,
    })
}

fn admit_canonical_provenance<E>(
    document: &claims::FlatSceneClaims,
    provenance: &SceneEntityProvenanceMap,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<(), CompiledSceneAdmissionError<E>> {
    if validate_flat_scene_provenance(document).is_err() {
        return Err(refused(
            "scene.invalid-provenance",
            "Compiled Scene canonical provenance is inconsistent",
        ));
    }
    for origin in provenance.values() {
        let Some(path) = origin.projection_path() else {
            continue;
        };
        for step in path.steps() {
            let PrefabProjectionStep::SourceEntity {
                source_document,
                canonical_source,
                ..
            } = step
            else {
                continue;
            };
            // The compiler admits asset associations for Named and ProjectLocation sources.
            // Only a StableAsset document key directly claims the lookup product's source UUID.
            if canonical_source.kind() != SourceDocumentKind::Prefab
                || matches!(
                    canonical_source.origin(),
                    SourceDocumentOrigin::StableAsset { stable_id } if stable_id.as_uuid().is_nil()
                )
                || !source_document.is_primary()
                || source_document.as_product().is_some_and(|reference| {
                    reference.source().as_uuid().is_nil()
                        || matches!(
                            canonical_source.origin(),
                            SourceDocumentOrigin::StableAsset { stable_id }
                                if *stable_id != reference.source()
                        )
                })
            {
                return Err(refused(
                    "scene.invalid-provenance",
                    "Compiled Scene provenance requires valid Prefab source ownership",
                ));
            }
        }
    }

    // Each key borrows its complete address. Admit all tree entries before the first insertion;
    // lookup aliases and private materialization locators never contribute to address identity.
    charge(
        provenance
            .len()
            .checked_mul(size_of::<CanonicalSceneEntityAddress<'_>>() + 6 * size_of::<usize>()),
        admit,
    )?;
    let mut addresses = BTreeSet::new();
    for origin in provenance.values() {
        if !addresses.insert(CanonicalSceneEntityAddress::new(origin)) {
            return Err(refused(
                "scene.compiled-duplicate-logical-address",
                "Compiled Scene entities must have distinct canonical logical addresses",
            ));
        }
    }
    Ok(())
}

fn admit_validation_scratch<E>(
    document: &claims::FlatSceneClaims,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<(), CompiledSceneAdmissionError<E>> {
    // The existing hierarchy checks own ID indexes, parent/path copies and sorted record
    // references. Use the compiler's conservative 512-byte structural-entry model, with
    // eight ID copies covering those indexes and transient cycle paths. Complete values move
    // into storage; their validation admits actual iterator/index scratch without reserving an
    // imaginary second payload tree.
    let scratch = (|| {
        let mut bytes = document.entities.len().checked_mul(512)?;
        for root in &document.roots {
            bytes = bytes
                .checked_add(512)?
                .checked_add(root.as_str().len().checked_mul(8)?)?;
        }
        for entity in &document.entities {
            bytes = bytes.checked_add(entity.id.as_str().len().checked_mul(8)?)?;
            for child in &entity.children {
                bytes = bytes
                    .checked_add(512)?
                    .checked_add(child.as_str().len().checked_mul(8)?)?
                    .checked_add(entity.id.as_str().len().checked_mul(8)?)?;
            }
            for component in entity.components.keys() {
                bytes = bytes
                    .checked_add(512)?
                    .checked_add(component.as_str().len())?;
            }
        }
        Some(bytes)
    })();
    charge(scratch, admit)?;

    Ok(())
}

fn charge<E>(
    bytes: Option<usize>,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<(), CompiledSceneAdmissionError<E>> {
    admit(bytes).map_err(CompiledSceneAdmissionError::Admission)?;
    if bytes.is_none() {
        return Err(refused(
            "scene.compiled-working-overflow",
            "Compiled Scene working-storage accounting overflowed",
        ));
    }
    Ok(())
}

fn refused<E>(code: &'static str, summary: &'static str) -> CompiledSceneAdmissionError<E> {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(error(code, summary));
    CompiledSceneAdmissionError::Diagnostics(diagnostics)
}

fn component_refused<E>(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    code: &'static str,
    summary: &'static str,
) -> CompiledSceneAdmissionError<E> {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(with_public_locator(
        with_public_locator(error(code, summary), "entity-id", entity.as_str()),
        "component-id",
        component.as_str(),
    ));
    CompiledSceneAdmissionError::Diagnostics(diagnostics)
}

fn component_value_refused<E>(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    schema: &ComponentSchema,
    codec_error: &ComponentCodecError,
) -> CompiledSceneAdmissionError<E> {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(with_public_locator(
        with_public_locator(
            with_codec_error(
                error(
                    "scene.invalid-component-payload",
                    "Component payload is invalid",
                ),
                codec_error,
                Some(schema),
            ),
            "entity-id",
            entity.as_str(),
        ),
        "component-id",
        component.as_str(),
    ));
    CompiledSceneAdmissionError::Diagnostics(diagnostics)
}

#[cfg(test)]
mod tests;
