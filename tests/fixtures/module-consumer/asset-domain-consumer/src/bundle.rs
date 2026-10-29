//! Bounded compound text source, explicit authored continuity, and a cooked Runtime format.
//!
//! Sources contain only `suffix` and `entries: [{name, text}]`. Row numbers are addresses in
//! exactly one byte snapshot, never product identity. Identical-byte row exchanges cannot be
//! distinguished. Authors explicitly assign every row and preserve that mapping in metadata.
//! Neither import caches nor display names supply continuity authority.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetPath,
    AssetProductRef, AssetRuntimeError, AssetRuntimeLimits, ImportedProductId, RuntimeAssetCodec,
    SourceAssetImporter, SourceHash, StableAssetId,
};
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use serde::{Deserialize, Deserializer, Serialize, de};

/// Fixed limits keep ordinary metadata within the shared metadata format's shape allowance.
pub const MAX_BUNDLE_ENTRIES: usize = 8;
/// Rebinding refuses when its retained retirement record would exceed this bound.
pub const MAX_BUNDLE_RETIRED: usize = 16;
pub const MAX_BUNDLE_CHARACTERS: usize = 4096;
pub const MAX_BUNDLE_SOURCE_BYTES: usize = 32 * 1024;
pub const MAX_BUNDLE_SUFFIX_BYTES: usize = 1024;
const MAX_NAME_BYTES: usize = 128;
const MAX_PATH_BYTES: usize = 512;
const MAX_ENTRY_TEXT_BYTES: usize = MAX_BUNDLE_CHARACTERS * 4;
const MAX_COOKED_TEXT_BYTES: usize = 64 * 1024;
const MAX_RUNTIME_BYTES: usize = 512 * 1024;
const RETAINED_BYTES: usize = 256 * 1024;
const ENGINE_VERSION: &str = "0.1.0";

pub struct TextBundleDomain;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextBundleSettings {
    pub max_characters: usize,
    pub continuity: TextBundleContinuity,
}

/// Authored authority for one exact source revision, with a bounded known-retired set.
///
/// Persist this alongside source metadata. Rebind requires explicit decisions from its author;
/// it does not perform IO or promise atomic persistence with the source. Losing this record
/// loses continuity authority. The bounded retired set is not unlimited historical nonreuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextBundleContinuity {
    source_hash: SourceHash,
    #[serde(deserialize_with = "deserialize_assignments")]
    assignments: Vec<TextBundleRowAssignment>,
    #[serde(deserialize_with = "deserialize_retired")]
    retired: Vec<ImportedProductId>,
}

/// One explicit initial row-to-product assignment. Primary belongs to the whole bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextBundleRowAssignment {
    pub row: usize,
    pub product: ImportedProductId,
}

/// Authored intent: keep a currently active product, or introduce an explicitly supplied ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBundleIdentityDecision {
    Continue(ImportedProductId),
    New(ImportedProductId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextBundleRebindAssignment {
    pub row: usize,
    pub identity: TextBundleIdentityDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBundleEntry {
    pub name: String,
    pub text: String,
}

/// Cooked data only. No raw source, settings, paths, continuity record or read capability remains.
#[derive(Debug, PartialEq, Eq)]
pub struct TextBundle {
    source: StableAssetId,
    entries: BTreeMap<ImportedProductId, TextBundleEntry>,
}

impl TextBundle {
    #[must_use]
    pub const fn source(&self) -> StableAssetId {
        self.source
    }

    #[must_use]
    pub const fn entries(&self) -> &BTreeMap<ImportedProductId, TextBundleEntry> {
        &self.entries
    }

    /// Checks the enclosing source as well as the file-local product identity.
    #[must_use]
    pub fn get(&self, reference: &AssetProductRef) -> Option<&TextBundleEntry> {
        if reference.source() != self.source {
            return None;
        }
        self.entries.get(&reference.product())
    }
}

impl TextBundleSettings {
    /// Initial authorship explicitly supplies one ID per row in source order; no IDs are minted.
    pub fn initial(
        raw: &[u8],
        max_characters: usize,
        products: impl IntoIterator<Item = ImportedProductId>,
    ) -> Result<Self, TextBundleError> {
        validate_maximum(max_characters)?;
        let mut assignments = Vec::new();
        for product in products.into_iter().take(MAX_BUNDLE_ENTRIES + 1) {
            if assignments.len() == MAX_BUNDLE_ENTRIES {
                return Err(invalid_continuity());
            }
            assignments.push(TextBundleRowAssignment {
                row: assignments.len(),
                product,
            });
        }
        Ok(Self {
            max_characters,
            continuity: TextBundleContinuity::author_initial(raw, &assignments)?,
        })
    }

    pub fn rebind(
        &self,
        raw: &[u8],
        assignments: &[TextBundleRebindAssignment],
    ) -> Result<Self, TextBundleError> {
        self.validate()?;
        let candidate = Self {
            max_characters: self.max_characters,
            continuity: self.continuity.rebind(raw, assignments)?,
        };
        candidate.validate_successor(self)?;
        Ok(candidate)
    }

    /// Checks metadata transition facts without acquiring source or publication authority.
    ///
    /// A Host must call this against its admitted predecessor before publishing a successor.
    /// Importing candidate settings alone cannot establish that predecessor relationship.
    pub fn validate_successor(&self, previous: &Self) -> Result<(), TextBundleError> {
        previous.validate()?;
        self.validate()?;
        let old_active = previous.continuity.active_ids();
        let new_active = self.continuity.active_ids();
        let old_retired: BTreeSet<_> = previous.continuity.retired.iter().copied().collect();
        let new_retired: BTreeSet<_> = self.continuity.retired.iter().copied().collect();
        if !old_retired.is_subset(&new_retired)
            || !new_active.is_disjoint(&old_retired)
            || old_active
                .difference(&new_active)
                .any(|id| !new_retired.contains(id))
        {
            return Err(invalid_transition());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), TextBundleError> {
        validate_maximum(self.max_characters)?;
        self.continuity.validate(self.continuity.assignments.len())
    }
}

impl TextBundleContinuity {
    pub fn author_initial(
        raw_bytes: &[u8],
        assignments: &[TextBundleRowAssignment],
    ) -> Result<Self, TextBundleError> {
        let raw = decode_source(raw_bytes)?;
        validate_assignments(assignments, raw.entries.len())?;
        let mut assignments = assignments.to_vec();
        assignments.sort_unstable_by_key(|assignment| assignment.row);
        Ok(Self {
            source_hash: SourceHash::from_bytes(raw_bytes),
            assignments,
            retired: Vec::new(),
        })
    }

    /// Rebinds every new row explicitly. Continue requires an old active ID; New requires an
    /// ID absent from both old active and known-retired sets. Removed active IDs become retired.
    pub fn rebind(
        &self,
        new_raw_bytes: &[u8],
        assignments: &[TextBundleRebindAssignment],
    ) -> Result<Self, TextBundleError> {
        self.validate(self.assignments.len())?;
        let raw = decode_source(new_raw_bytes)?;
        if assignments.len() != raw.entries.len() || assignments.len() > MAX_BUNDLE_ENTRIES {
            return Err(invalid_continuity());
        }
        let active = self.active_ids();
        let mut retired: BTreeSet<_> = self.retired.iter().copied().collect();
        let mut rows = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            let product = match assignment.identity {
                TextBundleIdentityDecision::Continue(id) if active.contains(&id) => id,
                TextBundleIdentityDecision::New(id)
                    if !active.contains(&id) && !retired.contains(&id) =>
                {
                    id
                }
                _ => return Err(invalid_transition()),
            };
            rows.push(TextBundleRowAssignment {
                row: assignment.row,
                product,
            });
        }
        validate_assignments(&rows, raw.entries.len())?;
        let next_active: BTreeSet<_> = rows.iter().map(|row| row.product).collect();
        retired.extend(active.difference(&next_active).copied());
        if retired.len() > MAX_BUNDLE_RETIRED {
            return Err(retirement_limit());
        }
        rows.sort_unstable_by_key(|assignment| assignment.row);
        Ok(Self {
            source_hash: SourceHash::from_bytes(new_raw_bytes),
            assignments: rows,
            retired: retired.into_iter().collect(),
        })
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub fn assignments(&self) -> &[TextBundleRowAssignment] {
        &self.assignments
    }

    #[must_use]
    pub fn retired(&self) -> &[ImportedProductId] {
        &self.retired
    }

    fn active_ids(&self) -> BTreeSet<ImportedProductId> {
        self.assignments
            .iter()
            .map(|assignment| assignment.product)
            .collect()
    }

    fn validate(&self, row_count: usize) -> Result<(), TextBundleError> {
        validate_assignments(&self.assignments, row_count)?;
        if self.retired.len() > MAX_BUNDLE_RETIRED {
            return Err(retirement_limit());
        }
        let active = self.active_ids();
        let mut retired = BTreeSet::new();
        for id in &self.retired {
            if id.as_subproduct().is_none() || active.contains(id) || !retired.insert(*id) {
                return Err(invalid_continuity());
            }
        }
        Ok(())
    }
}

impl AssetDomain for TextBundleDomain {
    type Product = TextBundle;
}

impl SourceAssetImporter for TextBundleDomain {
    type Settings = TextBundleSettings;
    const SOURCE_KIND: &'static str = "consumer.text-bundle";

    fn limits() -> AssetImportLimits {
        // Includes bounded settings, source parser scratch, raw strings, suffix and cooked output.
        // Primary/auxiliary immutable observations are accounted separately by the Host.
        AssetImportLimits::new(
            byte_limit(MAX_BUNDLE_SOURCE_BYTES),
            byte_limit(512 * 1024),
            byte_limit(RETAINED_BYTES),
        )
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<TextBundle, AssetImportError<E>> {
        let identity = source.metadata().identity();
        if identity.source_kind()
            != &nara_asset::AssetSourceKind::Other(Self::SOURCE_KIND.to_owned())
            || identity.stable_id().as_uuid().is_nil()
        {
            return Err(import_error(invalid_source()));
        }
        let source_id = identity.stable_id();
        let settings = source
            .metadata()
            .admit::<Self::Settings>()
            .map_err(|_| import_error(invalid_settings()))?;
        let settings = settings.settings();
        settings.validate().map_err(import_error)?;
        let raw = decode_source(source.bytes()).map_err(import_error)?;
        if settings.continuity.source_hash != SourceHash::from_bytes(source.bytes()) {
            return Err(import_error(stale_continuity()));
        }
        settings
            .continuity
            .validate(raw.entries.len())
            .map_err(import_error)?;
        // Check each primary body before the only auxiliary read and before building cooked text.
        for row in &raw.entries {
            if row.text.chars().count() > settings.max_characters {
                return Err(import_error(too_long()));
            }
        }
        let suffix = source.read_dependency(&raw.suffix, byte_limit(MAX_BUNDLE_SUFFIX_BYTES))?;
        if suffix.len() > MAX_BUNDLE_SUFFIX_BYTES {
            return Err(import_error(invalid_suffix()));
        }
        let suffix = std::str::from_utf8(&suffix).map_err(|_| import_error(invalid_suffix()))?;
        let suffix_characters = suffix.chars().count();
        let mut cooked_bytes = 0_usize;
        for row in &raw.entries {
            if row.text.chars().count() + suffix_characters > settings.max_characters {
                return Err(import_error(too_long()));
            }
            cooked_bytes += row.text.len() + suffix.len();
        }
        if cooked_bytes > MAX_COOKED_TEXT_BYTES {
            return Err(import_error(too_long()));
        }
        let by_row: BTreeMap<_, _> = settings
            .continuity
            .assignments
            .iter()
            .map(|assignment| (assignment.row, assignment.product))
            .collect();
        let mut entries = BTreeMap::new();
        for (row, entry) in raw.entries.into_iter().enumerate() {
            let product = *by_row
                .get(&row)
                .ok_or_else(|| import_error(invalid_continuity()))?;
            let mut text = String::with_capacity(entry.text.len() + suffix.len());
            text.push_str(&entry.text);
            text.push_str(suffix);
            entries.insert(
                product,
                TextBundleEntry {
                    name: entry.name,
                    text,
                },
            );
        }
        Ok(TextBundle {
            source: source_id,
            entries,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBundle {
    suffix: AssetPath,
    #[serde(deserialize_with = "deserialize_source_entries")]
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    name: String,
    text: String,
}

fn decode_source(bytes: &[u8]) -> Result<RawBundle, TextBundleError> {
    if bytes.len() > MAX_BUNDLE_SOURCE_BYTES {
        return Err(invalid_source());
    }
    preflight_json(bytes, source_shape()).map_err(|_| invalid_source())?;
    let raw: RawBundle = serde_json::from_slice(bytes).map_err(|_| invalid_source())?;
    if raw.entries.is_empty() || raw.suffix.as_str().len() > MAX_PATH_BYTES {
        return Err(invalid_source());
    }
    for entry in &raw.entries {
        if entry.name.is_empty()
            || entry.name.len() > MAX_NAME_BYTES
            || entry.text.len() > MAX_ENTRY_TEXT_BYTES
        {
            return Err(invalid_source());
        }
    }
    Ok(raw)
}

fn validate_assignments(
    assignments: &[TextBundleRowAssignment],
    row_count: usize,
) -> Result<(), TextBundleError> {
    if row_count == 0 || row_count > MAX_BUNDLE_ENTRIES || assignments.len() != row_count {
        return Err(invalid_continuity());
    }
    let mut rows = BTreeSet::new();
    let mut products = BTreeSet::new();
    for assignment in assignments {
        if assignment.row >= row_count
            || assignment.product.as_subproduct().is_none()
            || !rows.insert(assignment.row)
            || !products.insert(assignment.product)
        {
            return Err(invalid_continuity());
        }
    }
    Ok(())
}

fn validate_maximum(maximum: usize) -> Result<(), TextBundleError> {
    if maximum == 0 || maximum > MAX_BUNDLE_CHARACTERS {
        return Err(invalid_settings());
    }
    Ok(())
}

// This private serde helper caps a sequence before requesting the next owned element. It is
// also used for metadata, whose candidate may have been admitted under caller-selected limits.
fn bounded_sequence<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Bounded<T, N> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a sequence with at most {N} items")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut input: A) -> Result<Vec<T>, A::Error> {
            let mut values = Vec::new();
            while values.len() < N {
                let Some(value) = input.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if input.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "text bundle sequence exceeds its item limit",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Bounded::<T, N>(std::marker::PhantomData))
}

fn deserialize_assignments<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<TextBundleRowAssignment>, D::Error> {
    bounded_sequence::<D, _, MAX_BUNDLE_ENTRIES>(deserializer)
}

fn deserialize_retired<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ImportedProductId>, D::Error> {
    bounded_sequence::<D, _, MAX_BUNDLE_RETIRED>(deserializer)
}

fn deserialize_source_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RawEntry>, D::Error> {
    bounded_sequence::<D, _, MAX_BUNDLE_ENTRIES>(deserializer)
}

fn source_shape() -> SerdeShapeLimits {
    shape(6, 128, 16, MAX_ENTRY_TEXT_BYTES, MAX_BUNDLE_SOURCE_BYTES)
}

fn runtime_shape() -> SerdeShapeLimits {
    shape(10, 512, 16, MAX_ENTRY_TEXT_BYTES, 96 * 1024)
}

fn shape(
    depth: usize,
    nodes: usize,
    items: usize,
    string: usize,
    strings: usize,
) -> SerdeShapeLimits {
    SerdeShapeLimits::new(
        DepthLimit::new(depth).expect("fixed nonzero depth limit"),
        ItemLimit::new(nodes).expect("fixed nonzero node limit"),
        ItemLimit::new(items).expect("fixed nonzero container limit"),
        byte_limit(string),
        byte_limit(strings),
    )
}

fn preflight_json(bytes: &[u8], limits: SerdeShapeLimits) -> Result<(), ()> {
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut parser, limits).map_err(|_| ())?;
    parser.end().map_err(|_| ())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBundle {
    source: StableAssetId,
    #[serde(deserialize_with = "deserialize_runtime_entries")]
    entries: Vec<RuntimeEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeEntry {
    reference: AssetProductRef,
    name: String,
    text: String,
}

fn deserialize_runtime_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RuntimeEntry>, D::Error> {
    bounded_sequence::<D, _, MAX_BUNDLE_ENTRIES>(deserializer)
}

impl RuntimeAssetCodec<TextBundle> for TextBundleDomain {
    const RUNTIME_KIND: &'static str = "consumer_text_bundle";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        // Includes the maximum serializer buffer and boxing copy, preflight/parser scratch and
        // temporary cooked strings/map. Retained output excludes encoded input owned by the Host.
        AssetRuntimeLimits::new(
            byte_limit(MAX_RUNTIME_BYTES),
            byte_limit(2 * 1024 * 1024),
            byte_limit(RETAINED_BYTES),
        )
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        validate_product(product).map_err(runtime_error)?;
        let entries = product
            .entries
            .iter()
            .map(|(id, entry)| {
                Ok(RuntimeEntry {
                    reference: AssetProductRef::new(product.source, *id)
                        .map_err(|_| invalid_runtime())?,
                    name: entry.name.clone(),
                    text: entry.text.clone(),
                })
            })
            .collect::<Result<Vec<_>, TextBundleError>>()
            .map_err(runtime_error)?;
        let envelope = PersistentFileEnvelope::canonical_v1(
            runtime_kind(),
            engine_version(),
            FormatGenerator::new("nara", engine_version()).expect("fixed valid generator"),
            RuntimeBundle {
                source: product.source,
                entries,
            },
        );
        let bytes = serde_json::to_vec(&envelope).map_err(|_| runtime_error(invalid_runtime()))?;
        if bytes.len() > MAX_RUNTIME_BYTES {
            return Err(runtime_error(invalid_runtime()));
        }
        preflight_json(&bytes, runtime_shape()).map_err(|_| runtime_error(invalid_runtime()))?;
        Ok(bytes.into_boxed_slice())
    }

    fn decode_runtime(bytes: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        let contract = PersistentFileContract::canonical_v1(runtime_kind(), engine_version());
        let envelope = decode_persistent_file_with_preflight(
            bytes,
            byte_limit(MAX_RUNTIME_BYTES),
            &contract,
            |bytes| preflight_json(bytes, runtime_shape()),
            |bytes| serde_json::from_slice::<PersistentFileHeader>(bytes).map_err(|_| ()),
            |bytes| {
                serde_json::from_slice::<PersistentFileEnvelope<RuntimeBundle>>(bytes)
                    .map_err(|_| ())
            },
        )
        .map_err(|_| runtime_error(invalid_runtime()))?;
        let runtime = envelope.into_payload();
        let mut entries = BTreeMap::new();
        let mut previous = None;
        for entry in runtime.entries {
            let id = entry.reference.product();
            if entry.reference.source() != runtime.source
                || id.as_subproduct().is_none()
                || previous.is_some_and(|previous| previous >= id)
            {
                return Err(runtime_error(invalid_runtime()));
            }
            previous = Some(id);
            if entries
                .insert(
                    id,
                    TextBundleEntry {
                        name: entry.name,
                        text: entry.text,
                    },
                )
                .is_some()
            {
                return Err(runtime_error(invalid_runtime()));
            }
        }
        let product = TextBundle {
            source: runtime.source,
            entries,
        };
        validate_product(&product).map_err(runtime_error)?;
        Ok(product)
    }
}

fn validate_product(product: &TextBundle) -> Result<(), TextBundleError> {
    if product.source.as_uuid().is_nil()
        || product.entries.is_empty()
        || product.entries.len() > MAX_BUNDLE_ENTRIES
    {
        return Err(invalid_runtime());
    }
    let mut total = 0_usize;
    for (id, entry) in &product.entries {
        if id.as_subproduct().is_none()
            || entry.name.is_empty()
            || entry.name.len() > MAX_NAME_BYTES
            || entry.text.len() > MAX_ENTRY_TEXT_BYTES
            || entry.text.chars().count() > MAX_BUNDLE_CHARACTERS
        {
            return Err(invalid_runtime());
        }
        total += entry.text.len();
    }
    if total > MAX_COOKED_TEXT_BYTES {
        return Err(invalid_runtime());
    }
    Ok(())
}

fn runtime_kind() -> FormatKind {
    FormatKind::new(<TextBundleDomain as RuntimeAssetCodec<TextBundle>>::RUNTIME_KIND)
        .expect("fixed valid Runtime kind")
}

fn engine_version() -> EngineVersion {
    EngineVersion::parse(ENGINE_VERSION).expect("fixed valid engine version")
}

fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).expect("fixed nonzero byte limit")
}

#[derive(Debug)]
pub struct TextBundleError(Diagnostic);

impl TextBundleError {
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }

    #[must_use]
    pub fn into_diagnostic(self) -> Diagnostic {
        self.0
    }
}

impl fmt::Display for TextBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the text bundle or its authored continuity was rejected")
    }
}

impl Error for TextBundleError {}

fn import_error<E>(error: TextBundleError) -> AssetImportError<E> {
    AssetImportError::domain(error.into_diagnostic())
}

fn runtime_error(error: TextBundleError) -> AssetRuntimeError {
    AssetRuntimeError::new(error.into_diagnostic())
}

fn error(code: &'static str, summary: &'static str) -> TextBundleError {
    TextBundleError(Diagnostic::error(
        DiagnosticCode::new(code).expect("fixed valid bundle diagnostic code"),
        SafeSummary::new(summary).expect("fixed safe bundle diagnostic summary"),
    ))
}

fn invalid_source() -> TextBundleError {
    error(
        "consumer.text-bundle.source-invalid",
        "Use bounded bundle JSON with a relative suffix path and one to eight named text entries",
    )
}

fn invalid_settings() -> TextBundleError {
    error(
        "consumer.text-bundle.settings-invalid",
        "Set max_characters between 1 and 4096 and persist complete authored bundle continuity",
    )
}

fn invalid_continuity() -> TextBundleError {
    error(
        "consumer.text-bundle.continuity-invalid",
        "Explicitly assign every bundle row a distinct non-primary product ID with no retired reuse",
    )
}

fn stale_continuity() -> TextBundleError {
    error(
        "consumer.text-bundle.continuity-stale",
        "Rebind every bundle row explicitly against these exact source bytes before importing",
    )
}

fn invalid_transition() -> TextBundleError {
    error(
        "consumer.text-bundle.transition-invalid",
        "Continue only active IDs, introduce fresh IDs explicitly, and preserve all known retired IDs",
    )
}

fn retirement_limit() -> TextBundleError {
    error(
        "consumer.text-bundle.retirement-limit",
        "The bounded bundle retirement record is full; resolve continuity explicitly before rebinding",
    )
}

fn invalid_suffix() -> TextBundleError {
    error(
        "consumer.text-bundle.suffix-invalid",
        "Use a UTF-8 suffix of at most 1024 bytes",
    )
}

fn too_long() -> TextBundleError {
    error(
        "consumer.text-bundle.too-long",
        "Shorten cooked bundle text or increase max_characters within the supported bound",
    )
}

fn invalid_runtime() -> TextBundleError {
    error(
        "consumer.text-bundle.runtime-invalid",
        "Use the supported cooked bundle envelope with bounded text and ordered unique product references",
    )
}
