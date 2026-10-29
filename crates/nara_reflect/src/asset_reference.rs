use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{
    AssetPathError, AssetProductIdentityError, AssetProductRef, AssetRef, ImportedProductId,
    StableAssetId, StableAssetIdError,
};

use crate::{
    ComponentCodecError, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldSchema, ComponentRegistry,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueShape, EntityReferenceTraversalLimits,
    effective_component_value,
    entity_reference::{
        PresentReferenceLeaf, ReferenceTraversalError, collect_present_field_reference_leaves,
        collect_present_field_reference_leaves_with_admission, collect_present_reference_leaves,
        collect_present_reference_leaves_with_admission, collect_present_value_reference_leaves,
        collect_present_value_reference_leaves_with_admission,
    },
};

mod rewrite;

pub use rewrite::{
    AssetReferenceRewriteError, rewrite_declared_asset_references,
    rewrite_declared_asset_references_with_admission,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAssetReference {
    field_id: ComponentFieldId,
    path: ComponentFieldPath,
    asset_ref: AssetRef,
}

impl DeclaredAssetReference {
    #[must_use]
    pub const fn field_id(&self) -> &ComponentFieldId {
        &self.field_id
    }

    #[must_use]
    pub const fn path(&self) -> &ComponentFieldPath {
        &self.path
    }

    /// Returns the complete declared reference, including its source and product selector.
    #[must_use]
    pub const fn asset_ref(&self) -> &AssetRef {
        &self.asset_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredAssetReferenceError {
    RegistryNotFrozen,
    UnknownComponent {
        component: ComponentTypeId,
    },
    SchemaVersionMismatch {
        component: ComponentTypeId,
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    FieldPath {
        component: ComponentTypeId,
        field: ComponentFieldId,
        source: ComponentFieldPathError,
    },
    RequiredFieldIsNull {
        component: ComponentTypeId,
        field: ComponentFieldId,
    },
    InvalidValueShape {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
    },
    InvalidKind {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
    },
    InvalidPath {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
        source: AssetPathError,
    },
    InvalidStableId {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
        source: StableAssetIdError,
    },
    InvalidProductSource {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
    },
    InvalidProductIdentity {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
        source: AssetProductIdentityError,
    },
    Materialization {
        component: ComponentTypeId,
        source: ComponentCodecError,
    },
    Traversal {
        component: ComponentTypeId,
        source: ComponentEntityReferenceRewriteError<()>,
    },
}

impl Display for DeclaredAssetReferenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RegistryNotFrozen => {
                "component registry must be frozen before asset reference traversal"
            }
            Self::UnknownComponent { .. } => {
                "asset reference traversal targeted an unknown component"
            }
            Self::SchemaVersionMismatch { .. } => {
                "asset reference traversal requires the current component schema version"
            }
            Self::FieldPath { .. } => {
                "declared asset reference field could not be resolved in the component value"
            }
            Self::RequiredFieldIsNull { .. } => "required asset reference field contains null",
            Self::InvalidValueShape { .. } => {
                "declared asset reference field has an invalid value shape"
            }
            Self::InvalidKind { .. } => {
                "declared asset reference field has an unsupported reference kind"
            }
            Self::InvalidPath { .. } => "declared asset reference contains an invalid path",
            Self::InvalidStableId { .. } => {
                "declared asset reference contains an invalid stable ID"
            }
            Self::InvalidProductSource { .. } => {
                "declared product reference requires a canonical source UUID"
            }
            Self::InvalidProductIdentity { .. } => {
                "declared asset reference contains an invalid product identity"
            }
            Self::Materialization { .. } => {
                "declared asset reference default materialization rejected the effective value"
            }
            Self::Traversal { .. } => {
                "declared asset reference traversal rejected component structure"
            }
        })
    }
}

impl Error for DeclaredAssetReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FieldPath { source, .. } => Some(source),
            Self::InvalidPath { source, .. } => Some(source),
            Self::InvalidStableId { source, .. } => Some(source),
            Self::InvalidProductIdentity { source, .. } => Some(source),
            Self::Materialization { source, .. } => Some(source),
            Self::RegistryNotFrozen
            | Self::UnknownComponent { .. }
            | Self::SchemaVersionMismatch { .. }
            | Self::RequiredFieldIsNull { .. }
            | Self::InvalidValueShape { .. }
            | Self::InvalidKind { .. }
            | Self::InvalidProductSource { .. }
            | Self::Traversal { .. } => None,
        }
    }
}

/// Keeps the caller's allocation failure separate from schema/reference interpretation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetReferenceCollectionError<E> {
    Reference(DeclaredAssetReferenceError),
    Admission(E),
}

/// Collects assets from a borrowed, already normalized value and an admitted current schema.
///
/// The Scene compiler owns schema/value admission and default materialization. This entry never
/// fills defaults or clones the input tree. The callback receives cumulative-work deltas:
/// visited nodes, logical bytes, and modeled allocation bytes. Scratch growth and each owned
/// output are admitted before allocation. Allocation includes the returned Vec's slots, leaf
/// scratch, paths, field IDs, parsed asset paths (or failed legacy stable-ID text), and bounded
/// canonical product UUID parser scratch; a caller's aggregate Vec is separate.
/// Whole new Vec requests are charged on growth, with no refunds for discarded scratch.
///
/// A denied callback returns its exact error without allocating an error path. No callback
/// reservation is made from the configured worst-case limits.
pub fn collect_declared_effective_asset_references_with_admission<E>(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    let component = schema.id();
    if version != schema.version() {
        admit(0, 0, component.as_str().len()).map_err(AssetReferenceCollectionError::Admission)?;
        return Err(AssetReferenceCollectionError::Reference(
            DeclaredAssetReferenceError::SchemaVersionMismatch {
                component: component.clone(),
                expected: schema.version(),
                actual: version,
            },
        ));
    }
    let leaves = collect_present_reference_leaves_with_admission::<(), E>(
        schema,
        value,
        limits,
        ComponentValueKind::AssetRef,
        true,
        &mut admit,
    );
    materialize_asset_references_with_admission(component, leaves, &mut admit)
}

fn materialize_asset_references_with_admission<E>(
    component: &ComponentTypeId,
    leaves: Result<Vec<PresentReferenceLeaf<'_, '_>>, ReferenceTraversalError<(), E>>,
    admit: &mut dyn FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    let leaves = match leaves {
        Ok(leaves) => leaves,
        Err(ReferenceTraversalError::Admission(error)) => {
            return Err(AssetReferenceCollectionError::Admission(error));
        }
        Err(ReferenceTraversalError::Reference(source)) => {
            admit(0, 0, component.as_str().len())
                .map_err(AssetReferenceCollectionError::Admission)?;
            return Err(AssetReferenceCollectionError::Reference(
                DeclaredAssetReferenceError::Traversal {
                    component: component.clone(),
                    source,
                },
            ));
        }
    };
    // Each slot, field ID and parsed path was admitted at the borrowed record boundary.
    let mut references = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        if matches!(leaf.value, ComponentValue::Null) {
            admit(0, 0, component.as_str().len())
                .map_err(AssetReferenceCollectionError::Admission)?;
            return Err(AssetReferenceCollectionError::Reference(
                DeclaredAssetReferenceError::RequiredFieldIsNull {
                    component: component.clone(),
                    field: leaf.field.id().clone(),
                },
            ));
        }
        let asset_ref = match decode_asset_reference(leaf.value) {
            Ok(reference) => reference,
            Err(error) => {
                admit(0, 0, component.as_str().len())
                    .map_err(AssetReferenceCollectionError::Admission)?;
                let component = component.clone();
                let field = leaf.field.id().clone();
                let path = leaf.path;
                return Err(AssetReferenceCollectionError::Reference(match error {
                    AssetReferenceValueError::Shape => {
                        DeclaredAssetReferenceError::InvalidValueShape {
                            component,
                            field,
                            path,
                        }
                    }
                    AssetReferenceValueError::Kind => DeclaredAssetReferenceError::InvalidKind {
                        component,
                        field,
                        path,
                    },
                    AssetReferenceValueError::Path(source) => {
                        DeclaredAssetReferenceError::InvalidPath {
                            component,
                            field,
                            path,
                            source,
                        }
                    }
                    AssetReferenceValueError::StableId(source) => {
                        DeclaredAssetReferenceError::InvalidStableId {
                            component,
                            field,
                            path,
                            source,
                        }
                    }
                    AssetReferenceValueError::ProductSource => {
                        DeclaredAssetReferenceError::InvalidProductSource {
                            component,
                            field,
                            path,
                        }
                    }
                    AssetReferenceValueError::ProductIdentity(source) => {
                        DeclaredAssetReferenceError::InvalidProductIdentity {
                            component,
                            field,
                            path,
                            source,
                        }
                    }
                }));
            }
        };
        references.push(DeclaredAssetReference {
            field_id: leaf.field.id().clone(),
            path: leaf.path,
            asset_ref,
        });
    }
    Ok(references)
}

/// Collects schema-declared effective asset values using the convenience interpretation limits.
pub fn collect_declared_asset_references(
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_asset_references_with_limits(
        registry,
        component,
        version,
        value,
        ComponentValueMaterializationLimits::default(),
        EntityReferenceTraversalLimits::default(),
    )
}

/// Collects effective asset values with explicit expansion and reference-work budgets.
pub fn collect_declared_asset_references_with_limits(
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    materialization_limits: ComponentValueMaterializationLimits,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    if !registry.is_frozen() {
        return Err(DeclaredAssetReferenceError::RegistryNotFrozen);
    }
    let schema = registry.schema(component).ok_or_else(|| {
        DeclaredAssetReferenceError::UnknownComponent {
            component: component.clone(),
        }
    })?;
    if version != schema.version() {
        return Err(DeclaredAssetReferenceError::SchemaVersionMismatch {
            component: component.clone(),
            expected: schema.version(),
            actual: version,
        });
    }

    collect_declared_asset_references_from_schema_with_limits(
        schema,
        version,
        value,
        materialization_limits,
        traversal_limits,
    )
}

/// Collects schema-declared asset references without requiring executable registry authority.
///
/// The caller must already own an immutable, authenticated schema catalog. This function only
/// interprets the supplied data schema and generic component value; it does not grant native codec,
/// migration, provider, or Runtime authority.
pub fn collect_declared_asset_references_from_schema(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_asset_references_from_schema_with_limits(
        schema,
        version,
        value,
        ComponentValueMaterializationLimits::default(),
        EntityReferenceTraversalLimits::default(),
    )
}

/// Interprets an authenticated current schema using separate expansion and reference-work limits.
///
/// This fills defaults but does not validate unrelated required fields or grant native authority.
/// Returned references own their values; the caller's authored tree remains unchanged.
pub fn collect_declared_asset_references_from_schema_with_limits(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    materialization_limits: ComponentValueMaterializationLimits,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let component = schema.id();
    if version != schema.version() {
        return Err(DeclaredAssetReferenceError::SchemaVersionMismatch {
            component: component.clone(),
            expected: schema.version(),
            actual: version,
        });
    }

    let effective =
        effective_component_value(schema, value, materialization_limits).map_err(|source| {
            DeclaredAssetReferenceError::Materialization {
                component: component.clone(),
                source,
            }
        })?;
    let leaves = collect_present_reference_leaves::<()>(
        schema,
        effective.as_ref(),
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, effective.as_ref(), leaves, 0)
}

/// Collects references from an already selected field value, retaining component-relative paths.
///
/// The caller supplies the actual field subtree, as a SetField operation does. Missing defaults
/// and reference target existence remain the owning transaction's responsibility.
pub fn collect_declared_field_asset_references(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_field_asset_references_with_limits(
        component,
        field,
        value,
        EntityReferenceTraversalLimits::default(),
    )
}

/// Collects references from a selected field subtree with explicit reference-work limits.
///
/// Paths remain component-relative. This does not fill defaults or grant native authority;
/// reference target existence remains the owning transaction's responsibility.
pub fn collect_declared_field_asset_references_with_limits(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let leaves = collect_present_field_reference_leaves::<()>(
        field,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, value, leaves, field.path().segments().len())
}

/// Collects assets from a selected field under an already admitted schema.
///
/// The borrowed value is the actual field subtree; returned paths remain component-relative.
/// Defaults are not filled. The callback receives work deltas (nodes, logical bytes, modeled
/// allocation bytes) at the same pre-growth and pre-copy boundaries as the effective collector.
/// Output slots, field IDs and parsed asset paths are admitted before materialization. A refusal
/// returns the exact caller error without allocating an error path or cloning the input.
pub fn collect_declared_field_asset_references_with_admission<E>(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    let leaves = collect_present_field_reference_leaves_with_admission::<(), E>(
        field,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
        true,
        &mut admit,
    );
    materialize_asset_references_with_admission(component, leaves, &mut admit)
}

/// Collects declared assets relative to a selected replacement payload, with explicit limits.
///
/// The caller supplies an admitted owning field and its selected shape, for example from
/// [`crate::resolve_component_property_schema_claim`]. Identity, metadata and capabilities remain
/// those of the original field. Returned paths start at the payload; a scalar asset occupies the
/// empty path. Input and returned reference work share the traversal limits.
///
/// This does not resolve a target, prove that a selected value exists, infer dependencies from
/// undeclared wire values, fill defaults or construct a complete component value. Shape affinity,
/// binding, edit and native admission remain the caller's responsibility.
pub fn collect_declared_value_asset_references_with_limits(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let leaves = collect_present_value_reference_leaves::<()>(
        field,
        shape,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, value, leaves, 0)
}

/// Collects assets from a borrowed selected shape and replacement payload with work admission.
///
/// The caller supplies an already admitted owning field and selected shape. Paths are relative
/// to the payload, including the empty path for a scalar asset. This shares the field collector's
/// pre-allocation admission and exact caller-error propagation without copying the shape, filling
/// defaults, constructing a component tree or reserving from configured worst-case limits.
pub fn collect_declared_value_asset_references_with_admission<E>(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    let leaves = collect_present_value_reference_leaves_with_admission::<(), E>(
        field,
        shape,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
        true,
        &mut admit,
    );
    materialize_asset_references_with_admission(component, leaves, &mut admit)
}

fn materialize_asset_references(
    component: &ComponentTypeId,
    value: &ComponentValue,
    leaves: Vec<PresentReferenceLeaf<'_, '_>>,
    root_segments: usize,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let mut references = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let relative =
            ComponentFieldPath::new(leaf.path.segments()[root_segments..].iter().cloned());
        let field_value =
            value
                .get_path(&relative)
                .map_err(|source| DeclaredAssetReferenceError::FieldPath {
                    component: component.clone(),
                    field: leaf.field.id().clone(),
                    source,
                })?;
        if matches!(field_value, ComponentValue::Null) {
            return Err(DeclaredAssetReferenceError::RequiredFieldIsNull {
                component: component.clone(),
                field: leaf.field.id().clone(),
            });
        }
        let asset_ref = parse_asset_reference(component, leaf.field.id(), &leaf.path, field_value)?;
        references.push(DeclaredAssetReference {
            field_id: leaf.field.id().clone(),
            path: leaf.path,
            asset_ref,
        });
    }
    Ok(references)
}

fn parse_asset_reference(
    component: &ComponentTypeId,
    field: &ComponentFieldId,
    path: &ComponentFieldPath,
    value: &ComponentValue,
) -> Result<AssetRef, DeclaredAssetReferenceError> {
    decode_asset_reference(value).map_err(|error| match error {
        AssetReferenceValueError::Shape => DeclaredAssetReferenceError::InvalidValueShape {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
        },
        AssetReferenceValueError::Kind => DeclaredAssetReferenceError::InvalidKind {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
        },
        AssetReferenceValueError::Path(source) => DeclaredAssetReferenceError::InvalidPath {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
            source,
        },
        AssetReferenceValueError::StableId(source) => {
            DeclaredAssetReferenceError::InvalidStableId {
                component: component.clone(),
                field: field.clone(),
                path: path.clone(),
                source,
            }
        }
        AssetReferenceValueError::ProductSource => {
            DeclaredAssetReferenceError::InvalidProductSource {
                component: component.clone(),
                field: field.clone(),
                path: path.clone(),
            }
        }
        AssetReferenceValueError::ProductIdentity(source) => {
            DeclaredAssetReferenceError::InvalidProductIdentity {
                component: component.clone(),
                field: field.clone(),
                path: path.clone(),
                source,
            }
        }
    })
}

#[derive(Debug)]
enum AssetReferenceValueError {
    Shape,
    Kind,
    Path(AssetPathError),
    StableId(StableAssetIdError),
    ProductSource,
    ProductIdentity(AssetProductIdentityError),
}

fn decode_asset_reference(value: &ComponentValue) -> Result<AssetRef, AssetReferenceValueError> {
    match asset_reference_parts(value)? {
        AssetReferenceParts::Path(payload) => {
            AssetRef::path(payload).map_err(AssetReferenceValueError::Path)
        }
        AssetReferenceParts::StableId(payload) => {
            AssetRef::stable_id(payload).map_err(AssetReferenceValueError::StableId)
        }
        AssetReferenceParts::Product { source, subproduct } => {
            // Bound UUID parser/error scratch before parsing. The old StableId spelling remains
            // unchanged; only the structured product form requires canonical source text.
            if source.len() != 36 {
                return Err(AssetReferenceValueError::ProductSource);
            }
            let source_id = StableAssetId::parse_str(source)
                .map_err(|_| AssetReferenceValueError::ProductSource)?;
            if source_id.to_string() != source {
                return Err(AssetReferenceValueError::ProductSource);
            }
            let product = match subproduct {
                None => ImportedProductId::PRIMARY,
                Some(id) => ImportedProductId::parse_subproduct(id)
                    .map_err(AssetReferenceValueError::ProductIdentity)?,
            };
            AssetProductRef::new(source_id, product)
                .map(AssetRef::product)
                .map_err(AssetReferenceValueError::ProductIdentity)
        }
    }
}

/// Decodes the canonical asset-reference leaf used by persistent fields.
///
/// This checks reference syntax only. The caller owns value-size admission, source resolution
/// and product existence. Budgeted Schema consumers should use the reference collectors.
pub fn decode_asset_reference_value(
    value: &ComponentValue,
) -> Result<AssetRef, ComponentCodecError> {
    decode_asset_reference(value).map_err(|_| {
        ComponentCodecError::invalid_field("<asset-ref>", "a canonical admitted asset reference")
    })
}

/// Returns the logical allocation bound for decoding one asset-reference leaf.
///
/// This borrows the closed wire record without allocating. It covers owned path text and
/// parser scratch, including failed identity parsing, but excludes the input and error
/// diagnostics. `None` means the wire shape refuses before payload decoding. A returned bound
/// does not validate reference syntax, resolve a source or prove product existence.
#[must_use]
pub fn asset_reference_decode_work_bytes(value: &ComponentValue) -> Option<usize> {
    asset_reference_value_work(value).map(|work| work.decode_allocation_bytes)
}

/// Encodes one complete persistent asset-reference leaf without resolving its target.
///
/// Path and stable source selectors name the primary product. Product selectors retain their
/// complete source and product identity. Callers with aggregate allocation limits admit the
/// complete value cost before calling; the budgeted rewrite entry handles that admission.
#[must_use]
pub fn encode_asset_reference_value(reference: &AssetRef) -> ComponentValue {
    let (kind, value) = match reference {
        AssetRef::Path(path) => ("path", ComponentValue::String(path.as_str().to_owned())),
        AssetRef::StableId(id) => ("stable_id", ComponentValue::String(id.to_string())),
        AssetRef::Product(reference) => {
            let product = match reference.product().as_subproduct() {
                None => {
                    ComponentValue::map([("kind", ComponentValue::String("primary".to_owned()))])
                }
                Some(id) => ComponentValue::map([
                    ("kind", ComponentValue::String("subproduct".to_owned())),
                    ("id", ComponentValue::String(id.to_string())),
                ]),
            };
            (
                "product",
                ComponentValue::map([
                    (
                        "source",
                        ComponentValue::String(reference.source().to_string()),
                    ),
                    ("product", product),
                ]),
            )
        }
    };
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_owned())),
        ("value", value),
    ])
}

pub(crate) fn is_asset_reference_value(value: &ComponentValue) -> bool {
    decode_asset_reference(value).is_ok()
}

enum AssetReferenceParts<'a> {
    Path(&'a str),
    StableId(&'a str),
    Product {
        source: &'a str,
        subproduct: Option<&'a str>,
    },
}

fn asset_reference_parts(
    value: &ComponentValue,
) -> Result<AssetReferenceParts<'_>, AssetReferenceValueError> {
    let ComponentValue::Map(fields) = value else {
        return Err(AssetReferenceValueError::Shape);
    };
    if fields.len() != 2 {
        return Err(AssetReferenceValueError::Shape);
    }
    let (Some(ComponentValue::String(kind)), Some(value)) =
        (fields.get("kind"), fields.get("value"))
    else {
        return Err(AssetReferenceValueError::Shape);
    };
    match (kind.as_str(), value) {
        ("path", ComponentValue::String(payload)) => Ok(AssetReferenceParts::Path(payload)),
        ("stable_id", ComponentValue::String(payload)) => {
            Ok(AssetReferenceParts::StableId(payload))
        }
        ("product", ComponentValue::Map(payload)) if payload.len() == 2 => {
            let (Some(ComponentValue::String(source)), Some(ComponentValue::Map(product))) =
                (payload.get("source"), payload.get("product"))
            else {
                return Err(AssetReferenceValueError::Shape);
            };
            let Some(ComponentValue::String(kind)) = product.get("kind") else {
                return Err(AssetReferenceValueError::Shape);
            };
            let subproduct = match kind.as_str() {
                "primary" if product.len() == 1 => None,
                "subproduct" if product.len() == 2 => {
                    let Some(ComponentValue::String(id)) = product.get("id") else {
                        return Err(AssetReferenceValueError::Shape);
                    };
                    Some(id.as_str())
                }
                "primary" | "subproduct" => return Err(AssetReferenceValueError::Shape),
                _ => return Err(AssetReferenceValueError::Kind),
            };
            Ok(AssetReferenceParts::Product { source, subproduct })
        }
        ("path" | "stable_id" | "product", _) => Err(AssetReferenceValueError::Shape),
        _ => Err(AssetReferenceValueError::Kind),
    }
}

/// Actual canonical leaf work, shared by reference traversal, validation and rewriting. This
/// inspects the closed wire records only; it never walks Schema or allocates parser scratch.
pub(crate) struct AssetReferenceValueWork {
    pub(crate) nodes: usize,
    pub(crate) depth: usize,
    pub(crate) logical_bytes: usize,
    pub(crate) payload_bytes: usize,
    pub(crate) decode_allocation_bytes: usize,
    pub(crate) map_entries: usize,
}

impl AssetReferenceValueWork {
    fn text(kind_bytes: usize, payload_bytes: usize) -> Self {
        Self {
            nodes: 3,
            depth: 2,
            logical_bytes: 9usize
                .saturating_add(kind_bytes)
                .saturating_add(payload_bytes),
            payload_bytes,
            // AssetPath owns its text; failed legacy StableId parsing retains its input.
            decode_allocation_bytes: payload_bytes,
            map_entries: 2,
        }
    }

    fn product(source_bytes: usize, subproduct_bytes: Option<usize>) -> Self {
        let (nodes, map_entries, selector_bytes) = match subproduct_bytes {
            None => (6, 5, 4usize + 7),
            Some(bytes) => (7, 6, (4usize + 10 + 2).saturating_add(bytes)),
        };
        let payload_bytes = (6usize + 7)
            .saturating_add(source_bytes)
            .saturating_add(selector_bytes);
        Self {
            nodes,
            depth: 4,
            logical_bytes: (9usize + 7).saturating_add(payload_bytes),
            payload_bytes,
            // Canonical UUID checks allocate at most one 36-byte source string and one
            // 36-byte product string. Wrong-length input refuses without copying it.
            decode_allocation_bytes: usize::from(source_bytes == 36) * 36
                + usize::from(subproduct_bytes == Some(36)) * 36,
            map_entries,
        }
    }
}

pub(crate) fn asset_reference_value_work(
    value: &ComponentValue,
) -> Option<AssetReferenceValueWork> {
    Some(match asset_reference_parts(value).ok()? {
        AssetReferenceParts::Path(payload) => AssetReferenceValueWork::text(4, payload.len()),
        AssetReferenceParts::StableId(payload) => AssetReferenceValueWork::text(9, payload.len()),
        AssetReferenceParts::Product { source, subproduct } => {
            AssetReferenceValueWork::product(source.len(), subproduct.map(str::len))
        }
    })
}

fn asset_reference_encoding_work(reference: &AssetRef) -> AssetReferenceValueWork {
    match reference {
        AssetRef::Path(path) => AssetReferenceValueWork::text(4, path.as_str().len()),
        AssetRef::StableId(_) => AssetReferenceValueWork::text(9, 36),
        AssetRef::Product(reference) => {
            AssetReferenceValueWork::product(36, reference.product().as_subproduct().map(|_| 36))
        }
    }
}

#[cfg(test)]
mod tests;
