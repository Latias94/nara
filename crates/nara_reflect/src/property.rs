//! Schema-relative property identity, independent of documents and runtime incarnations.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    mem::size_of,
};

use nara_core::{ByteLimit, ItemLimit};

use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentMemberPath,
    ComponentMemberPathSegment, ComponentSchema, ComponentSchemaVersion, ComponentShapeError,
    ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueRef,
    ComponentValueShape,
    effective::{
        EffectiveValueAdmissionError, validate_value_materialization_budget,
        validate_value_materialization_budget_with_admission,
    },
    schema::{
        MAX_SHAPE_BYTES, MAX_SHAPE_DEPTH, keyed_list_member, validate_component_shape_budget,
        validate_component_value_budget,
    },
    validate_component_value, validate_component_value_with_limits,
};

mod keyed_index;

pub use keyed_index::ComponentPropertyReadIndex;
pub(crate) use keyed_index::KeyedListLocations;

/// Exact logical identity within a keyed list. Reusing a key asserts identity continuity;
/// Reflect does not retain deleted item history or distinguish restoration from accidental reuse.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ComponentPropertyKey {
    String(String),
    I64(i64),
    U64(u64),
}

impl ComponentPropertyKey {
    #[must_use]
    pub fn from_value<'value>(value: impl Into<ComponentValueRef<'value>>) -> Option<Self> {
        let value = value.into();
        match value.kind() {
            ComponentValueKind::String => value.as_str().map(|value| Self::String(value.into())),
            ComponentValueKind::I64 => value.as_i64().map(Self::I64),
            ComponentValueKind::U64 => value.as_u64().map(Self::U64),
            _ => None,
        }
    }

    #[must_use]
    pub const fn value_kind(&self) -> ComponentValueKind {
        match self {
            Self::String(_) => ComponentValueKind::String,
            Self::I64(_) => ComponentValueKind::I64,
            Self::U64(_) => ComponentValueKind::U64,
        }
    }

    fn logical_bytes(&self) -> usize {
        match self {
            Self::String(key) => key.len(),
            Self::I64(_) | Self::U64(_) => 8,
        }
    }

    fn matches(&self, value: ComponentValueRef<'_>) -> bool {
        match self {
            Self::String(key) => value.as_str() == Some(key.as_str()),
            Self::I64(key) => value.as_i64() == Some(*key),
            Self::U64(key) => value.as_u64() == Some(*key),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ComponentPropertyStep {
    Member(ComponentFieldId),
    ListItem {
        by: ComponentFieldId,
        key: ComponentPropertyKey,
    },
    MapEntry(String),
}

/// A durable selector relative to one component schema, not a document or mutation authority.
///
/// Wire member names, list positions and schema versions are deliberately not identity.
/// Deserializers at file/transport boundaries must still enforce their allocation budgets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentPropertyPath {
    pub root: ComponentFieldId,
    pub steps: Vec<ComponentPropertyStep>,
}

impl ComponentPropertyPath {
    pub const MAX_STEPS: usize = MAX_SHAPE_DEPTH;

    #[must_use]
    pub fn new(root: ComponentFieldId) -> Self {
        Self {
            root,
            steps: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_steps(mut self, steps: impl IntoIterator<Item = ComponentPropertyStep>) -> Self {
        self.steps.extend(steps);
        self
    }

    pub fn validate(&self) -> Result<(), ComponentPropertyError> {
        self.logical_bytes().map(|_| ())
    }

    /// Validates the selector and returns its logical size for an owner's aggregate work budget.
    /// This counts identity strings and scalar key bytes, not serialized transport overhead.
    pub fn logical_bytes(&self) -> Result<usize, ComponentPropertyError> {
        if self.steps.len() > Self::MAX_STEPS {
            return Err(ComponentPropertyError::PathDepthLimit);
        }
        let mut bytes = self.root.as_str().len();
        self.root
            .validate()
            .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
        for step in &self.steps {
            bytes = bytes.saturating_add(match step {
                ComponentPropertyStep::Member(id) => {
                    id.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
                    id.as_str().len()
                }
                ComponentPropertyStep::ListItem { by, key } => {
                    by.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
                    by.as_str().len().saturating_add(key.logical_bytes())
                }
                ComponentPropertyStep::MapEntry(key) => key.len(),
            });
            if bytes > MAX_SHAPE_BYTES {
                return Err(ComponentPropertyError::PathByteLimit);
            }
        }
        Ok(bytes)
    }

    /// Replaces an existing property in a private source candidate and returns its inverse edit.
    ///
    /// The returned selector addresses the same storage target after the replacement, including
    /// an edited list identity. The supplied limits bound the previous and replacement values at
    /// that target. The owner admits the source and validates the complete candidate and defaults
    /// before publication. The candidate must be discarded on error.
    #[doc(hidden)]
    pub fn replace_in_candidate(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        candidate: &mut ComponentValue,
        replacement: ComponentValue,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<(ComponentPropertyPath, ComponentValue), ComponentPropertyError> {
        self.replace_candidate(schema, version, candidate, replacement, limits, &mut None)
    }

    /// Performs the same source replacement with cumulative work and allocation admission.
    ///
    /// The caller supplies a catalog-admitted schema and an owned replacement. Callback deltas
    /// include selector lookup, path storage, validation scratch and the returned selector or
    /// diagnostic. Refusal returns the exact caller error. Discard the private candidate on error.
    #[doc(hidden)]
    pub fn replace_in_candidate_with_admission<E>(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        candidate: &mut ComponentValue,
        replacement: ComponentValue,
        limits: ComponentValueMaterializationLimits,
        mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
    ) -> Result<(ComponentPropertyPath, ComponentValue), ComponentPropertyAdmissionError<E>> {
        let mut refusal = None;
        let result = {
            let mut callback = |nodes, bytes, allocated| match admit(nodes, bytes, allocated) {
                Ok(()) => true,
                Err(error) => {
                    refusal = Some(error);
                    false
                }
            };
            self.replace_candidate(
                schema,
                version,
                candidate,
                replacement,
                limits,
                &mut Some(&mut callback),
            )
        };
        match refusal {
            Some(error) => Err(ComponentPropertyAdmissionError::Admission(error)),
            None => result.map_err(ComponentPropertyAdmissionError::Property),
        }
    }

    fn replace_candidate(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        candidate: &mut ComponentValue,
        replacement: ComponentValue,
        limits: ComponentValueMaterializationLimits,
        admission: &mut Option<&mut PropertyAdmission<'_>>,
    ) -> Result<(ComponentPropertyPath, ComponentValue), ComponentPropertyError> {
        let mut edit = PropertyReplacement {
            value: (&replacement).into(),
            key_change: None,
        };
        let (resolved, storage) = resolve_location_with_admission(
            schema,
            version,
            (&*candidate).into(),
            self,
            None,
            admission.as_deref_mut(),
            Some(&mut edit),
        )?;
        admit_property_value(resolved.value, Some(limits), admission)?;
        preflight_candidate_shape(resolved.shape, self, resolved.value, admission)?;
        resolved
            .shape
            .validate_value(resolved.value)
            .map_err(|error| {
                ComponentPropertyError::InvalidValue(ComponentCodecError::invalid_field(
                    self.root.as_str(),
                    error.to_string(),
                ))
            })?;
        admit_property_value(edit.value, Some(limits), admission)?;
        preflight_candidate_shape(resolved.shape, self, edit.value, admission)?;
        validate_property_replacement_shape(resolved.shape, self, edit.value)?;
        let inverse = self.inverse_after_key_change(edit.key_change, admission)?;
        if admission.is_some() {
            admit_storage_access(storage.segments(), true, admission)?;
        }
        let previous = candidate
            .replace_path(&storage, replacement)
            .map_err(ComponentPropertyError::ValueUnavailable)?;
        Ok((inverse, previous))
    }

    fn inverse_after_key_change(
        &self,
        change: Option<ListKeyChange<'_, '_>>,
        admission: &mut Option<&mut PropertyAdmission<'_>>,
    ) -> Result<Self, ComponentPropertyError> {
        let Some(change) = change else {
            if admission.is_some() {
                self.admit_inverse(self.logical_bytes()?, admission)?;
            }
            return Ok(self.clone());
        };
        let key_bytes = change.value.as_str().map_or(8, str::len);
        let other_bytes = self.logical_bytes()? - change.previous.logical_bytes();
        let bytes = other_bytes.saturating_add(key_bytes);
        if bytes > MAX_SHAPE_BYTES {
            return Err(ComponentPropertyError::PathByteLimit);
        }
        if change.duplicate {
            admit_property_work(admission, 0, 0, change.value.as_str().map_or(0, str::len))?;
        } else {
            self.admit_inverse(bytes, admission)?;
        }
        let key = ComponentPropertyKey::from_value(change.value).ok_or_else(|| {
            ComponentPropertyError::KeyTypeMismatch {
                step: change.step,
                expected: change.previous.value_kind(),
                actual: change.value.kind(),
            }
        })?;
        if change.duplicate {
            return Err(ComponentPropertyError::AmbiguousListItem {
                step: change.step,
                key,
            });
        }
        let mut steps = Vec::with_capacity(self.steps.len());
        steps.extend_from_slice(&self.steps[..change.step]);
        steps.push(ComponentPropertyStep::ListItem {
            by: change.by.clone(),
            key,
        });
        steps.extend_from_slice(&self.steps[change.step + 1..]);
        Ok(Self {
            root: self.root.clone(),
            steps,
        })
    }

    fn admit_inverse(
        &self,
        bytes: usize,
        admission: &mut Option<&mut PropertyAdmission<'_>>,
    ) -> Result<(), ComponentPropertyError> {
        admit_property_work(
            admission,
            self.steps.len() + 1,
            bytes,
            self.steps.len() * size_of::<ComponentPropertyStep>() + bytes,
        )
    }

    /// Replaces a property in an isolated, unpublished candidate without cloning its siblings.
    ///
    /// This internal owner seam consumes the candidate on success or failure. It shares selector,
    /// replacement shape and value-budget validation with `replace_component_property`. Complete
    /// candidate validation is deferred to its owner, including default interpretation and keyed
    /// list uniqueness. The owner must admit allocations and validate the result before publishing.
    #[doc(hidden)]
    pub fn replace_in_owned_value(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        mut value: ComponentValue,
        replacement: ComponentValue,
    ) -> Result<ComponentValue, ComponentPropertyError> {
        let (resolved, storage) = resolve_location(schema, version, (&value).into(), self, None)?;
        validate_property_replacement(resolved.shape, self, &replacement, None)?;
        replace_prepared_property(&mut value, &storage, replacement, None)?;
        Ok(value)
    }

    /// Replaces a property in an isolated candidate under its owner's explicit value limits.
    ///
    /// Unlike the arbitrary-value entry point, this uses the supplied logical node, byte and
    /// depth limits for the original, replacement and complete resulting value. It retains the
    /// same selector, replacement shape and storage semantics. The candidate is consumed on
    /// failure; complete candidate semantics, defaults and keyed-list uniqueness remain the
    /// owner's responsibility before publication. No sibling or rollback copy is allocated.
    #[doc(hidden)]
    pub fn replace_in_owned_value_with_limits(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        mut value: ComponentValue,
        replacement: ComponentValue,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<ComponentValue, ComponentPropertyError> {
        let (resolved, storage) =
            resolve_location(schema, version, (&value).into(), self, Some(limits))?;
        validate_property_replacement(resolved.shape, self, &replacement, Some(limits))?;
        replace_prepared_property(&mut value, &storage, replacement, Some(limits))?;
        Ok(value)
    }
}

/// A borrowed observation tied to the exact supplied schema and value. No reusable storage
/// locator escapes resolution, and replacing the value requires resolving the selector again.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedComponentProperty<'schema, 'value> {
    pub shape: &'schema ComponentValueShape,
    pub value: ComponentValueRef<'value>,
}

/// A selector's declaration in an admitted current schema, without any value observation.
///
/// The owning field retains its original identity, metadata and capabilities. The selected
/// shape describes a replacement payload; it does not prove that a target or value exists.
#[derive(Debug, Clone, Copy)]
pub struct ComponentPropertySchemaClaim<'schema> {
    pub field: &'schema ComponentFieldSchema,
    pub shape: &'schema ComponentValueShape,
}

/// Finite selector lookup work, independent of the selected value's copying budget.
/// Counts examined declarations, optional wrappers, tombstones, list keys and indexed lookups,
/// not allocator usage, elapsed time or internal BTreeMap comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentPropertyReadBudget {
    max_entries: usize,
    max_logical_bytes: usize,
    scanned_entries: usize,
    scanned_logical_bytes: usize,
}

impl ComponentPropertyReadBudget {
    #[must_use]
    pub const fn new(entries: ItemLimit, logical_bytes: ByteLimit) -> Self {
        Self {
            max_entries: entries.get(),
            max_logical_bytes: logical_bytes.get(),
            scanned_entries: 0,
            scanned_logical_bytes: 0,
        }
    }

    #[must_use]
    pub const fn scanned_entries(self) -> usize {
        self.scanned_entries
    }

    #[must_use]
    pub const fn scanned_logical_bytes(self) -> usize {
        self.scanned_logical_bytes
    }

    /// Charges one owner-observed declaration or collection identity before inspecting it.
    ///
    /// Source owners reuse this allowance across selector resolution and child enumeration.
    /// This does not validate a declaration, copy values or authorize a source mutation.
    #[doc(hidden)]
    pub fn observe_entry(&mut self, logical_bytes: usize) -> Result<(), ComponentPropertyError> {
        self.observe(logical_bytes)
    }

    fn observe(&mut self, bytes: usize) -> Result<(), ComponentPropertyError> {
        let entries = self.scanned_entries.checked_add(1);
        let bytes = self.scanned_logical_bytes.checked_add(bytes);
        match (entries, bytes) {
            (Some(entries), Some(bytes))
                if entries <= self.max_entries && bytes <= self.max_logical_bytes =>
            {
                self.scanned_entries = entries;
                self.scanned_logical_bytes = bytes;
                Ok(())
            }
            _ => Err(ComponentPropertyError::ReadBudgetExceeded),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPropertyError {
    ReadBudgetExceeded,
    PathDepthLimit,
    PathByteLimit,
    /// Invalid caller IDs are not retained: they may be arbitrarily large unadmitted candidates.
    InvalidFieldId,
    VersionMismatch {
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    InvalidSchema(ComponentShapeError),
    ValueBudget(ComponentShapeError),
    InvalidValue(ComponentCodecError),
    UnknownRoot(ComponentFieldId),
    RetiredField(ComponentFieldId),
    RetiredMember(ComponentMemberPath),
    ValueUnavailable(ComponentFieldPathError),
    OptionalAbsent {
        step: usize,
    },
    ExpectedRecord {
        step: usize,
    },
    /// No immediate identified member matches. Anonymous ancestors cannot be skipped, and
    /// anonymous members are never inferred from a name that happens to match the requested ID.
    MemberNotFound {
        step: usize,
        id: ComponentFieldId,
    },
    ExpectedList {
        step: usize,
    },
    UnkeyedList {
        step: usize,
    },
    ListIdentityMismatch {
        step: usize,
        expected: ComponentFieldId,
        actual: ComponentFieldId,
    },
    KeyTypeMismatch {
        step: usize,
        expected: ComponentValueKind,
        actual: ComponentValueKind,
    },
    InvalidListItemKey {
        step: usize,
        index: usize,
    },
    ListItemNotFound {
        step: usize,
        key: ComponentPropertyKey,
    },
    AmbiguousListItem {
        step: usize,
        key: ComponentPropertyKey,
    },
    ExpectedMap {
        step: usize,
    },
    MapEntryNotFound {
        step: usize,
        key: String,
    },
    InvalidReplacementShape(ComponentShapeError),
    InvalidReplacement(ComponentCodecError),
    StorageIndexOverflow,
}

impl Display for ComponentPropertyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "component property resolution failed: {self:?}")
    }
}

impl Error for ComponentPropertyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSchema(error)
            | Self::ValueBudget(error)
            | Self::InvalidReplacementShape(error) => Some(error),
            Self::InvalidValue(error) | Self::InvalidReplacement(error) => Some(error),
            Self::ValueUnavailable(error) => Some(error),
            _ => None,
        }
    }
}

/// Separates property interpretation failure from the owning caller's work allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPropertyAdmissionError<E> {
    Property(ComponentPropertyError),
    Admission(E),
}

type PropertyAdmission<'a> = dyn FnMut(usize, usize, usize) -> bool + 'a;

/// Resolves against the caller's exact admitted current schema and current-version value.
///
/// The schema owner supplies admission/lineage authority; this function does not select a
/// catalog, migrate data or authorize an edit. The supplied value and selector are bounded by
/// Reflect's schema/value admission limits. Missing fields/defaults are never synthesized.
pub fn resolve_component_property<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: impl Into<ComponentValueRef<'value>>,
    path: &ComponentPropertyPath,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    let value = value.into();
    let (resolved, _) = resolve_location(schema, version, value, path, None)?;
    validate_component_value(schema, value).map_err(ComponentPropertyError::InvalidValue)?;
    Ok(resolved)
}

/// Resolves a property using the caller's explicit logical value-admission limits.
///
/// The caller supplies an admitted current schema. This checks the whole value's node, byte
/// and depth cost using `limits`, then shares the ordinary selector and semantic validation.
/// It neither selects a Schema authority nor materializes a missing property. Arbitrary-value
/// callers that need Reflect's fixed default admission should use [`resolve_component_property`].
pub fn resolve_component_property_with_limits<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: impl Into<ComponentValueRef<'value>>,
    path: &ComponentPropertyPath,
    limits: ComponentValueMaterializationLimits,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    let value = value.into();
    let (resolved, _) = resolve_location(schema, version, value, path, Some(limits))?;
    validate_component_value_with_limits(schema, value, limits)
        .map_err(ComponentPropertyError::InvalidValue)?;
    Ok(resolved)
}

/// Resolves a selector to its storage coordinate in the exact supplied schema/value snapshot.
///
/// This internal owner seam shares selector, schema/version and raw value-budget admission
/// with [`ComponentPropertyPath::replace_in_owned_value_with_limits`]. It does not materialize
/// defaults or validate complete value semantics; those remain the owner's responsibility.
/// The coordinate is transient: resolve again after the schema or value changes. It is neither
/// a durable selector/address nor authority to edit, retain or publish the supplied value.
///
/// Before copying the complete root storage coordinate, the callback admits its segment storage
/// and string bytes. Later storage-vector growth, selected wire-name copies and retired-member
/// diagnostic storage are admitted before allocation. These are modeled storage allowances, not allocator/RSS bounds.
/// Refusal returns the exact caller error without performing the denied allocation step.
#[doc(hidden)]
pub fn resolve_component_property_storage_path_with_admission<'value, E>(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: impl Into<ComponentValueRef<'value>>,
    path: &ComponentPropertyPath,
    limits: ComponentValueMaterializationLimits,
    mut admit: impl FnMut(usize) -> Result<(), E>,
) -> Result<ComponentFieldPath, ComponentPropertyAdmissionError<E>> {
    let mut refusal = None;
    let result = {
        let mut callback = |_, _, allocated| {
            if allocated == 0 {
                return true;
            }
            match admit(allocated) {
                Ok(()) => true,
                Err(error) => {
                    refusal = Some(error);
                    false
                }
            }
        };
        resolve_location_with_admission(
            schema,
            version,
            value.into(),
            path,
            Some(limits),
            Some(&mut callback),
            None,
        )
        .map(|(_, storage)| storage)
    };
    match refusal {
        Some(error) => Err(ComponentPropertyAdmissionError::Admission(error)),
        None => result.map_err(ComponentPropertyAdmissionError::Property),
    }
}

/// Resolves only a selector's schema claim, retaining its owning field and selected shape.
///
/// The caller supplies an admitted schema. Current version, durable IDs, tombstones and keyed
/// list identity policy are checked through the same walker as value resolution. Selector size,
/// selected declaration depth and declaration scans are bounded; the budget includes selector
/// bytes and examined root declarations/tombstones as well as nested lookup work.
///
/// No actual component, list item, map entry or optional value is required or inspected. This
/// does not validate a target, replacement value, default, binding, edit or native authority.
pub fn resolve_component_property_schema_claim<'schema>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyError> {
    resolve_component_property_schema_claim_impl(schema, version, path, budget, None)
}

/// Resolves a selector through the same walker with pre-scan and pre-growth work admission.
///
/// The caller supplies an admitted schema and retains the shared read budget across lookups.
/// Callback deltas count examined entries, logical identity bytes and modeled retired-member
/// diagnostic storage, excluding shared catalog storage. Successful lookup borrows selector
/// prefixes. Refusal returns the exact caller error before
/// the next declaration comparison or scratch allocation. Selector/version/depth guards and
/// interpretation errors remain the same as the ordinary schema-claim entry point.
pub fn resolve_component_property_schema_claim_with_admission<'schema, E>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyAdmissionError<E>> {
    let mut refusal = None;
    let result = {
        let mut callback = |nodes, bytes, allocated| match admit(nodes, bytes, allocated) {
            Ok(()) => true,
            Err(error) => {
                refusal = Some(error);
                false
            }
        };
        resolve_component_property_schema_claim_impl(
            schema,
            version,
            path,
            budget,
            Some(&mut callback),
        )
    };
    match refusal {
        Some(error) => Err(ComponentPropertyAdmissionError::Admission(error)),
        None => result.map_err(ComponentPropertyAdmissionError::Property),
    }
}

/// Validates a selected replacement under its owner's finite work and value allowances.
///
/// The caller supplies an admitted schema and its already resolved property claim. Only the
/// supplied replacement is validated; selector/target resolution, defaults and the shared
/// selector read budget remain with the caller. The path supplies the root identity for
/// numeric-range diagnostics.
///
/// The existing materialization walker checks raw limits, including opaque contents, in one
/// admitted pass. Declaration/value scratch and potential errors use the registry's shared
/// preflight before the shared replacement shape validator runs. Callback deltas count visited
/// work, logical bytes and modeled scratch allocation, not allocator/RSS bounds.
pub fn validate_component_property_value_with_admission<'value, E>(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    value: impl Into<ComponentValueRef<'value>>,
    limits: ComponentValueMaterializationLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<(), ComponentPropertyAdmissionError<E>> {
    let value = value.into();
    validate_value_materialization_budget_with_admission(value, limits, &mut admit).map_err(
        |error| match error {
            EffectiveValueAdmissionError::Value(error) => {
                ComponentPropertyAdmissionError::Property(ComponentPropertyError::InvalidValue(
                    error,
                ))
            }
            EffectiveValueAdmissionError::Admission(error) => {
                ComponentPropertyAdmissionError::Admission(error)
            }
        },
    )?;
    crate::registry::preflight_property_validation(shape, &path.root, value, &mut admit)
        .map_err(ComponentPropertyAdmissionError::Admission)?;
    validate_property_replacement_shape(shape, path, value)
        .map_err(ComponentPropertyAdmissionError::Property)
}

fn resolve_component_property_schema_claim_impl<'schema>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    mut admission: Option<&mut PropertyAdmission<'_>>,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyError> {
    if version != schema.version {
        return Err(ComponentPropertyError::VersionMismatch {
            expected: schema.version,
            actual: version,
        });
    }
    observe_read(
        &mut Some(&mut *budget),
        &mut admission,
        path.logical_bytes()?,
    )?;
    let field = resolve_root_field(schema, &path.root, Some(&mut *budget), &mut admission)?;
    let selected =
        resolve_root_location(field, None, path, Some(budget), None, admission, None, None)?;
    Ok(ComponentPropertySchemaClaim {
        field,
        shape: selected.shape,
    })
}

/// Borrows a descendant of an already admitted root, without validating/materializing siblings.
/// The caller owns current Schema/version admission and explicit/default source interpretation.
/// This is neither arbitrary input validation nor permission to edit the observed value.
pub fn resolve_component_property_in_root<'schema, 'value>(
    field: &'schema ComponentFieldSchema,
    value: impl Into<ComponentValueRef<'value>>,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    let selected = admitted_root_location(field, Some(value.into()), path, budget, false, None)?;
    Ok(ResolvedComponentProperty {
        shape: selected.shape,
        value: selected
            .value
            .expect("value resolution retains its observation"),
    })
}

/// Reads an already admitted root with the index from its exact original field validation.
/// The caller supplies the same explicit or defaulted field value and Schema. Index positions
/// are private to the observation. A retained read path needs the original revision fence.
#[doc(hidden)]
pub fn read_admitted_component_property_in_root<'value>(
    field: &ComponentFieldSchema,
    value: ComponentValueRef<'value>,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    capture_read_path: bool,
    index: Option<&ComponentPropertyReadIndex>,
) -> Result<(ComponentValueRef<'value>, ComponentFieldPath), ComponentPropertyError> {
    let lists = index.and_then(|index| index.lists(field.id()));
    let selected =
        admitted_root_location(field, Some(value), path, budget, capture_read_path, lists)?;
    Ok((
        selected.value.expect("the observed root has a value"),
        selected.storage.unwrap_or_default(),
    ))
}

/// Borrows a selector's declaration below an already admitted and indexed root field.
///
/// Root lookup and source presence remain the caller's responsibility. Nested declarations,
/// optional wrappers, tombstones and collection policy share the ordinary property walker.
/// No source values are inspected and no effective component is materialized.
#[doc(hidden)]
pub fn resolve_component_property_schema_in_root<'schema>(
    field: &'schema ComponentFieldSchema,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyError> {
    let selected = admitted_root_location(field, None, path, budget, false, None)?;
    Ok(ComponentPropertySchemaClaim {
        field,
        shape: selected.shape,
    })
}

fn admitted_root_location<'schema, 'value>(
    field: &'schema ComponentFieldSchema,
    value: Option<ComponentValueRef<'value>>,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    capture_read_path: bool,
    indexed_lists: Option<&KeyedListLocations>,
) -> Result<PropertyLocation<'schema, 'value>, ComponentPropertyError> {
    path.validate()?;
    if field.id != path.root {
        return Err(ComponentPropertyError::UnknownRoot(path.root.clone()));
    }
    resolve_root_location(
        field,
        value,
        path,
        Some(budget),
        capture_read_path.then(Vec::new),
        None,
        None,
        indexed_lists,
    )
}

/// Replaces exactly one existing target, returning its previous value.
///
/// All resolution, shape and complete candidate validation precede publication. Failure leaves
/// the supplied value unchanged, including when a key edit would create a duplicate elsewhere.
/// Native codecs and document owners still validate business invariants, references and revision
/// authority before their own publication. Changing an item's key changes its logical identity.
pub fn replace_component_property(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &mut ComponentValue,
    path: &ComponentPropertyPath,
    replacement: ComponentValue,
) -> Result<ComponentValue, ComponentPropertyError> {
    let current = ComponentValueRef::from(&*value);
    let (resolved, storage) = resolve_location(schema, version, current, path, None)?;
    validate_component_value(schema, current).map_err(ComponentPropertyError::InvalidValue)?;
    validate_property_replacement(resolved.shape, path, &replacement, None)?;
    let mut candidate = value.clone();
    let previous = replace_prepared_property(&mut candidate, &storage, replacement, None)?;
    validate_component_value(schema, &candidate)
        .map_err(ComponentPropertyError::InvalidReplacement)?;
    *value = candidate;
    Ok(previous)
}

fn validate_property_replacement(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    replacement: &ComponentValue,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<(), ComponentPropertyError> {
    admit_property_value(replacement.into(), limits, &mut None)?;
    validate_property_replacement_shape(shape, path, replacement.into())
}

fn validate_property_replacement_shape(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    replacement: ComponentValueRef<'_>,
) -> Result<(), ComponentPropertyError> {
    shape.validate_value(replacement).map_err(|error| {
        if error.kind == crate::ComponentShapeErrorKind::NumericInterval {
            ComponentPropertyError::InvalidReplacement(crate::registry::numeric_range_error(
                &path.root,
            ))
        } else {
            ComponentPropertyError::InvalidReplacementShape(error)
        }
    })
}

fn replace_prepared_property(
    candidate: &mut ComponentValue,
    storage: &ComponentFieldPath,
    replacement: ComponentValue,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<ComponentValue, ComponentPropertyError> {
    let previous = candidate
        .replace_path(storage, replacement)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    admit_property_value(ComponentValueRef::from(&*candidate), limits, &mut None)?;
    Ok(previous)
}

fn preflight_candidate_shape(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    value: ComponentValueRef<'_>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    if admission.is_none() {
        return Ok(());
    }
    crate::registry::preflight_property_validation(shape, &path.root, value, &mut |n, b, a| {
        admit_property_work(admission, n, b, a)
    })
}

fn admit_property_value(
    value: ComponentValueRef<'_>,
    limits: Option<ComponentValueMaterializationLimits>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    let Some(limits) = limits else {
        return validate_component_value_budget(value).map_err(ComponentPropertyError::ValueBudget);
    };
    if admission.is_some() {
        validate_value_materialization_budget_with_admission(
            value,
            limits,
            |nodes, bytes, allocated| admit_property_work(admission, nodes, bytes, allocated),
        )
        .map_err(|error| match error {
            EffectiveValueAdmissionError::Value(error) => {
                ComponentPropertyError::InvalidValue(error)
            }
            EffectiveValueAdmissionError::Admission(error) => error,
        })
    } else {
        validate_value_materialization_budget(value, limits)
            .map(|_| ())
            .map_err(ComponentPropertyError::InvalidValue)
    }
}

fn resolve_location<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: ComponentValueRef<'value>,
    path: &ComponentPropertyPath,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<
    (
        ResolvedComponentProperty<'schema, 'value>,
        ComponentFieldPath,
    ),
    ComponentPropertyError,
> {
    resolve_location_with_admission(schema, version, value, path, limits, None, None)
}

fn resolve_location_with_admission<'schema, 'value, 'path>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: ComponentValueRef<'value>,
    path: &'path ComponentPropertyPath,
    limits: Option<ComponentValueMaterializationLimits>,
    mut admission: Option<&mut PropertyAdmission<'_>>,
    edit: Option<&mut PropertyReplacement<'path, '_>>,
) -> Result<
    (
        ResolvedComponentProperty<'schema, 'value>,
        ComponentFieldPath,
    ),
    ComponentPropertyError,
> {
    if version != schema.version {
        return Err(ComponentPropertyError::VersionMismatch {
            expected: schema.version,
            actual: version,
        });
    }
    let bytes = path.logical_bytes()?;
    let meter_candidate = admission.is_some() && edit.is_some();
    if meter_candidate {
        admit_property_work(
            &mut admission,
            path.steps.len() + 1,
            bytes,
            bytes + ComponentFieldId::MAX_BYTES,
        )?;
    } else {
        validate_component_shape_budget(schema)
            .map_err(|(_, error)| ComponentPropertyError::InvalidSchema(error))?;
    }
    if edit.is_none() {
        admit_property_value(value, limits, &mut None)?;
    }
    let root = resolve_root_field(schema, &path.root, None, &mut admission)?;
    if meter_candidate {
        admit_storage_access(root.path.segments(), false, &mut admission)?;
    }
    let current = value
        .get_path(&root.path)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    if admission.is_some() {
        admit_property_work(
            &mut admission,
            0,
            0,
            storage_path_bytes(root.path.segments())?,
        )?;
    }
    let selected = resolve_root_location(
        root,
        Some(current),
        path,
        None,
        Some(root.path.segments().to_vec()),
        admission,
        edit,
        None,
    )?;
    Ok((
        ResolvedComponentProperty {
            shape: selected.shape,
            value: selected
                .value
                .expect("value resolution retains its observation"),
        },
        selected
            .storage
            .expect("complete resolution records its storage path"),
    ))
}

struct PropertyLocation<'schema, 'value> {
    shape: &'schema ComponentValueShape,
    value: Option<ComponentValueRef<'value>>,
    storage: Option<ComponentFieldPath>,
}

struct PropertyReplacement<'path, 'value> {
    value: ComponentValueRef<'value>,
    key_change: Option<ListKeyChange<'path, 'value>>,
}

struct ListKeyChange<'path, 'value> {
    step: usize,
    by: &'path ComponentFieldId,
    previous: &'path ComponentPropertyKey,
    value: ComponentValueRef<'value>,
    duplicate: bool,
}

fn resolve_root_location<'schema, 'value, 'path>(
    root: &'schema ComponentFieldSchema,
    mut current: Option<ComponentValueRef<'value>>,
    path: &'path ComponentPropertyPath,
    mut budget: Option<&mut ComponentPropertyReadBudget>,
    mut storage: Option<Vec<ComponentFieldPathSegment>>,
    mut admission: Option<&mut PropertyAdmission<'_>>,
    mut edit: Option<&mut PropertyReplacement<'path, '_>>,
    mut indexed_lists: Option<&KeyedListLocations>,
) -> Result<PropertyLocation<'schema, 'value>, ComponentPropertyError> {
    let mut shape = &root.shape;
    let mut depth = 0;
    let meter_candidate = admission.is_some() && edit.is_some();
    for (step, selector) in path.steps.iter().enumerate() {
        while let ComponentValueShape::Optional { value } = shape {
            observe_read(&mut budget, &mut admission, 0)?;
            if current.is_some_and(|value| value.kind() == ComponentValueKind::Null) {
                return Err(ComponentPropertyError::OptionalAbsent { step });
            }
            advance_declaration_depth(&mut depth)?;
            shape = value;
        }
        advance_declaration_depth(&mut depth)?;
        match selector {
            ComponentPropertyStep::Member(id) => {
                for tombstone in &root.member_tombstones {
                    if budget.is_some() || meter_candidate {
                        for segment in tombstone.segments() {
                            let bytes = match segment {
                                ComponentMemberPathSegment::Member(id) => id.as_str().len(),
                                _ => 0,
                            };
                            observe_read(&mut budget, &mut admission, bytes)?;
                        }
                    }
                    let prefix = &path.steps[..=step];
                    if tombstone.segments().len() <= prefix.len()
                        && prefix.iter().zip(tombstone.segments()).all(
                            |(selector, segment)| match (selector, segment) {
                                (
                                    ComponentPropertyStep::Member(left),
                                    ComponentMemberPathSegment::Member(right),
                                ) => left == right,
                                (
                                    ComponentPropertyStep::ListItem { .. },
                                    ComponentMemberPathSegment::ListItem,
                                )
                                | (
                                    ComponentPropertyStep::MapEntry(_),
                                    ComponentMemberPathSegment::MapEntry,
                                ) => true,
                                _ => false,
                            },
                        )
                    {
                        let mut declaration = Vec::new();
                        for selector in prefix {
                            reserve_declaration(&mut declaration, &mut admission)?;
                            declaration.push(match selector {
                                ComponentPropertyStep::Member(id) => {
                                    admit_property_work(&mut admission, 0, 0, id.as_str().len())?;
                                    ComponentMemberPathSegment::Member(id.clone())
                                }
                                ComponentPropertyStep::ListItem { .. } => {
                                    ComponentMemberPathSegment::ListItem
                                }
                                ComponentPropertyStep::MapEntry(_) => {
                                    ComponentMemberPathSegment::MapEntry
                                }
                            });
                        }
                        return Err(ComponentPropertyError::RetiredMember(
                            ComponentMemberPath::new(declaration),
                        ));
                    }
                }
                let ComponentValueShape::Record { members } = shape else {
                    return Err(ComponentPropertyError::ExpectedRecord { step });
                };
                let mut selected = None;
                for member in members {
                    observe_read(
                        &mut budget,
                        &mut admission,
                        member
                            .name
                            .len()
                            .saturating_add(member.id.as_ref().map_or(0, |id| id.as_str().len())),
                    )?;
                    if member.id.as_ref() == Some(id) {
                        selected = Some(member);
                        break;
                    }
                }
                let member = selected.ok_or_else(|| ComponentPropertyError::MemberNotFound {
                    step,
                    id: id.clone(),
                })?;
                if let Some(value) = current {
                    let Some(value) = value.get(&member.name) else {
                        if meter_candidate {
                            admit_property_work(&mut admission, 0, 0, member.name.len())?;
                        }
                        return Err(ComponentPropertyError::InvalidValue(
                            ComponentCodecError::missing_field(&member.name),
                        ));
                    };
                    current = Some(value);
                }
                if let Some(storage) = &mut storage {
                    reserve_storage(storage, &mut admission)?;
                    admit_property_work(&mut admission, 0, 0, member.name.len())?;
                    storage.push(ComponentFieldPathSegment::field(&member.name));
                }
                indexed_lists = indexed_lists.and_then(|lists| lists.field(&member.name));
                shape = &member.shape;
            }
            ComponentPropertyStep::ListItem { by, key } => {
                if matches!(shape, ComponentValueShape::List { .. }) {
                    return Err(ComponentPropertyError::UnkeyedList { step });
                }
                let ComponentValueShape::KeyedList {
                    item,
                    key: identity,
                } = shape
                else {
                    return Err(ComponentPropertyError::ExpectedList { step });
                };
                if by != identity {
                    return Err(ComponentPropertyError::ListIdentityMismatch {
                        step,
                        expected: identity.clone(),
                        actual: by.clone(),
                    });
                }
                // Admit the helper's finite declaration scan before it visits the members.
                if (budget.is_some() || meter_candidate)
                    && let ComponentValueShape::Record { members } = item.as_ref()
                {
                    for member in members {
                        observe_read(
                            &mut budget,
                            &mut admission,
                            member.name.len().saturating_add(
                                member.id.as_ref().map_or(0, |id| id.as_str().len()),
                            ),
                        )?;
                    }
                }
                let member = keyed_list_member(item, identity).map_err(|kind| {
                    ComponentPropertyError::InvalidSchema(ComponentShapeError {
                        path: ComponentFieldPath::new(
                            storage.as_deref().unwrap_or_default().iter().cloned(),
                        ),
                        kind,
                    })
                })?;
                let expected = member.shape.value_kind();
                if key.value_kind() != expected {
                    return Err(ComponentPropertyError::KeyTypeMismatch {
                        step,
                        expected,
                        actual: key.value_kind(),
                    });
                }
                shape = item;
                let Some(value) = current else {
                    continue;
                };
                let Some(items) = value.as_list() else {
                    if meter_candidate {
                        admit_property_work(
                            &mut admission,
                            0,
                            0,
                            "keyed list".len() + "list".len(),
                        )?;
                    }
                    return Err(ComponentPropertyError::InvalidValue(
                        ComponentCodecError::invalid_field("keyed list", "list"),
                    ));
                };
                let indexed = indexed_lists.and_then(KeyedListLocations::rows);
                let (index, value) = if let Some(rows) = indexed {
                    observe_read(
                        &mut budget,
                        &mut admission,
                        member.name.len().saturating_add(key.logical_bytes()),
                    )?;
                    let index =
                        *rows
                            .get(key)
                            .ok_or_else(|| ComponentPropertyError::ListItemNotFound {
                                step,
                                key: key.clone(),
                            })?;
                    let value = items
                        .get(index)
                        .filter(|value| {
                            value
                                .get(&member.name)
                                .is_some_and(|identity| key.matches(identity))
                        })
                        .ok_or(ComponentPropertyError::InvalidListItemKey { step, index })?;
                    (index, value)
                } else {
                    let new_key = match (&edit, &path.steps[step + 1..]) {
                        (Some(edit), []) => {
                            admit_property_work(&mut admission, 1, member.name.len(), 0)?;
                            edit.value.get(&member.name)
                        }
                        (Some(edit), [ComponentPropertyStep::Member(id)]) if id == by => {
                            Some(edit.value)
                        }
                        _ => None,
                    };
                    let mut key_change = None;
                    if let Some(replacement) = new_key.filter(|value| value.kind() == expected) {
                        admit_property_work(&mut admission, 1, key.logical_bytes(), 0)?;
                        if !key.matches(replacement) {
                            key_change = Some(ListKeyChange {
                                step,
                                by,
                                previous: key,
                                value: replacement,
                                duplicate: false,
                            });
                        }
                    }
                    let mut found = None;
                    for (index, value) in items.iter().enumerate() {
                        observe_read(&mut budget, &mut admission, member.name.len())?;
                        let identity = value
                            .get(&member.name)
                            .filter(|value| value.kind() == expected)
                            .ok_or(ComponentPropertyError::InvalidListItemKey { step, index })?;
                        let bytes = identity.as_str().map_or(8, str::len);
                        observe_read(&mut budget, &mut admission, bytes)?;
                        if let Some(change) = &mut key_change
                            && !change.duplicate
                        {
                            admit_property_work(&mut admission, 1, bytes, 0)?;
                            change.duplicate = change.value == identity;
                        }
                        if key.matches(identity) && found.replace((index, value)).is_some() {
                            return Err(ComponentPropertyError::AmbiguousListItem {
                                step,
                                key: key.clone(),
                            });
                        }
                    }
                    let selected =
                        found.ok_or_else(|| ComponentPropertyError::ListItemNotFound {
                            step,
                            key: key.clone(),
                        })?;
                    if let (Some(edit), Some(change)) = (&mut edit, key_change) {
                        edit.key_change = Some(change);
                    }
                    selected
                };
                current = Some(value);
                indexed_lists = indexed_lists.and_then(|lists| lists.item(index));
                if let Some(storage) = &mut storage {
                    let index = u32::try_from(index)
                        .map_err(|_| ComponentPropertyError::StorageIndexOverflow)?;
                    reserve_storage(storage, &mut admission)?;
                    storage.push(ComponentFieldPathSegment::index(index));
                }
            }
            ComponentPropertyStep::MapEntry(key) => {
                let ComponentValueShape::Map { value: entry } = shape else {
                    return Err(ComponentPropertyError::ExpectedMap { step });
                };
                observe_read(&mut budget, &mut admission, key.len())?;
                current = current
                    .map(|value| {
                        value
                            .get(key)
                            .ok_or_else(|| ComponentPropertyError::MapEntryNotFound {
                                step,
                                key: key.clone(),
                            })
                    })
                    .transpose()?;
                if let Some(storage) = &mut storage {
                    reserve_storage(storage, &mut admission)?;
                    admit_property_work(&mut admission, 0, 0, key.len())?;
                    storage.push(ComponentFieldPathSegment::field(key));
                }
                indexed_lists = indexed_lists.and_then(|lists| lists.field(key));
                shape = entry;
            }
        }
    }
    Ok(PropertyLocation {
        shape,
        value: current,
        storage: storage.map(ComponentFieldPath::new),
    })
}

fn advance_declaration_depth(depth: &mut usize) -> Result<(), ComponentPropertyError> {
    *depth += 1;
    if *depth > MAX_SHAPE_DEPTH {
        return Err(ComponentPropertyError::PathDepthLimit);
    }
    Ok(())
}

fn observe_read(
    budget: &mut Option<&mut ComponentPropertyReadBudget>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
    bytes: usize,
) -> Result<(), ComponentPropertyError> {
    if let Some(admit) = admission.as_deref_mut() {
        // Validate the shared read ceiling without consuming an observation denied by the
        // caller. The ordinary path below retains its existing single budget update.
        let next = budget
            .as_deref()
            .map(|budget| {
                let mut next = *budget;
                next.observe(bytes)?;
                Ok::<_, ComponentPropertyError>(next)
            })
            .transpose()?;
        if !admit(1, bytes, 0) {
            return Err(ComponentPropertyError::ReadBudgetExceeded);
        }
        if let (Some(budget), Some(next)) = (budget.as_deref_mut(), next) {
            *budget = next;
        }
        Ok(())
    } else {
        match budget {
            Some(budget) => budget.observe(bytes),
            None => Ok(()),
        }
    }
}

fn admit_property_work(
    admission: &mut Option<&mut PropertyAdmission<'_>>,
    nodes: usize,
    bytes: usize,
    allocated: usize,
) -> Result<(), ComponentPropertyError> {
    if let Some(admit) = admission.as_deref_mut()
        && !admit(nodes, bytes, allocated)
    {
        // The public admission entry point retains the exact caller error outside this
        // internal carrier. No diagnostic or cloned selector is needed while unwinding.
        return Err(ComponentPropertyError::ReadBudgetExceeded);
    }
    Ok(())
}

fn storage_path_bytes(
    segments: &[ComponentFieldPathSegment],
) -> Result<usize, ComponentPropertyError> {
    let bytes = segments
        .len()
        .checked_mul(size_of::<ComponentFieldPathSegment>())
        .ok_or(ComponentPropertyError::ReadBudgetExceeded)?;
    segments.iter().try_fold(bytes, |bytes, segment| {
        bytes
            .checked_add(match segment {
                ComponentFieldPathSegment::Field(name) => name.len(),
                ComponentFieldPathSegment::Index(_) => 0,
            })
            .ok_or(ComponentPropertyError::ReadBudgetExceeded)
    })
}

fn admit_storage_access(
    segments: &[ComponentFieldPathSegment],
    replacing: bool,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    let stored_bytes = storage_path_bytes(segments)?;
    let mut allocated = stored_bytes.saturating_mul(6);
    if replacing {
        let mut prefix_bytes = 0_usize;
        for segment in segments {
            prefix_bytes = prefix_bytes
                .saturating_add(size_of::<ComponentFieldPathSegment>())
                .saturating_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => 0,
                });
            allocated = allocated.saturating_add(prefix_bytes);
        }
    }
    admit_property_work(admission, segments.len(), stored_bytes, allocated)
}

fn reserve_storage(
    storage: &mut Vec<ComponentFieldPathSegment>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    if admission.is_none() || storage.len() < storage.capacity() {
        return Ok(());
    }
    let capacity = storage
        .len()
        .checked_add(1)
        .and_then(|minimum| {
            storage
                .capacity()
                .checked_mul(2)
                .map(|doubled| minimum.max(doubled).max(4))
        })
        .ok_or(ComponentPropertyError::ReadBudgetExceeded)?;
    let allocated = capacity
        .checked_mul(size_of::<ComponentFieldPathSegment>())
        .ok_or(ComponentPropertyError::ReadBudgetExceeded)?;
    admit_property_work(admission, 0, 0, allocated)?;
    storage.reserve_exact(capacity - storage.len());
    Ok(())
}

fn reserve_declaration(
    declaration: &mut Vec<ComponentMemberPathSegment>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    if admission.is_none() || declaration.len() < declaration.capacity() {
        return Ok(());
    }
    let capacity = declaration
        .len()
        .saturating_add(1)
        .max(declaration.capacity().saturating_mul(2))
        .max(4);
    admit_property_work(
        admission,
        0,
        0,
        capacity.saturating_mul(size_of::<ComponentMemberPathSegment>()),
    )?;
    declaration.reserve_exact(capacity - declaration.len());
    Ok(())
}

fn resolve_root_field<'schema>(
    schema: &'schema ComponentSchema,
    id: &ComponentFieldId,
    mut budget: Option<&mut ComponentPropertyReadBudget>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<&'schema ComponentFieldSchema, ComponentPropertyError> {
    for tombstone in &schema.field_tombstones {
        observe_read(&mut budget, admission, tombstone.as_str().len())?;
        if tombstone == id {
            return Err(ComponentPropertyError::RetiredField(id.clone()));
        }
    }
    for field in &schema.fields {
        observe_read(&mut budget, admission, field.id.as_str().len())?;
        if &field.id == id {
            return Ok(field);
        }
    }
    Err(ComponentPropertyError::UnknownRoot(id.clone()))
}

#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod storage_path_tests;
#[cfg(test)]
mod value_admission_tests;
