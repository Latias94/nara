//! Strict, bounded source action-map JSON files. Runtime state is never persisted.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io::{self, Write},
    marker::PhantomData,
    mem::size_of,
    string::FromUtf8Error,
};

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, EngineVersionParseError, FormatGenerator,
    FormatGeneratorError, FormatKind, FormatKindError, FormatVersion, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeError, SerdeShapeLimits,
    SerdeShapePreflightError, decode_persistent_file_with_preflight, preflight_serde_shape,
};
use serde::{
    Deserialize, Serialize, Serializer,
    de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
    ser::SerializeSeq,
};

use crate::{
    ActionBinding, ActionContext, ActionContextDefinition, ActionId, ActionIdError,
    ActionMapBuilder, ActionMapDefinition, ActionMapError, ActionMapLimits, InputBinding,
    MAX_ACTION_BINDINGS, MAX_ACTION_CONTEXTS, MAX_ACTION_IDENTIFIER_BYTES,
};

pub const ACTION_MAP_FORMAT_KIND: &str = "action_map";
const ENGINE_MIN_VERSION: &str = "0.1.0";

/// Independent encoded, structural and compiled-definition limits for a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionMapFileLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
    definition: ActionMapLimits,
}

impl Default for ActionMapFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(256 * 1024).expect("non-zero action-map byte limit"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(16).expect("non-zero action-map depth limit"),
                ItemLimit::new(16_384).expect("non-zero action-map node limit"),
                ItemLimit::new(1_024).expect("non-zero action-map container limit"),
                ByteLimit::new(128).expect("non-zero action-map string limit"),
                ByteLimit::new(128 * 1024).expect("non-zero action-map total string limit"),
            ),
            definition: ActionMapLimits::default(),
        }
    }
}

impl ActionMapFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }

    #[must_use]
    pub const fn definition(self) -> ActionMapLimits {
        self.definition
    }

    #[must_use]
    pub const fn with_encoded_bytes(mut self, limit: ByteLimit) -> Self {
        self.encoded_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_shape(mut self, limits: SerdeShapeLimits) -> Self {
        self.shape = limits;
        self
    }

    #[must_use]
    pub const fn with_definition(mut self, limits: ActionMapLimits) -> Self {
        self.definition = limits;
        self
    }

    /// Conservative byte reservation before decoding, including an externally held encoded
    /// buffer, JSON scratch, structural keys, compiler work and the completed definition.
    ///
    /// This bounds requested storage, not allocator metadata or process RSS. Decoding uses no
    /// intermediate domain collections. World-local state needs its own admission using the
    /// compiled definition's `runtime_state_bytes()`. `None` means reservation arithmetic
    /// overflowed; a Host must refuse that reservation rather than treating it as zero.
    #[must_use]
    pub fn predecode_work_bytes(self) -> Option<usize> {
        // Each node receives room for a sparse String-keyed B-tree node and visitor metadata.
        // Strings are charged separately; no domain Vec coexists with the compiler's tables.
        let node_bytes = 16_usize.checked_mul(size_of::<String>() + size_of::<usize>())?;
        let key_storage = self.shape.nodes().get().checked_mul(node_bytes)?;
        let structural_strings = self.shape.total_string_bytes().get().checked_mul(2)?;
        // One caller-owned input plus geometric parser scratch, escaped header values and
        // the bounded identifier currently being transferred into the compiler.
        let parser_and_input = self.encoded_bytes.get().checked_mul(4)?;
        let traversal = self.shape.depth().get().checked_mul(512)?;
        parser_and_input
            .checked_add(key_storage)?
            .checked_add(structural_strings)?
            .checked_add(traversal)?
            .checked_add(self.definition.work_bytes().get())?
            .checked_add(self.definition.retained_bytes().get())?
            .checked_add(size_of::<Self>() + size_of::<ActionMapFileCandidate>())
    }
}

/// An intrinsic primary Product and its move-only, compiled authoring definition.
///
/// This owns model data only. Mapping lifetimes, context mutations, held actions and World
/// installation belong to the runtime Input owner.
#[derive(Debug)]
pub struct ActionMapFileCandidate {
    product: AssetProductRef,
    definition: ActionMapDefinition,
}

impl ActionMapFileCandidate {
    pub fn new(
        product: AssetProductRef,
        definition: ActionMapDefinition,
    ) -> Result<Self, ActionMapFileError> {
        require_primary(product)?;
        Ok(Self {
            product,
            definition,
        })
    }

    pub fn decode_json_bytes(input: &[u8]) -> Result<Self, ActionMapFileError> {
        Self::decode_json_bytes_with_limits(input, ActionMapFileLimits::default())
    }

    pub fn decode_json_bytes_with_limits(
        input: &[u8],
        limits: ActionMapFileLimits,
    ) -> Result<Self, ActionMapFileError> {
        let contract = PersistentFileContract::canonical_v1(
            format_kind()?,
            engine_version(env!("CARGO_PKG_VERSION"))?,
        );
        let envelope = decode_persistent_file_with_preflight(
            input,
            limits.encoded_bytes(),
            &contract,
            |bytes| {
                let mut parser = serde_json::Deserializer::from_slice(bytes);
                preflight_serde_shape(&mut parser, limits.shape()).map_err(
                    |error| match error {
                        SerdeShapePreflightError::Shape(error) => ActionMapFileError::Shape(error),
                        SerdeShapePreflightError::Parse(error) => {
                            ActionMapFileError::JsonParse(error)
                        }
                    },
                )?;
                parser.end().map_err(ActionMapFileError::JsonParse)
            },
            |bytes| {
                serde_json::from_slice::<ObjectOnly<PersistentFileHeader>>(bytes)
                    .map(|record| record.0)
                    .map_err(ActionMapFileError::JsonParse)
            },
            |bytes| {
                let mut builder = ActionMapBuilder::new(limits.definition());
                // Declare every context first, regardless of JSON member order. The second
                // pass streams bindings straight into the same compiler, without raw Vecs.
                decode_pass(bytes, limits.definition(), Pass::Contexts, &mut builder)?;
                let envelope =
                    decode_pass(bytes, limits.definition(), Pass::Bindings, &mut builder)?;
                let definition = builder.compile().map_err(ActionMapFileError::Definition)?;
                Ok(envelope.map_payload(|product| Self {
                    product,
                    definition,
                }))
            },
        )
        .map_err(|error| match error {
            PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
                ActionMapFileError::EncodedBytesExceeded { observed, maximum }
            }
            PersistentFileDecodeError::Shape(error)
            | PersistentFileDecodeError::Header(error)
            | PersistentFileDecodeError::Payload(error) => error,
            PersistentFileDecodeError::Contract(error) => ActionMapFileError::Contract(error),
        })?;
        Ok(envelope.into_payload())
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub const fn definition(&self) -> &ActionMapDefinition {
        &self.definition
    }

    #[must_use]
    pub fn into_parts(self) -> (AssetProductRef, ActionMapDefinition) {
        (self.product, self.definition)
    }

    pub fn to_json_string(&self) -> Result<String, ActionMapFileError> {
        self.to_json_string_with_limits(ActionMapFileLimits::default())
    }

    /// Writes borrowed model data, then admits the result through the same reader and limits.
    pub fn to_json_string_with_limits(
        &self,
        limits: ActionMapFileLimits,
    ) -> Result<String, ActionMapFileError> {
        let envelope = PersistentFileEnvelope::canonical_v1(
            format_kind()?,
            engine_version(ENGINE_MIN_VERSION)?,
            FormatGenerator::new("nara", engine_version(env!("CARGO_PKG_VERSION"))?)
                .map_err(ActionMapFileError::Generator)?,
            PayloadRef {
                product: self.product,
                contexts: ContextsRef(self.definition.contexts()),
                bindings: self.definition.bindings(),
            },
        );
        let mut output = BoundedOutput {
            bytes: Vec::new(),
            maximum: limits.encoded_bytes().get(),
            exceeded: None,
        };
        if let Err(error) = serde_json::to_writer_pretty(&mut output, &envelope) {
            return Err(match output.exceeded {
                Some(observed) => ActionMapFileError::EncodedBytesExceeded {
                    observed,
                    maximum: output.maximum,
                },
                None => ActionMapFileError::Encode(error),
            });
        }
        Self::decode_json_bytes_with_limits(&output.bytes, limits)?;
        String::from_utf8(output.bytes).map_err(ActionMapFileError::EncodedUtf8)
    }
}

/// Concrete failures survive the serde visitor boundary; only JSON grammar errors use serde's
/// syntax-specific error type.
#[derive(Debug)]
pub enum ActionMapFileError {
    EncodedBytesExceeded { observed: usize, maximum: usize },
    Shape(SerdeShapeError),
    Contract(PersistentFileContractError),
    JsonParse(serde_json::Error),
    PrimaryProductRequired { product: AssetProductRef },
    ContextLimitExceeded { maximum: usize },
    BindingLimitExceeded { maximum: usize },
    IdentifierBytesExceeded { observed: usize, maximum: usize },
    Identifier(ActionIdError),
    Definition(ActionMapError),
    FormatKind(FormatKindError),
    EngineVersion(EngineVersionParseError),
    Generator(FormatGeneratorError),
    Encode(serde_json::Error),
    EncodedUtf8(FromUtf8Error),
}

impl Display for ActionMapFileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "action map contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape(error) => write!(formatter, "action-map shape is invalid: {error}"),
            Self::Contract(error) => Display::fmt(error, formatter),
            Self::JsonParse(error) => write!(formatter, "invalid action-map JSON: {error}"),
            Self::PrimaryProductRequired { product } => write!(
                formatter,
                "action map requires an intrinsic primary Product, found {product}"
            ),
            Self::ContextLimitExceeded { maximum } => {
                write!(formatter, "action-map context count exceeds {maximum}")
            }
            Self::BindingLimitExceeded { maximum } => {
                write!(formatter, "action-map binding count exceeds {maximum}")
            }
            Self::IdentifierBytesExceeded { observed, maximum } => write!(
                formatter,
                "action-map identifier contains {observed} bytes, maximum is {maximum}"
            ),
            Self::Identifier(error) => write!(formatter, "invalid action-map identifier: {error}"),
            Self::Definition(error) => write!(formatter, "invalid action-map definition: {error}"),
            Self::FormatKind(error) => Display::fmt(error, formatter),
            Self::EngineVersion(error) => Display::fmt(error, formatter),
            Self::Generator(error) => Display::fmt(error, formatter),
            Self::Encode(error) => write!(formatter, "action-map encoding failed: {error}"),
            Self::EncodedUtf8(error) => {
                write!(formatter, "action-map output is not UTF-8: {error}")
            }
        }
    }
}

impl Error for ActionMapFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::JsonParse(error) | Self::Encode(error) => Some(error),
            Self::Identifier(error) => Some(error),
            Self::Definition(error) => Some(error),
            Self::FormatKind(error) => Some(error),
            Self::EngineVersion(error) => Some(error),
            Self::Generator(error) => Some(error),
            Self::EncodedUtf8(error) => Some(error),
            Self::EncodedBytesExceeded { .. }
            | Self::PrimaryProductRequired { .. }
            | Self::ContextLimitExceeded { .. }
            | Self::BindingLimitExceeded { .. }
            | Self::IdentifierBytesExceeded { .. } => None,
        }
    }
}

fn format_kind() -> Result<FormatKind, ActionMapFileError> {
    FormatKind::new(ACTION_MAP_FORMAT_KIND).map_err(ActionMapFileError::FormatKind)
}

fn engine_version(value: &str) -> Result<EngineVersion, ActionMapFileError> {
    EngineVersion::parse(value).map_err(ActionMapFileError::EngineVersion)
}

fn require_primary(product: AssetProductRef) -> Result<(), ActionMapFileError> {
    if product.product() != ImportedProductId::PRIMARY {
        return Err(ActionMapFileError::PrimaryProductRequired { product });
    }
    Ok(())
}

#[derive(Serialize)]
struct PayloadRef<'a> {
    product: AssetProductRef,
    contexts: ContextsRef<'a>,
    bindings: &'a [ActionBinding],
}

struct ContextsRef<'a>(&'a [ActionContextDefinition]);

impl Serialize for ContextsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct ContextRef<'a> {
            context: &'a ActionContext,
            enabled: bool,
        }

        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for entry in self.0 {
            sequence.serialize_element(&ContextRef {
                context: &entry.context,
                enabled: entry.enabled,
            })?;
        }
        sequence.end()
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: Option<usize>,
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let observed = self.bytes.len().saturating_add(bytes.len());
        if observed > self.maximum {
            self.exceeded = Some(observed);
            return Err(io::Error::other("action-map encoded byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// Derived serde records may also accept positional arrays. This adapter deliberately exposes
// only MapAccess, including when reusing Core's header and generator implementations.
struct ObjectOnly<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = ObjectOnly<T>;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object record")
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(de::value::MapAccessDeserializer::new(map)).map(ObjectOnly)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Clone, Copy)]
enum Pass {
    Contexts,
    Bindings,
}

struct DecodeState {
    limits: ActionMapLimits,
    failure: Option<ActionMapFileError>,
}

impl DecodeState {
    fn fail<E: de::Error>(&mut self, error: ActionMapFileError) -> E {
        let syntax_error = E::custom(&error);
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        syntax_error
    }
}

fn decode_pass(
    input: &[u8],
    limits: ActionMapLimits,
    pass: Pass,
    builder: &mut ActionMapBuilder,
) -> Result<PersistentFileEnvelope<AssetProductRef>, ActionMapFileError> {
    let mut state = DecodeState {
        limits,
        failure: None,
    };
    let mut parser = serde_json::Deserializer::from_slice(input);
    let result = EnvelopeSeed {
        payload: PayloadSeed {
            pass,
            builder,
            state: &mut state,
        },
    }
    .deserialize(&mut parser);
    match result {
        Ok(envelope) => {
            parser.end().map_err(ActionMapFileError::JsonParse)?;
            Ok(envelope)
        }
        Err(error) => Err(state
            .failure
            .unwrap_or(ActionMapFileError::JsonParse(error))),
    }
}

struct EnvelopeSeed<'a> {
    payload: PayloadSeed<'a>,
}

impl<'de> DeserializeSeed<'de> for EnvelopeSeed<'_> {
    type Value = PersistentFileEnvelope<AssetProductRef>;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for EnvelopeSeed<'_> {
    type Value = PersistentFileEnvelope<AssetProductRef>;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("an action-map envelope object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            FormatVersion,
            EngineMinVersion,
            Generator,
            Payload,
        }

        let mut kind = None;
        let mut version = None;
        let mut engine = None;
        let mut generator = None;
        let mut payload = None;
        let mut payload_seed = Some(self.payload);
        while let Some(field) = map.next_key()? {
            match field {
                Field::Kind => {
                    if kind.is_some() {
                        return Err(de::Error::duplicate_field("kind"));
                    }
                    kind = Some(map.next_value::<FormatKind>()?);
                }
                Field::FormatVersion => {
                    if version.is_some() {
                        return Err(de::Error::duplicate_field("format_version"));
                    }
                    version = Some(map.next_value::<FormatVersion>()?);
                }
                Field::EngineMinVersion => {
                    if engine.is_some() {
                        return Err(de::Error::duplicate_field("engine_min_version"));
                    }
                    engine = Some(map.next_value::<EngineVersion>()?);
                }
                Field::Generator => {
                    if generator.is_some() {
                        return Err(de::Error::duplicate_field("generator"));
                    }
                    generator = Some(map.next_value::<ObjectOnly<FormatGenerator>>()?.0);
                }
                Field::Payload => {
                    let seed = payload_seed
                        .take()
                        .ok_or_else(|| de::Error::duplicate_field("payload"))?;
                    payload = Some(map.next_value_seed(seed)?);
                }
            }
        }
        Ok(PersistentFileEnvelope::new(
            kind.ok_or_else(|| de::Error::missing_field("kind"))?,
            version.ok_or_else(|| de::Error::missing_field("format_version"))?,
            engine.ok_or_else(|| de::Error::missing_field("engine_min_version"))?,
            generator.ok_or_else(|| de::Error::missing_field("generator"))?,
            payload.ok_or_else(|| de::Error::missing_field("payload"))?,
        ))
    }
}

struct PayloadSeed<'a> {
    pass: Pass,
    builder: &'a mut ActionMapBuilder,
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for PayloadSeed<'_> {
    type Value = AssetProductRef;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for PayloadSeed<'_> {
    type Value = AssetProductRef;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("an action-map payload object")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Product,
            Contexts,
            Bindings,
        }

        let mut product = None;
        let mut contexts = false;
        let mut bindings = false;
        while let Some(field) = map.next_key()? {
            match field {
                Field::Product => {
                    if product.is_some() {
                        return Err(de::Error::duplicate_field("product"));
                    }
                    let reference = map.next_value::<AssetProductRef>()?;
                    require_primary(reference)
                        .map_err(|error| self.state.fail::<M::Error>(error))?;
                    product = Some(reference);
                }
                Field::Contexts => {
                    if contexts {
                        return Err(de::Error::duplicate_field("contexts"));
                    }
                    contexts = true;
                    if matches!(self.pass, Pass::Contexts) {
                        map.next_value_seed(RecordsSeed {
                            kind: Pass::Contexts,
                            builder: self.builder,
                            state: self.state,
                        })?;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Field::Bindings => {
                    if bindings {
                        return Err(de::Error::duplicate_field("bindings"));
                    }
                    bindings = true;
                    if matches!(self.pass, Pass::Bindings) {
                        map.next_value_seed(RecordsSeed {
                            kind: Pass::Bindings,
                            builder: self.builder,
                            state: self.state,
                        })?;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
            }
        }
        if !contexts {
            return Err(de::Error::missing_field("contexts"));
        }
        if !bindings {
            return Err(de::Error::missing_field("bindings"));
        }
        product.ok_or_else(|| de::Error::missing_field("product"))
    }
}

struct RecordsSeed<'a> {
    kind: Pass,
    builder: &'a mut ActionMapBuilder,
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for RecordsSeed<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for RecordsSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of action-map object records")
    }

    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<(), S::Error> {
        let mut count = 0;
        while sequence
            .next_element_seed(RecordSeed {
                kind: self.kind,
                index: count,
                builder: self.builder,
                state: self.state,
            })?
            .is_some()
        {
            count += 1;
        }
        Ok(())
    }
}

struct RecordSeed<'a> {
    kind: Pass,
    index: usize,
    builder: &'a mut ActionMapBuilder,
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for RecordSeed<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        match self.kind {
            Pass::Contexts => {
                let maximum = self.state.limits.contexts().get().min(MAX_ACTION_CONTEXTS);
                if self.index >= maximum {
                    return Err(self
                        .state
                        .fail(ActionMapFileError::ContextLimitExceeded { maximum }));
                }
                let (context, enabled) =
                    ContextSeed { state: self.state }.deserialize(deserializer)?;
                self.builder
                    .declare_context(context, enabled)
                    .map_err(|error| {
                        self.state
                            .fail::<D::Error>(ActionMapFileError::Definition(error))
                    })?;
            }
            Pass::Bindings => {
                let maximum = self.state.limits.bindings().get().min(MAX_ACTION_BINDINGS);
                if self.index >= maximum {
                    return Err(self
                        .state
                        .fail(ActionMapFileError::BindingLimitExceeded { maximum }));
                }
                let binding = BindingSeed { state: self.state }.deserialize(deserializer)?;
                self.builder.bind(binding).map_err(|error| {
                    self.state
                        .fail::<D::Error>(ActionMapFileError::Definition(error))
                })?;
            }
        }
        Ok(())
    }
}

struct ContextSeed<'a> {
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for ContextSeed<'_> {
    type Value = (ActionContext, bool);

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ContextSeed<'_> {
    type Value = (ActionContext, bool);

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a context object with context and enabled members")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Context,
            Enabled,
        }

        let mut context = None;
        let mut enabled = None;
        while let Some(field) = map.next_key()? {
            match field {
                Field::Context => {
                    if context.is_some() {
                        return Err(de::Error::duplicate_field("context"));
                    }
                    let value = map.next_value_seed(IdentifierSeed { state: self.state })?;
                    context = Some(ActionContext::new(value).map_err(|error| {
                        self.state
                            .fail::<M::Error>(ActionMapFileError::Identifier(error))
                    })?);
                }
                Field::Enabled => {
                    if enabled.is_some() {
                        return Err(de::Error::duplicate_field("enabled"));
                    }
                    enabled = Some(map.next_value::<bool>()?);
                }
            }
        }
        Ok((
            context.ok_or_else(|| de::Error::missing_field("context"))?,
            enabled.ok_or_else(|| de::Error::missing_field("enabled"))?,
        ))
    }
}

struct BindingSeed<'a> {
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for BindingSeed<'_> {
    type Value = ActionBinding;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for BindingSeed<'_> {
    type Value = ActionBinding;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a binding object with action, input and context members")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Action,
            Input,
            Context,
        }

        let mut action = None;
        let mut input = None;
        let mut context = None;
        while let Some(field) = map.next_key()? {
            match field {
                Field::Action => {
                    if action.is_some() {
                        return Err(de::Error::duplicate_field("action"));
                    }
                    let value = map.next_value_seed(IdentifierSeed { state: self.state })?;
                    action = Some(ActionId::new(value).map_err(|error| {
                        self.state
                            .fail::<M::Error>(ActionMapFileError::Identifier(error))
                    })?);
                }
                Field::Input => {
                    if input.is_some() {
                        return Err(de::Error::duplicate_field("input"));
                    }
                    input = Some(map.next_value::<InputBinding>()?);
                }
                Field::Context => {
                    if context.is_some() {
                        return Err(de::Error::duplicate_field("context"));
                    }
                    let value = map.next_value_seed(IdentifierSeed { state: self.state })?;
                    context = Some(ActionContext::new(value).map_err(|error| {
                        self.state
                            .fail::<M::Error>(ActionMapFileError::Identifier(error))
                    })?);
                }
            }
        }
        Ok(ActionBinding {
            action: action.ok_or_else(|| de::Error::missing_field("action"))?,
            input: input.ok_or_else(|| de::Error::missing_field("input"))?,
            context: context.ok_or_else(|| de::Error::missing_field("context"))?,
        })
    }
}

struct IdentifierSeed<'a> {
    state: &'a mut DecodeState,
}

impl<'de> DeserializeSeed<'de> for IdentifierSeed<'_> {
    type Value = String;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<String, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for IdentifierSeed<'_> {
    type Value = String;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded action-map identifier string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
        let maximum = self
            .state
            .limits
            .identifier_bytes()
            .get()
            .min(MAX_ACTION_IDENTIFIER_BYTES);
        if value.len() > maximum {
            return Err(self
                .state
                .fail(ActionMapFileError::IdentifierBytesExceeded {
                    observed: value.len(),
                    maximum,
                }));
        }
        Ok(value.to_owned())
    }
}
