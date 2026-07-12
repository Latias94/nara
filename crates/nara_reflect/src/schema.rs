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
        let mut catalog = self.clone();
        catalog.canonicalize();
        let mut hasher = blake3::Hasher::new();
        // Generation and predecessor are deliberately excluded: this is a canonical content
        // fingerprint, and including predecessor would make the lineage identity recursive.
        feed_bytes(&mut hasher, b"nara.component-schema-catalog-fingerprint.v1");
        feed_len(&mut hasher, catalog.components.len());
        for schema in &catalog.components {
            feed_schema(&mut hasher, schema);
        }
        feed_len(&mut hasher, catalog.type_tombstones.len());
        for tombstone in &catalog.type_tombstones {
            feed_bytes(&mut hasher, b"type-tombstone");
            feed_str(&mut hasher, tombstone.as_str());
        }
        CatalogFingerprint(*hasher.finalize().as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentFieldSchema {
    pub id: ComponentFieldId,
    pub aliases: Vec<String>,
    pub path: ComponentFieldPath,
    pub value_kind: ComponentValueKind,
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
        value_kind: ComponentValueKind,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            value_kind,
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
        value_kind: ComponentValueKind,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            value_kind,
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
        value_kind: ComponentValueKind,
        default_value: ComponentValue,
    ) -> Self {
        Self {
            id,
            aliases: vec![alias.into()],
            path,
            value_kind,
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
    pub const fn value_kind(&self) -> ComponentValueKind {
        self.value_kind
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogFingerprint([u8; 32]);

impl CatalogFingerprint {
    pub const HEX_LENGTH: usize = 64;

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
    for alias in &schema.aliases {
        feed_str(hasher, alias);
    }
    feed_bytes(hasher, &schema.version.get().to_le_bytes());
    feed_len(hasher, schema.capabilities.len());
    for capability in &schema.capabilities {
        feed_bytes(hasher, &[component_capability_tag(*capability)]);
    }
    feed_len(hasher, schema.fields.len());
    for field in &schema.fields {
        feed_bytes(hasher, b"field");
        feed_str(hasher, field.id.as_str());
        feed_len(hasher, field.aliases.len());
        for alias in &field.aliases {
            feed_str(hasher, alias);
        }
        feed_path(hasher, &field.path);
        feed_bytes(
            hasher,
            &[
                component_value_kind_tag(field.value_kind),
                u8::from(field.required),
            ],
        );
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
    for tombstone in &schema.field_tombstones {
        feed_bytes(hasher, b"field-tombstone");
        feed_str(hasher, tombstone.as_str());
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
