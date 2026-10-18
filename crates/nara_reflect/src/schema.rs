//! Runtime-independent component schema identities and catalog data.

use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_identity::EntityReference;

use crate::{ComponentFieldPath, ComponentFieldPathSegment, ComponentValue};

const MAX_ALIAS_BYTES: usize = 128;

macro_rules! stable_schema_id {
    ($name:ident, $error_name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub const MAX_BYTES: usize = 128;

            /// Creates a registry candidate ID. Freeze validates the candidate.
            #[must_use]
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub fn try_new(id: impl Into<String>) -> Result<Self, $error_name> {
                let id = id.into();
                validate_schema_id(&id, Self::MAX_BYTES).map_err($error_name)?;
                Ok(Self(id))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn validate(&self) -> Result<(), $error_name> {
                validate_schema_id(self.as_str(), Self::MAX_BYTES).map_err($error_name)
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $error_name(SchemaIdErrorKind);

        impl Display for $error_name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                write!(formatter, concat!($label, " {}"), self.0)
            }
        }

        impl Error for $error_name {}

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let id = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::try_new(id).map_err(serde::de::Error::custom)
            }
        }
    };
}

stable_schema_id!(ComponentTypeId, ComponentTypeIdError, "component type ID");
stable_schema_id!(
    ComponentFieldId,
    ComponentFieldIdError,
    "component field ID"
);

#[derive(Debug, Clone, PartialEq, Eq)]
enum SchemaIdErrorKind {
    Empty,
    TooLong { length: usize, maximum: usize },
    InvalidStart(char),
    InvalidCharacter { index: usize, character: char },
}

impl Display for SchemaIdErrorKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("is empty"),
            Self::TooLong { length, maximum } => {
                write!(formatter, "has {length} bytes, maximum is {maximum}")
            }
            Self::InvalidStart(character) => write!(
                formatter,
                "must start with an ASCII letter or digit, found '{character}'"
            ),
            Self::InvalidCharacter { index, character } => write!(
                formatter,
                "contains invalid character '{character}' at byte {index}"
            ),
        }
    }
}

fn validate_schema_id(value: &str, maximum: usize) -> Result<(), SchemaIdErrorKind> {
    let Some(first) = value.chars().next() else {
        return Err(SchemaIdErrorKind::Empty);
    };
    if value.len() > maximum {
        return Err(SchemaIdErrorKind::TooLong {
            length: value.len(),
            maximum,
        });
    }
    if !first.is_ascii_alphanumeric() {
        return Err(SchemaIdErrorKind::InvalidStart(first));
    }
    if let Some((index, character)) = value.char_indices().find(|(_, character)| {
        !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
    }) {
        return Err(SchemaIdErrorKind::InvalidCharacter { index, character });
    }
    Ok(())
}

pub(crate) fn validate_alias(alias: &str) -> Result<(), AliasError> {
    if alias.is_empty() {
        return Err(AliasError::Empty);
    }
    if alias.len() > MAX_ALIAS_BYTES {
        return Err(AliasError::TooLong {
            length: alias.len(),
            maximum: MAX_ALIAS_BYTES,
        });
    }
    if let Some((index, character)) = alias
        .char_indices()
        .find(|(_, character)| character.is_control())
    {
        return Err(AliasError::ControlCharacter { index, character });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasError {
    Empty,
    TooLong { length: usize, maximum: usize },
    ControlCharacter { index: usize, character: char },
}

impl Display for AliasError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("alias is empty"),
            Self::TooLong { length, maximum } => {
                write!(formatter, "alias has {length} bytes, maximum is {maximum}")
            }
            Self::ControlCharacter { index, character } => write!(
                formatter,
                "alias contains control character '{character}' at byte {index}"
            ),
        }
    }
}

impl Error for AliasError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaVersion(pub u32);

impl ComponentSchemaVersion {
    pub const ONE: Self = Self(1);

    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for ComponentSchemaVersion {
    fn default() -> Self {
        Self::ONE
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.get())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <u32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value)
            .ok_or_else(|| serde::de::Error::custom("component schema version must be non-zero"))
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentSchema {
    pub id: ComponentTypeId,
    pub aliases: Vec<String>,
    pub version: ComponentSchemaVersion,
    #[cfg_attr(feature = "serde", serde(default))]
    pub capabilities: BTreeSet<ComponentCapability>,
    pub fields: Vec<ComponentFieldSchema>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub field_tombstones: Vec<ComponentFieldId>,
    /// Complete data copied by an explicit authoring creation transaction.
    /// This does not supply missing fields when decoding stored components.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub authoring_initial_value: Option<ComponentValue>,
}

impl ComponentSchema {
    #[must_use]
    pub fn new(
        id: ComponentTypeId,
        alias: impl Into<String>,
        version: ComponentSchemaVersion,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            version,
            capabilities: BTreeSet::new(),
            fields: Vec::new(),
            field_tombstones: Vec::new(),
            authoring_initial_value: None,
        }
    }

    #[must_use]
    pub fn with_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = ComponentCapability>,
    ) -> Self {
        self.capabilities.extend(capabilities);
        self
    }

    #[must_use]
    pub fn with_fields(mut self, fields: impl IntoIterator<Item = ComponentFieldSchema>) -> Self {
        self.fields.extend(fields);
        self
    }

    #[must_use]
    pub fn with_field_tombstones(
        mut self,
        tombstones: impl IntoIterator<Item = ComponentFieldId>,
    ) -> Self {
        self.field_tombstones.extend(tombstones);
        self
    }

    /// Declares a complete creation template, validated when the catalog is admitted.
    #[must_use]
    pub fn with_authoring_initial_value(mut self, value: ComponentValue) -> Self {
        self.authoring_initial_value = Some(value);
        self
    }

    #[must_use]
    pub const fn authoring_initial_value(&self) -> Option<&ComponentValue> {
        self.authoring_initial_value.as_ref()
    }

    #[must_use]
    pub const fn id(&self) -> &ComponentTypeId {
        &self.id
    }

    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    #[must_use]
    pub const fn version(&self) -> ComponentSchemaVersion {
        self.version
    }

    #[must_use]
    pub fn fields(&self) -> &[ComponentFieldSchema] {
        &self.fields
    }

    #[must_use]
    pub fn field_tombstones(&self) -> &[ComponentFieldId] {
        &self.field_tombstones
    }

    #[must_use]
    pub fn has_capability(&self, capability: ComponentCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentSchemaCatalog {
    pub generation: u64,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub predecessor: Option<CatalogFingerprint>,
    pub components: Vec<ComponentSchema>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub type_tombstones: Vec<ComponentTypeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentCatalogGenerationError {
    generation: u64,
}

impl ComponentCatalogGenerationError {
    #[must_use]
    pub const fn new(generation: u64) -> Self {
        Self { generation }
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

impl Display for ComponentCatalogGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "component catalog generation {} has no successor",
            self.generation
        )
    }
}

impl Error for ComponentCatalogGenerationError {}

impl Default for ComponentSchemaCatalog {
    fn default() -> Self {
        Self {
            generation: 1,
            predecessor: None,
            components: Vec::new(),
            type_tombstones: Vec::new(),
        }
    }
}

impl ComponentSchemaCatalog {
    /// Starts the immediate successor of one schema owner's catalog.
    ///
    /// This helper carries owner-local generation, predecessor, and tombstone lineage only. A
    /// flattened Runtime composition has no owner-attribution proof and must not be supplied as
    /// the predecessor of a later product recipe.
    pub fn successor_of(predecessor: &Self) -> Result<Self, ComponentCatalogGenerationError> {
        let generation = predecessor
            .generation
            .checked_add(1)
            .ok_or_else(|| ComponentCatalogGenerationError::new(predecessor.generation))?;
        Ok(Self {
            generation,
            predecessor: Some(predecessor.fingerprint()),
            components: Vec::new(),
            type_tombstones: predecessor.type_tombstones.clone(),
        })
    }

    pub fn canonicalize(&mut self) {
        self.components
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.type_tombstones.sort();
        for schema in &mut self.components {
            schema.aliases.sort();
            schema.fields.sort_by(|left, right| left.id.cmp(&right.id));
            schema.field_tombstones.sort();
            for field in &mut schema.fields {
                field.aliases.sort();
                if field.shape.validate().is_ok() {
                    field.shape.canonicalize();
                }
            }
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn predecessor(&self) -> Option<&CatalogFingerprint> {
        self.predecessor.as_ref()
    }

    #[must_use]
    pub fn components(&self) -> &[ComponentSchema] {
        &self.components
    }

    #[must_use]
    pub fn type_tombstones(&self) -> &[ComponentTypeId] {
        &self.type_tombstones
    }

    #[must_use]
    pub fn fingerprint(&self) -> CatalogFingerprint {
        let mut hasher = blake3::Hasher::new();
        // Generation and predecessor are deliberately excluded: this is a canonical content
        // fingerprint, and including predecessor would make the lineage identity recursive.
        feed_bytes(&mut hasher, b"nara.component-schema-catalog-fingerprint.v2");
        let mut components = self.components.iter().collect::<Vec<_>>();
        components.sort_by(|left, right| left.id.cmp(&right.id));
        feed_len(&mut hasher, components.len());
        for schema in components {
            if let Err((_, error)) = validate_component_shape_budget(schema) {
                feed_bytes(&mut hasher, b"invalid-unadmitted-schema-shape");
                feed_str(&mut hasher, schema.id.as_str());
                feed_str(&mut hasher, &error.to_string());
                continue;
            }
            if schema
                .fields
                .iter()
                .filter_map(|field| field.default_value.as_ref())
                .chain(schema.authoring_initial_value.iter())
                .any(|value| validate_component_value_budget(value).is_err())
            {
                feed_bytes(&mut hasher, b"invalid-unadmitted-schema-value");
                feed_str(&mut hasher, schema.id.as_str());
                continue;
            }
            feed_schema(&mut hasher, schema);
        }
        let mut tombstones = self.type_tombstones.iter().collect::<Vec<_>>();
        tombstones.sort();
        feed_len(&mut hasher, tombstones.len());
        for tombstone in tombstones {
            feed_bytes(&mut hasher, b"type-tombstone");
            feed_str(&mut hasher, tombstone.as_str());
        }
        CatalogFingerprint(*hasher.finalize().as_bytes())
    }
}

pub(crate) const MAX_SHAPE_DEPTH: usize = 64;
const MAX_SHAPE_NODES: usize = 16_384;
const MAX_SHAPE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ComponentValueShape {
    Scalar {
        kind: ComponentValueKind,
        #[cfg_attr(
            feature = "serde",
            serde(default, skip_serializing_if = "Option::is_none")
        )]
        numeric_domain: Option<ComponentNumericDomain>,
    },
    EntityRef,
    AssetRef,
    Record {
        members: Vec<ComponentShapeMember>,
    },
    List {
        item: Box<Self>,
    },
    Map {
        value: Box<Self>,
    },
    Optional {
        value: Box<Self>,
    },
    Opaque {
        kind: ComponentValueKind,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentShapeMember {
    pub name: String,
    pub shape: ComponentValueShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentShapeError {
    pub path: ComponentFieldPath,
    pub kind: ComponentShapeErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentShapeErrorKind {
    InvalidScalarKind(ComponentValueKind),
    InvalidOpaqueKind(ComponentValueKind),
    InvalidNumericDomain {
        kind: ComponentValueKind,
        domain: ComponentNumericDomain,
    },
    InvalidRecordMember,
    DuplicateRecordMember(String),
    InvalidOptional,
    DepthLimit,
    NodeLimit,
    ByteLimit,
    ValueKind {
        expected: ComponentValueKind,
        actual: ComponentValueKind,
    },
    NumericDomain(ComponentNumericDomain),
    MissingMember(String),
    UndeclaredMember(String),
}

impl Display for ComponentShapeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "component shape at '{}': {:?}",
            self.path, self.kind
        )
    }
}

impl Error for ComponentShapeError {}

#[derive(Default)]
struct ShapeBudget {
    nodes: usize,
    bytes: usize,
}

impl ShapeBudget {
    fn observe(
        &mut self,
        depth: usize,
        bytes: usize,
        path: &[ComponentFieldPathSegment],
    ) -> Result<(), ComponentShapeError> {
        if depth > MAX_SHAPE_DEPTH {
            return Err(shape_error(path, ComponentShapeErrorKind::DepthLimit));
        }
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MAX_SHAPE_NODES {
            return Err(shape_error(path, ComponentShapeErrorKind::NodeLimit));
        }
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_SHAPE_BYTES {
            return Err(shape_error(path, ComponentShapeErrorKind::ByteLimit));
        }
        Ok(())
    }
}

fn shape_error(
    path: &[ComponentFieldPathSegment],
    kind: ComponentShapeErrorKind,
) -> ComponentShapeError {
    ComponentShapeError {
        path: ComponentFieldPath::new(path.iter().cloned()),
        kind,
    }
}

impl From<ComponentValueKind> for ComponentValueShape {
    fn from(kind: ComponentValueKind) -> Self {
        match kind {
            ComponentValueKind::AssetRef => Self::AssetRef,
            ComponentValueKind::EntityRef => Self::EntityRef,
            ComponentValueKind::List | ComponentValueKind::Map => Self::Opaque { kind },
            _ => Self::scalar(kind),
        }
    }
}

impl ComponentValueShape {
    #[must_use]
    pub const fn scalar(kind: ComponentValueKind) -> Self {
        Self::Scalar {
            kind,
            numeric_domain: None,
        }
    }

    #[must_use]
    pub fn list(item: Self) -> Self {
        Self::List {
            item: Box::new(item),
        }
    }

    #[must_use]
    pub fn map(value: Self) -> Self {
        Self::Map {
            value: Box::new(value),
        }
    }

    #[must_use]
    pub fn optional(value: Self) -> Self {
        match value {
            Self::Optional { .. }
            | Self::Scalar {
                kind: ComponentValueKind::Null,
                ..
            } => value,
            value => Self::Optional {
                value: Box::new(value),
            },
        }
    }

    #[must_use]
    pub fn record(members: impl IntoIterator<Item = (String, Self)>) -> Self {
        Self::Record {
            members: members
                .into_iter()
                .map(|(name, shape)| ComponentShapeMember { name, shape })
                .collect(),
        }
    }

    #[must_use]
    pub const fn opaque(kind: ComponentValueKind) -> Self {
        Self::Opaque { kind }
    }

    #[must_use]
    pub fn with_numeric_domain(
        mut self,
        domain: impl Into<Option<ComponentNumericDomain>>,
    ) -> Self {
        let domain = domain.into();
        let mut current = &mut self;
        for _depth in 0..=MAX_SHAPE_DEPTH {
            match current {
                Self::Optional { value } => current = value,
                Self::Scalar { numeric_domain, .. } => {
                    *numeric_domain = domain;
                    return self;
                }
                shape => {
                    if domain.is_some() && shape.validate().is_ok() {
                        *shape = Self::Scalar {
                            kind: shape.value_kind(),
                            numeric_domain: domain,
                        };
                    }
                    return self;
                }
            }
        }
        self
    }

    #[must_use]
    pub fn value_kind(&self) -> ComponentValueKind {
        let mut current = self;
        for _depth in 0..=MAX_SHAPE_DEPTH {
            match current {
                Self::Scalar { kind, .. } | Self::Opaque { kind } => return *kind,
                Self::EntityRef => return ComponentValueKind::EntityRef,
                Self::AssetRef => return ComponentValueKind::AssetRef,
                Self::Record { .. } | Self::Map { .. } => return ComponentValueKind::Map,
                Self::List { .. } => return ComponentValueKind::List,
                Self::Optional { value } => current = value,
            }
        }
        ComponentValueKind::Null
    }

    #[must_use]
    pub fn numeric_domain(&self) -> Option<ComponentNumericDomain> {
        let mut current = self;
        for _depth in 0..=MAX_SHAPE_DEPTH {
            match current {
                Self::Scalar { numeric_domain, .. } => return *numeric_domain,
                Self::Optional { value } => current = value,
                _ => return None,
            }
        }
        None
    }

    #[must_use]
    pub fn contains_kind(&self, kind: ComponentValueKind) -> bool {
        if self.validate().is_err() {
            return false;
        }
        let mut pending = vec![self];
        while let Some(shape) = pending.pop() {
            match shape {
                Self::Record { members } => {
                    pending.extend(members.iter().map(|member| &member.shape))
                }
                Self::List { item } => pending.push(item),
                Self::Map { value } | Self::Optional { value } => pending.push(value),
                Self::Opaque { .. } => {}
                shape if shape.value_kind() == kind => return true,
                _ => {}
            }
        }
        false
    }

    pub fn validate(&self) -> Result<(), ComponentShapeError> {
        self.validate_shape_node(0, &mut ShapeBudget::default(), &mut Vec::new())
    }

    fn validate_shape_node(
        &self,
        depth: usize,
        budget: &mut ShapeBudget,
        path: &mut Vec<ComponentFieldPathSegment>,
    ) -> Result<(), ComponentShapeError> {
        budget.observe(depth, 1, path)?;
        match self {
            Self::Scalar {
                kind,
                numeric_domain,
            } => {
                if let Some(domain) = numeric_domain
                    && domain.value_kind() != *kind
                {
                    return Err(shape_error(
                        path,
                        ComponentShapeErrorKind::InvalidNumericDomain {
                            kind: *kind,
                            domain: *domain,
                        },
                    ));
                }
                if matches!(
                    kind,
                    ComponentValueKind::List
                        | ComponentValueKind::Map
                        | ComponentValueKind::AssetRef
                        | ComponentValueKind::EntityRef
                ) {
                    return Err(shape_error(
                        path,
                        ComponentShapeErrorKind::InvalidScalarKind(*kind),
                    ));
                }
            }
            Self::Opaque { kind }
                if !matches!(kind, ComponentValueKind::List | ComponentValueKind::Map) =>
            {
                return Err(shape_error(
                    path,
                    ComponentShapeErrorKind::InvalidOpaqueKind(*kind),
                ));
            }
            Self::Record { members } => {
                let mut names = BTreeSet::new();
                for member in members {
                    if member.name.is_empty()
                        || member.name.len() > ComponentFieldId::MAX_BYTES
                        || member.name.chars().any(char::is_control)
                    {
                        return Err(shape_error(
                            path,
                            ComponentShapeErrorKind::InvalidRecordMember,
                        ));
                    }
                    if !names.insert(&member.name) {
                        return Err(shape_error(
                            path,
                            ComponentShapeErrorKind::DuplicateRecordMember(member.name.clone()),
                        ));
                    }
                    budget.bytes = budget.bytes.saturating_add(member.name.len());
                    path.push(ComponentFieldPathSegment::field(&member.name));
                    member.shape.validate_shape_node(depth + 1, budget, path)?;
                    path.pop();
                }
            }
            Self::List { item } => item.validate_shape_node(depth + 1, budget, path)?,
            Self::Map { value } => value.validate_shape_node(depth + 1, budget, path)?,
            Self::Optional { value } => {
                if matches!(
                    value.as_ref(),
                    Self::Optional { .. }
                        | Self::Scalar {
                            kind: ComponentValueKind::Null,
                            ..
                        }
                ) {
                    return Err(shape_error(path, ComponentShapeErrorKind::InvalidOptional));
                }
                value.validate_shape_node(depth + 1, budget, path)?;
            }
            _ => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn accepts_value(&self, value: &ComponentValue) -> bool {
        self.validate_value(value).is_ok()
    }

    /// Checks declared structure without imposing a payload allocation or traversal budget.
    ///
    /// Callers own admission of the value's size and work. Opaque containers validate their outer
    /// kind only; reference traversal and persistent file owners enforce their own payload limits.
    pub fn validate_value(&self, value: &ComponentValue) -> Result<(), ComponentShapeError> {
        self.validate()?;
        self.validate_value_node(value, &mut Vec::new())
    }

    fn validate_value_node(
        &self,
        value: &ComponentValue,
        path: &mut Vec<ComponentFieldPathSegment>,
    ) -> Result<(), ComponentShapeError> {
        let expected = self.value_kind();
        match (self, value) {
            (Self::Optional { .. }, ComponentValue::Null) => Ok(()),
            (Self::Optional { value: shape }, value) => shape.validate_value_node(value, path),
            (
                Self::Scalar {
                    kind,
                    numeric_domain,
                },
                value,
            ) if value.kind() == *kind => {
                if let Some(domain) = numeric_domain
                    && !domain.accepts_value(value)
                {
                    return Err(shape_error(
                        path,
                        ComponentShapeErrorKind::NumericDomain(*domain),
                    ));
                }
                Ok(())
            }
            (Self::EntityRef, ComponentValue::EntityReference(_)) => Ok(()),
            (Self::AssetRef, value) if crate::asset_reference::is_asset_reference_value(value) => {
                Ok(())
            }
            (Self::Opaque { kind }, value) if value.kind() == *kind => Ok(()),
            (Self::Record { members }, ComponentValue::Map(values)) => {
                let declared = members
                    .iter()
                    .map(|member| member.name.as_str())
                    .collect::<BTreeSet<_>>();
                for member in members {
                    let value = values.get(&member.name).ok_or_else(|| {
                        shape_error(
                            path,
                            ComponentShapeErrorKind::MissingMember(member.name.clone()),
                        )
                    })?;
                    path.push(ComponentFieldPathSegment::field(&member.name));
                    member.shape.validate_value_node(value, path)?;
                    path.pop();
                }
                for name in values.keys() {
                    if !declared.contains(name.as_str()) {
                        return Err(shape_error(
                            path,
                            ComponentShapeErrorKind::UndeclaredMember(name.clone()),
                        ));
                    }
                }
                Ok(())
            }
            (Self::List { item }, ComponentValue::List(values)) => {
                for (index, value) in values.iter().enumerate() {
                    path.push(ComponentFieldPathSegment::index(index as u32));
                    item.validate_value_node(value, path)?;
                    path.pop();
                }
                Ok(())
            }
            (Self::Map { value: shape }, ComponentValue::Map(values)) => {
                for (name, value) in values {
                    path.push(ComponentFieldPathSegment::field(name));
                    shape.validate_value_node(value, path)?;
                    path.pop();
                }
                Ok(())
            }
            _ => Err(shape_error(
                path,
                ComponentShapeErrorKind::ValueKind {
                    expected,
                    actual: value.kind(),
                },
            )),
        }
    }

    pub(crate) fn canonicalize(&mut self) {
        match self {
            Self::Record { members } => {
                members.sort_by(|left, right| left.name.cmp(&right.name));
                for member in members {
                    member.shape.canonicalize();
                }
            }
            Self::List { item } => item.canonicalize(),
            Self::Map { value } | Self::Optional { value } => value.canonicalize(),
            _ => {}
        }
    }
}

fn validate_value_budget(
    value: &ComponentValue,
    depth: usize,
    budget: &mut ShapeBudget,
    path: &mut Vec<ComponentFieldPathSegment>,
) -> Result<(), ComponentShapeError> {
    let bytes = match value {
        ComponentValue::String(value) => value.len(),
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) => {
            entity.as_str().len()
        }
        ComponentValue::EntityReference(EntityReference::Persistent { entity }) => {
            entity.namespace.as_str().len().saturating_add(16)
        }
        _ => 8,
    };
    budget.observe(depth, bytes, path)?;
    match value {
        ComponentValue::Map(values) => {
            for (name, value) in values {
                budget.bytes = budget.bytes.saturating_add(name.len());
                if budget.bytes > MAX_SHAPE_BYTES {
                    return Err(shape_error(path, ComponentShapeErrorKind::ByteLimit));
                }
                path.push(ComponentFieldPathSegment::field(name));
                validate_value_budget(value, depth + 1, budget, path)?;
                path.pop();
            }
        }
        ComponentValue::List(values) => {
            for (index, value) in values.iter().enumerate() {
                path.push(ComponentFieldPathSegment::index(index as u32));
                validate_value_budget(value, depth + 1, budget, path)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_component_shape_budget(
    schema: &ComponentSchema,
) -> Result<(), (ComponentFieldId, ComponentShapeError)> {
    let mut budget = ShapeBudget::default();
    for field in &schema.fields {
        if field.path.segments().len() > MAX_SHAPE_DEPTH {
            return Err((
                field.id.clone(),
                shape_error(&[], ComponentShapeErrorKind::DepthLimit),
            ));
        }
        let path_bytes = field
            .path
            .segments()
            .iter()
            .fold(0_usize, |bytes, segment| {
                bytes.saturating_add(match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => 4,
                })
            });
        budget.bytes = budget.bytes.saturating_add(path_bytes);
        if budget.bytes > MAX_SHAPE_BYTES {
            return Err((
                field.id.clone(),
                shape_error(&[], ComponentShapeErrorKind::ByteLimit),
            ));
        }
        let mut path = field.path.segments().to_vec();
        field
            .shape
            .validate_shape_node(path.len(), &mut budget, &mut path)
            .map_err(|error| (field.id.clone(), error))?;
    }
    Ok(())
}

pub(crate) fn validate_component_value_budget(
    value: &ComponentValue,
) -> Result<(), ComponentShapeError> {
    validate_value_budget(value, 0, &mut ShapeBudget::default(), &mut Vec::new())
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentFieldSchema {
    pub id: ComponentFieldId,
    pub aliases: Vec<String>,
    pub path: ComponentFieldPath,
    pub shape: ComponentValueShape,
    pub required: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub capabilities: BTreeSet<ComponentCapability>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub default_value: Option<ComponentValue>,
}

impl ComponentFieldSchema {
    #[must_use]
    pub fn required(
        id: ComponentFieldId,
        alias: impl Into<String>,
        path: ComponentFieldPath,
        shape: impl Into<ComponentValueShape>,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            shape: shape.into(),
            required: true,
            capabilities: BTreeSet::new(),
            default_value: None,
        }
    }

    #[must_use]
    pub fn optional(
        id: ComponentFieldId,
        alias: impl Into<String>,
        path: ComponentFieldPath,
        shape: impl Into<ComponentValueShape>,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            shape: ComponentValueShape::optional(shape.into()),
            required: false,
            capabilities: BTreeSet::new(),
            default_value: None,
        }
    }

    #[must_use]
    pub fn optional_with_default(
        id: ComponentFieldId,
        alias: impl Into<String>,
        path: ComponentFieldPath,
        shape: impl Into<ComponentValueShape>,
        default_value: ComponentValue,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            shape: ComponentValueShape::optional(shape.into()),
            required: false,
            capabilities: BTreeSet::new(),
            default_value: Some(default_value),
        }
    }

    #[must_use]
    pub fn with_aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.aliases.extend(aliases.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_numeric_domain(
        mut self,
        domain: impl Into<Option<ComponentNumericDomain>>,
    ) -> Self {
        self.shape = self.shape.with_numeric_domain(domain);
        self
    }

    #[must_use]
    pub fn with_capability(mut self, capability: ComponentCapability) -> Self {
        self.capabilities.insert(capability);
        self
    }

    #[must_use]
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = ComponentCapability>,
    ) -> Self {
        self.capabilities.extend(capabilities);
        self
    }

    #[must_use]
    pub const fn id(&self) -> &ComponentFieldId {
        &self.id
    }

    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    #[must_use]
    pub const fn path(&self) -> &ComponentFieldPath {
        &self.path
    }

    #[must_use]
    pub fn value_kind(&self) -> ComponentValueKind {
        self.shape.value_kind()
    }

    #[must_use]
    pub fn numeric_domain(&self) -> Option<ComponentNumericDomain> {
        self.shape.numeric_domain()
    }

    #[must_use]
    pub const fn shape(&self) -> &ComponentValueShape {
        &self.shape
    }

    /// Checks the declared wire kind and finite numeric domain.
    ///
    /// Optional fields retain their explicit-null semantics. Native codecs may impose further
    /// component-specific invariants that are not represented by this schema.
    #[must_use]
    pub fn accepts_value(&self, value: &ComponentValue) -> bool {
        self.shape.accepts_value(value)
    }

    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }

    #[must_use]
    pub const fn default_value(&self) -> Option<&ComponentValue> {
        self.default_value.as_ref()
    }

    #[must_use]
    pub fn has_capability(&self, capability: ComponentCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ComponentCapability {
    Scene,
    Inspect,
    Edit,
    AssetRef,
    EntityRef,
}

impl ComponentCapability {
    pub const SCENE_AUTHORING: [Self; 3] = [Self::Scene, Self::Inspect, Self::Edit];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentValueKind {
    Null,
    Bool,
    I64,
    U64,
    F64,
    String,
    List,
    Map,
    AssetRef,
    EntityRef,
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentValueKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F64 => "f64",
            Self::String => "string",
            Self::List => "list",
            Self::Map => "map",
            Self::AssetRef => "asset_ref",
            Self::EntityRef => "entity_ref",
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentValueKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(deserializer)?.as_str() {
            "null" => Ok(Self::Null),
            "bool" => Ok(Self::Bool),
            "i64" => Ok(Self::I64),
            "u64" => Ok(Self::U64),
            "f64" => Ok(Self::F64),
            "string" => Ok(Self::String),
            "list" => Ok(Self::List),
            "map" => Ok(Self::Map),
            "asset_ref" => Ok(Self::AssetRef),
            "entity_ref" => Ok(Self::EntityRef),
            _ => Err(serde::de::Error::custom("unknown component value kind")),
        }
    }
}

/// The admitted Rust numeric widths whose range is narrower than their wire kind.
///
/// An absent domain keeps the full I64/U64/F64 range. F32 accepts finite values within its
/// range; ordinary rounding and underflow during f64-to-f32 conversion are permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentNumericDomain {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    F32,
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentNumericDomain {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::F32 => "f32",
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentNumericDomain {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(deserializer)?.as_str() {
            "i8" => Ok(Self::I8),
            "i16" => Ok(Self::I16),
            "i32" => Ok(Self::I32),
            "u8" => Ok(Self::U8),
            "u16" => Ok(Self::U16),
            "u32" => Ok(Self::U32),
            "f32" => Ok(Self::F32),
            _ => Err(serde::de::Error::custom("unknown component numeric domain")),
        }
    }
}

impl ComponentNumericDomain {
    #[must_use]
    pub const fn value_kind(self) -> ComponentValueKind {
        match self {
            Self::I8 | Self::I16 | Self::I32 => ComponentValueKind::I64,
            Self::U8 | Self::U16 | Self::U32 => ComponentValueKind::U64,
            Self::F32 => ComponentValueKind::F64,
        }
    }

    #[must_use]
    pub fn accepts_value(self, value: &ComponentValue) -> bool {
        match (self, value) {
            (Self::I8, ComponentValue::I64(value)) => i8::try_from(*value).is_ok(),
            (Self::I16, ComponentValue::I64(value)) => i16::try_from(*value).is_ok(),
            (Self::I32, ComponentValue::I64(value)) => i32::try_from(*value).is_ok(),
            (Self::U8, ComponentValue::U64(value)) => u8::try_from(*value).is_ok(),
            (Self::U16, ComponentValue::U64(value)) => u16::try_from(*value).is_ok(),
            (Self::U32, ComponentValue::U64(value)) => u32::try_from(*value).is_ok(),
            (Self::F32, ComponentValue::F64(value)) => {
                value.get() >= f64::from(f32::MIN) && value.get() <= f64::from(f32::MAX)
            }
            _ => false,
        }
    }

    const fn fingerprint_tag(self) -> u8 {
        match self {
            Self::I8 => 0,
            Self::I16 => 1,
            Self::I32 => 2,
            Self::U8 => 3,
            Self::U16 => 4,
            Self::U32 => 5,
            Self::F32 => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogFingerprint([u8; 32]);

impl CatalogFingerprint {
    pub const HEX_LENGTH: usize = 64;

    #[must_use]
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Result<Self, CatalogFingerprintParseError> {
        if value.len() != Self::HEX_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
        {
            return Err(CatalogFingerprintParseError);
        }
        let hash = blake3::Hash::from_hex(value).map_err(|_| CatalogFingerprintParseError)?;
        Ok(Self(*hash.as_bytes()))
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

impl Display for CatalogFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogFingerprintParseError;

impl Display for CatalogFingerprintParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("catalog fingerprint must contain exactly 64 lowercase hex digits")
    }
}

impl Error for CatalogFingerprintParseError {}

#[cfg(feature = "serde")]
impl serde::Serialize for CatalogFingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CatalogFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(serde::de::Error::custom)
    }
}

fn feed_schema(hasher: &mut blake3::Hasher, schema: &ComponentSchema) {
    feed_bytes(hasher, b"component");
    feed_str(hasher, schema.id.as_str());
    feed_len(hasher, schema.aliases.len());
    let mut aliases = schema.aliases.iter().collect::<Vec<_>>();
    aliases.sort();
    for alias in aliases {
        feed_str(hasher, alias);
    }
    feed_bytes(hasher, &schema.version.get().to_le_bytes());
    feed_len(hasher, schema.capabilities.len());
    for capability in &schema.capabilities {
        feed_bytes(hasher, &[component_capability_tag(*capability)]);
    }
    feed_len(hasher, schema.fields.len());
    let mut fields = schema.fields.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.id.cmp(&right.id));
    for field in fields {
        feed_bytes(hasher, b"field");
        feed_str(hasher, field.id.as_str());
        feed_len(hasher, field.aliases.len());
        let mut aliases = field.aliases.iter().collect::<Vec<_>>();
        aliases.sort();
        for alias in aliases {
            feed_str(hasher, alias);
        }
        feed_path(hasher, &field.path);
        feed_shape(hasher, &field.shape);
        feed_bytes(hasher, &[u8::from(field.required)]);
        feed_len(hasher, field.capabilities.len());
        for capability in &field.capabilities {
            feed_bytes(hasher, &[component_capability_tag(*capability)]);
        }
        match &field.default_value {
            Some(value) => {
                feed_bytes(hasher, &[1]);
                feed_value(hasher, value);
            }
            None => feed_bytes(hasher, &[0]),
        }
    }
    feed_len(hasher, schema.field_tombstones.len());
    let mut tombstones = schema.field_tombstones.iter().collect::<Vec<_>>();
    tombstones.sort();
    for tombstone in tombstones {
        feed_bytes(hasher, b"field-tombstone");
        feed_str(hasher, tombstone.as_str());
    }
    // Creation metadata changes authoring identity, while absent metadata preserves v1 hashes.
    if let Some(value) = &schema.authoring_initial_value {
        feed_bytes(hasher, b"authoring-initial-value");
        feed_value(hasher, value);
    }
}

fn feed_shape(hasher: &mut blake3::Hasher, shape: &ComponentValueShape) {
    match shape {
        ComponentValueShape::Scalar {
            kind,
            numeric_domain,
        } => {
            feed_bytes(hasher, &[0, component_value_kind_tag(*kind)]);
            match numeric_domain {
                Some(domain) => feed_bytes(hasher, &[1, domain.fingerprint_tag()]),
                None => feed_bytes(hasher, &[0]),
            }
        }
        ComponentValueShape::EntityRef => feed_bytes(hasher, &[1]),
        ComponentValueShape::AssetRef => feed_bytes(hasher, &[2]),
        ComponentValueShape::Record { members } => {
            feed_bytes(hasher, &[3]);
            feed_len(hasher, members.len());
            let mut members = members.iter().collect::<Vec<_>>();
            members.sort_by(|left, right| left.name.cmp(&right.name));
            for member in members {
                feed_str(hasher, &member.name);
                feed_shape(hasher, &member.shape);
            }
        }
        ComponentValueShape::List { item } => {
            feed_bytes(hasher, &[4]);
            feed_shape(hasher, item);
        }
        ComponentValueShape::Map { value } => {
            feed_bytes(hasher, &[5]);
            feed_shape(hasher, value);
        }
        ComponentValueShape::Optional { value } => {
            feed_bytes(hasher, &[6]);
            feed_shape(hasher, value);
        }
        ComponentValueShape::Opaque { kind } => {
            feed_bytes(hasher, &[7, component_value_kind_tag(*kind)])
        }
    }
}

fn feed_path(hasher: &mut blake3::Hasher, path: &ComponentFieldPath) {
    feed_len(hasher, path.segments().len());
    for segment in path.segments() {
        match segment {
            ComponentFieldPathSegment::Field(field) => {
                feed_bytes(hasher, &[0]);
                feed_str(hasher, field);
            }
            ComponentFieldPathSegment::Index(index) => {
                feed_bytes(hasher, &[1]);
                feed_bytes(hasher, &index.to_le_bytes());
            }
        }
    }
}

fn feed_value(hasher: &mut blake3::Hasher, value: &ComponentValue) {
    feed_bytes(hasher, &[component_value_kind_tag(value.kind())]);
    match value {
        ComponentValue::Null => {}
        ComponentValue::Bool(value) => feed_bytes(hasher, &[u8::from(*value)]),
        ComponentValue::I64(value) => feed_bytes(hasher, &value.to_le_bytes()),
        ComponentValue::U64(value) => feed_bytes(hasher, &value.to_le_bytes()),
        ComponentValue::F64(value) => {
            let normalized = if value.get() == 0.0 { 0.0 } else { value.get() };
            feed_bytes(hasher, &normalized.to_bits().to_le_bytes());
        }
        ComponentValue::String(value) => feed_str(hasher, value),
        ComponentValue::List(values) => {
            feed_len(hasher, values.len());
            for value in values {
                feed_value(hasher, value);
            }
        }
        ComponentValue::Map(values) => {
            feed_len(hasher, values.len());
            for (key, value) in values {
                feed_str(hasher, key);
                feed_value(hasher, value);
            }
        }
        ComponentValue::EntityReference(reference) => match reference {
            EntityReference::SceneLocal { entity } => {
                feed_bytes(hasher, &[0]);
                feed_str(hasher, entity.as_str());
            }
            EntityReference::Persistent { entity } => {
                feed_bytes(hasher, &[1]);
                feed_str(hasher, entity.namespace.as_str());
                feed_str(hasher, &entity.entity.to_string());
            }
        },
    }
}

const fn component_capability_tag(capability: ComponentCapability) -> u8 {
    match capability {
        ComponentCapability::Scene => 0,
        ComponentCapability::Inspect => 1,
        ComponentCapability::Edit => 2,
        ComponentCapability::AssetRef => 3,
        ComponentCapability::EntityRef => 4,
    }
}

const fn component_value_kind_tag(kind: ComponentValueKind) -> u8 {
    match kind {
        ComponentValueKind::Null => 0,
        ComponentValueKind::Bool => 1,
        ComponentValueKind::I64 => 2,
        ComponentValueKind::U64 => 3,
        ComponentValueKind::F64 => 4,
        ComponentValueKind::String => 5,
        ComponentValueKind::List => 6,
        ComponentValueKind::Map => 7,
        ComponentValueKind::AssetRef => 8,
        ComponentValueKind::EntityRef => 9,
    }
}

fn feed_str(hasher: &mut blake3::Hasher, value: &str) {
    feed_bytes(hasher, value.as_bytes());
}

fn feed_len(hasher: &mut blake3::Hasher, len: usize) {
    let len = u64::try_from(len).unwrap_or(u64::MAX);
    hasher.update(&len.to_le_bytes());
}

fn feed_bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}
