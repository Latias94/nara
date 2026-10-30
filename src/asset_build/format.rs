//! Canonical version-one metadata and allocation admission before semantic Vec decoding.

use std::{
    cmp::Ordering,
    fmt,
    io::{self, Write},
    mem::size_of,
};

use nara_asset::{AssetProductRef, AssetReferenceDemand};
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    preflight_serde_shape,
};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor},
};

use super::{
    budget::{ArtifactBuildLease, add, check, mul},
    model::{
        ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildSeed, ArtifactContentDigest, ArtifactGroupCandidate, ArtifactGroupMember,
        ArtifactRecipeDigest, ArtifactSeedDigest,
    },
    store::ArtifactBuildStore,
};

const GROUP_KIND: &str = "local_artifact_group";
const INDEX_KIND: &str = "local_artifact_index";
const MAX_STRING_BYTES: usize = 4096;
const MAX_DEFINITION_BYTES: usize = 128;
const MAX_MEMBER_NAME_BYTES: usize = 128;
const MAX_DEPTH: usize = 32;
// A logical ownership model for serde Vec growth, tagged-enum buffering and duplicate-key sets.
// It is deliberately separate from encoded byte limits and from allocator measurements.
const NODE_WORK_BYTES: usize = 192;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupManifest {
    pub(super) seed: ArtifactBuildSeed,
    pub(super) observations: Vec<ArtifactBuildObservation>,
    pub(super) recipe: ArtifactRecipeDigest,
    pub(super) members: Vec<ArtifactGroupMember>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SeedHint {
    pub(super) seed: ArtifactSeedDigest,
    pub(super) recipe: ArtifactRecipeDigest,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecipeEntry {
    pub(super) recipe: ArtifactRecipeDigest,
    pub(super) group: ArtifactContentDigest,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoreIndex {
    pub(super) hints: Vec<SeedHint>,
    pub(super) recipes: Vec<RecipeEntry>,
}

impl StoreIndex {
    pub(super) fn associate(
        &mut self,
        seed: ArtifactSeedDigest,
        recipe: ArtifactRecipeDigest,
        group: ArtifactContentDigest,
        limits: ArtifactBuildLimits,
    ) -> Result<(), ArtifactBuildError> {
        let hint_position = self.hints.binary_search_by_key(&seed, |hint| hint.seed);
        let recipe_position = self
            .recipes
            .binary_search_by_key(&recipe, |entry| entry.recipe);
        check(
            "seed-hints",
            add(self.hints.len(), usize::from(hint_position.is_err()))?,
            limits.index_entries.get(),
        )?;
        check(
            "successful-recipes",
            add(self.recipes.len(), usize::from(recipe_position.is_err()))?,
            limits.index_entries.get(),
        )?;
        match recipe_position {
            Ok(index) if self.recipes[index].group != group => {
                return Err(ArtifactBuildError::DeterminismConflict);
            }
            Ok(_) => {}
            Err(index) => {
                self.recipes.reserve_exact(1);
                self.recipes.insert(index, RecipeEntry { recipe, group });
            }
        }
        match hint_position {
            Ok(index) => self.hints[index].recipe = recipe,
            Err(index) => {
                self.hints.reserve_exact(1);
                self.hints.insert(index, SeedHint { seed, recipe });
            }
        }
        Ok(())
    }
}

pub(super) struct Encoded {
    pub(super) bytes: Vec<u8>,
    pub(super) _lease: ArtifactBuildLease,
}

pub(super) struct Decoded<T> {
    pub(super) value: T,
    pub(super) lease: ArtifactBuildLease,
}

pub(super) fn seed_digest(
    seed: &ArtifactBuildSeed,
) -> Result<ArtifactSeedDigest, ArtifactBuildError> {
    Ok(ArtifactSeedDigest(hash_serialized(
        b"nara.artifact.seed.v1\0",
        seed,
    )?))
}

pub(super) fn recipe_digest(
    seed: &ArtifactBuildSeed,
    observations: &[ArtifactBuildObservation],
) -> Result<ArtifactRecipeDigest, ArtifactBuildError> {
    #[derive(Serialize)]
    struct ClosedRecipe<'a> {
        seed: &'a ArtifactBuildSeed,
        observations: &'a [ArtifactBuildObservation],
    }
    Ok(ArtifactRecipeDigest(hash_serialized(
        b"nara.artifact.recipe.v1\0",
        &ClosedRecipe { seed, observations },
    )?))
}

fn hash_serialized(
    value_domain: &[u8],
    value: &impl Serialize,
) -> Result<[u8; 32], ArtifactBuildError> {
    struct HashWriter(blake3::Hasher);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(blake3::Hasher::new());
    writer.0.update(value_domain);
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        ArtifactBuildError::InvalidCandidate("build facts cannot be canonically encoded")
    })?;
    Ok(*writer.0.finalize().as_bytes())
}

pub(super) fn validate_seed(
    seed: &ArtifactBuildSeed,
    limits: ArtifactBuildLimits,
) -> Result<(), ArtifactBuildError> {
    validate_product(seed.source)?;
    if let Some(path) = &seed.location {
        validate_string(path.as_str(), MAX_STRING_BYTES)?;
    }
    if seed.definitions.is_empty() {
        return invalid("a build seed needs an explicit implementation definition");
    }
    check("definitions", seed.definitions.len(), limits.members.get())?;
    for (index, definition) in seed.definitions.iter().enumerate() {
        validate_string(&definition.id, MAX_DEFINITION_BYTES)?;
        if !definition.id.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/' | b'@')
        }) {
            return invalid("implementation IDs require bounded ASCII identifiers");
        }
        if seed.definitions[..index]
            .iter()
            .any(|previous| previous.id == definition.id)
        {
            return invalid("implementation definitions have duplicate IDs");
        }
    }
    check(
        "settings-and-target",
        add(seed.settings.len(), seed.target.len())?,
        limits.metadata_bytes.get(),
    )?;
    Ok(())
}

pub(super) fn candidate_work(
    candidate: &ArtifactGroupCandidate,
    limits: ArtifactBuildLimits,
) -> Result<usize, ArtifactBuildError> {
    validate_seed(&candidate.seed, limits)?;
    check("members", candidate.outputs.len(), limits.members.get())?;
    check(
        "observations",
        candidate.observations.len(),
        limits.observations.get(),
    )?;
    if candidate.outputs.is_empty() {
        return invalid("a successful artifact group must contain a member");
    }
    let mut total = 0;
    let mut products = 0;
    let mut dependencies = 0;
    for output in &candidate.outputs {
        validate_name(&output.name)?;
        check(
            "member-bytes",
            output.bytes.len(),
            limits.member_bytes.get(),
        )?;
        total = add(total, output.bytes.len())?;
        products = add(products, output.products.len())?;
        dependencies = add(dependencies, output.dependencies.len())?;
    }
    check("group-bytes", total, limits.group_bytes.get())?;
    check("products", products, limits.members.get())?;
    check(
        "runtime-dependencies",
        dependencies,
        limits.observations.get(),
    )?;
    // New member descriptors and byte owners coexist with the moved output Vec until conversion
    // completes. Product scratch proves global uniqueness without a quadratic member scan.
    add(
        mul(
            candidate.outputs.len(),
            size_of::<ArtifactGroupMember>() + size_of::<std::sync::Arc<[u8]>>(),
        )?,
        mul(products, size_of::<AssetProductRef>())?,
    )
}

pub(super) fn canonicalize_candidate(
    candidate: &mut ArtifactGroupCandidate,
    limits: ArtifactBuildLimits,
) -> Result<(), ArtifactBuildError> {
    candidate.observations.sort_unstable_by(observation_order);
    for pair in candidate.observations.windows(2) {
        if observation_order(&pair[0], &pair[1]) == Ordering::Equal && pair[0] != pair[1] {
            return invalid("one tracked input has conflicting observations");
        }
    }
    candidate.observations.dedup();
    for observation in &candidate.observations {
        validate_observation(observation)?;
    }
    candidate
        .outputs
        .sort_unstable_by(|left, right| left.name.cmp(&right.name));
    if candidate
        .outputs
        .windows(2)
        .any(|pair| pair[0].name == pair[1].name)
    {
        return invalid("group member names are duplicated");
    }
    let product_count = candidate
        .outputs
        .iter()
        .map(|output| output.products.len())
        .sum();
    let mut products = Vec::with_capacity(product_count);
    for output in &mut candidate.outputs {
        output.products.sort_unstable();
        output
            .dependencies
            .sort_unstable_by_key(|(product, _)| *product);
        validate_mappings(&output.products, &output.dependencies)?;
        products.extend_from_slice(&output.products);
    }
    validate_complete_products(&mut products, candidate.seed.source)?;
    // Counting encoding runs before any manifest/index buffer allocation, including seeds whose
    // individual strings are legal but whose aggregate metadata exceeds the format limit.
    check(
        "observations",
        candidate.observations.len(),
        limits.observations.get(),
    )
}

fn validate_product(product: AssetProductRef) -> Result<(), ArtifactBuildError> {
    if product.source().as_uuid().is_nil()
        || product
            .product()
            .as_subproduct()
            .is_some_and(|id| id.is_nil())
    {
        return invalid("Product identity must have a non-nil source and valid product ID");
    }
    Ok(())
}

fn validate_string(value: &str, maximum: usize) -> Result<(), ArtifactBuildError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return invalid(
            "metadata strings must be nonempty, bounded and free of control characters",
        );
    }
    Ok(())
}

fn validate_name(value: &str) -> Result<(), ArtifactBuildError> {
    validate_string(value, MAX_MEMBER_NAME_BYTES)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(value, "." | "..")
    {
        return invalid("member names require portable ASCII labels without paths");
    }
    Ok(())
}

fn validate_observation(observation: &ArtifactBuildObservation) -> Result<(), ArtifactBuildError> {
    match observation {
        ArtifactBuildObservation::Raw { path, .. } => {
            validate_string(path.as_str(), MAX_STRING_BYTES)
        }
        ArtifactBuildObservation::Processed { product, .. } => validate_product(*product),
    }
}

fn observation_order(
    left: &ArtifactBuildObservation,
    right: &ArtifactBuildObservation,
) -> Ordering {
    use ArtifactBuildObservation::{Processed, Raw};
    match (left, right) {
        (Raw { path: left, .. }, Raw { path: right, .. }) => left.cmp(right),
        (Processed { product: left, .. }, Processed { product: right, .. }) => left.cmp(right),
        (Raw { .. }, Processed { .. }) => Ordering::Less,
        (Processed { .. }, Raw { .. }) => Ordering::Greater,
    }
}

fn validate_mappings(
    products: &[AssetProductRef],
    dependencies: &[(AssetProductRef, AssetReferenceDemand)],
) -> Result<(), ArtifactBuildError> {
    for product in products {
        validate_product(*product)?;
    }
    for (product, _) in dependencies {
        validate_product(*product)?;
    }
    if products.windows(2).any(|pair| pair[0] >= pair[1]) {
        return invalid("member Product mappings must be unique and canonical");
    }
    if dependencies.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return invalid("Runtime references must have one canonical demand per Product");
    }
    Ok(())
}

fn validate_complete_products(
    products: &mut [AssetProductRef],
    source: AssetProductRef,
) -> Result<(), ArtifactBuildError> {
    products.sort_unstable();
    if products.windows(2).any(|pair| pair[0] == pair[1]) {
        return invalid("one Product is mapped by multiple group members");
    }
    if products.binary_search(&source).is_err() {
        return invalid("the build seed Product is absent from the complete output group");
    }
    Ok(())
}

pub(super) fn manifest_allocation(manifest: &GroupManifest) -> Result<usize, ArtifactBuildError> {
    let seed = &manifest.seed;
    let mut total = mul(
        seed.definitions.capacity(),
        size_of::<ArtifactBuildDefinition>(),
    )?;
    for definition in &seed.definitions {
        total = add(total, definition.id.capacity())?;
    }
    total = add(
        total,
        seed.location.as_ref().map_or(0, |path| path.as_str().len()),
    )?;
    total = add(
        total,
        add(seed.settings.capacity(), seed.target.capacity())?,
    )?;
    total = add(total, seed.format.as_str().len())?;
    total = add(
        total,
        mul(
            manifest.observations.capacity(),
            size_of::<ArtifactBuildObservation>(),
        )?,
    )?;
    for observation in &manifest.observations {
        total = add(
            total,
            match observation {
                ArtifactBuildObservation::Raw { path, .. } => path.as_str().len(),
                ArtifactBuildObservation::Processed { format, .. } => format.as_str().len(),
            },
        )?;
    }
    total = add(
        total,
        mul(
            manifest.members.capacity(),
            size_of::<ArtifactGroupMember>(),
        )?,
    )?;
    for member in &manifest.members {
        total = add(
            total,
            add(member.name.capacity(), member.format.as_str().len())?,
        )?;
        total = add(
            total,
            mul(member.products.capacity(), size_of::<AssetProductRef>())?,
        )?;
        total = add(
            total,
            mul(
                member.dependencies.capacity(),
                size_of::<(AssetProductRef, AssetReferenceDemand)>(),
            )?,
        )?;
    }
    Ok(total)
}

pub(super) fn index_growth_work(index: &StoreIndex) -> Result<usize, ArtifactBuildError> {
    add(
        mul(add(index.hints.len(), 1)?, size_of::<SeedHint>())?,
        mul(add(index.recipes.len(), 1)?, size_of::<RecipeEntry>())?,
    )
}

pub(super) fn encode_group(
    value: &GroupManifest,
    store: &ArtifactBuildStore,
) -> Result<Encoded, ArtifactBuildError> {
    encode(value, GROUP_KIND, store)
}

pub(super) fn encode_index(
    value: &StoreIndex,
    store: &ArtifactBuildStore,
) -> Result<Encoded, ArtifactBuildError> {
    encode(value, INDEX_KIND, store)
}

fn encode(
    value: &impl Serialize,
    kind: &str,
    store: &ArtifactBuildStore,
) -> Result<Encoded, ArtifactBuildError> {
    let version = engine_version()?;
    let envelope = PersistentFileEnvelope::canonical_v1(
        format_kind(kind)?,
        version.clone(),
        FormatGenerator::new("nara", version)
            .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid generator"))?,
        value,
    );
    let maximum = store.limits().metadata_bytes.get();
    let mut count = CountWriter {
        length: 0,
        maximum,
        failure: None,
    };
    if serde_json::to_writer(&mut count, &envelope).is_err() {
        return Err(count
            .failure
            .unwrap_or(ArtifactBuildError::InvalidCandidate(
                "metadata cannot be encoded",
            )));
    }
    let lease = store.reserve_work(count.length)?;
    let mut bytes = Vec::with_capacity(count.length);
    serde_json::to_writer(&mut bytes, &envelope)
        .map_err(|_| ArtifactBuildError::InvalidCandidate("metadata cannot be encoded"))?;
    Ok(Encoded {
        bytes,
        _lease: lease,
    })
}

pub(super) fn decode_group(
    bytes: &[u8],
    store: &ArtifactBuildStore,
) -> Result<Decoded<GroupManifest>, ArtifactBuildError> {
    let decoded: Decoded<GroupManifest> = decode(bytes, GROUP_KIND, store)?;
    validate_manifest(&decoded.value, store.limits()).map_err(cache_error)?;
    Ok(decoded)
}

pub(super) fn decode_index(
    bytes: &[u8],
    store: &ArtifactBuildStore,
) -> Result<Decoded<StoreIndex>, ArtifactBuildError> {
    let decoded: Decoded<StoreIndex> = decode(bytes, INDEX_KIND, store)?;
    let index = &decoded.value;
    if index
        .hints
        .windows(2)
        .any(|pair| pair[0].seed >= pair[1].seed)
        || index
            .recipes
            .windows(2)
            .any(|pair| pair[0].recipe >= pair[1].recipe)
        || index.hints.iter().any(|hint| {
            index
                .recipes
                .binary_search_by_key(&hint.recipe, |entry| entry.recipe)
                .is_err()
        })
    {
        return Err(ArtifactBuildError::InvalidCache(
            "index associations are duplicated, unordered or dangling",
        ));
    }
    Ok(decoded)
}

fn decode<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    kind: &str,
    store: &ArtifactBuildStore,
) -> Result<Decoded<T>, ArtifactBuildError> {
    check(
        "encoded-metadata",
        bytes.len(),
        store.limits().metadata_bytes.get(),
    )
    .map_err(cache_error)?;
    // JSON's escaped-string scratch can allocate even during a structural walk. Admit it before
    // parsing. The walk itself constructs no domain Vec or owned map-key set.
    let _scratch = store.reserve_work(mul(bytes.len(), 2)?)?;
    let mut state = PreflightState {
        limits: store.limits(),
        nodes: 0,
        strings: 0,
        work: 0,
        counts: Counts::default(),
        failure: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = ShapeSeed {
        state: &mut state,
        location: Location::Envelope,
        depth: 1,
    }
    .deserialize(&mut deserializer);
    if let Some(error) = state.failure {
        return Err(error);
    }
    result.map_err(|_| ArtifactBuildError::InvalidCache("invalid metadata shape"))?;
    deserializer
        .end()
        .map_err(|_| ArtifactBuildError::InvalidCache("trailing metadata"))?;
    let lease = store.reserve_work(state.work)?;
    let shape = SerdeShapeLimits::new(
        DepthLimit::new(MAX_DEPTH).ok_or(ArtifactBuildError::Overflow)?,
        ItemLimit::new(bytes.len().max(1)).ok_or(ArtifactBuildError::Overflow)?,
        ItemLimit::new(bytes.len().max(1)).ok_or(ArtifactBuildError::Overflow)?,
        ByteLimit::new(MAX_STRING_BYTES).ok_or(ArtifactBuildError::Overflow)?,
        store.limits().metadata_bytes,
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, shape).map_err(|_| {
        ArtifactBuildError::InvalidCache(
            "metadata structure, strings or duplicate keys are invalid",
        )
    })?;
    deserializer
        .end()
        .map_err(|_| ArtifactBuildError::InvalidCache("trailing metadata"))?;
    let header: PersistentFileHeader = serde_json::from_slice(bytes)
        .map_err(|_| ArtifactBuildError::InvalidCache("invalid metadata header"))?;
    PersistentFileContract::canonical_v1(format_kind(kind)?, engine_version()?)
        .validate_header(&header)
        .map_err(|_| ArtifactBuildError::InvalidCache("metadata envelope contract mismatch"))?;
    let envelope: PersistentFileEnvelope<T> = serde_json::from_slice(bytes)
        .map_err(|_| ArtifactBuildError::InvalidCache("invalid metadata payload"))?;
    // Compare a streamed re-encoding without allocating another metadata buffer. This also
    // rejects alternate field order, whitespace, positional records and noncanonical strings.
    let mut comparison = CompareWriter {
        expected: bytes,
        offset: 0,
    };
    serde_json::to_writer(&mut comparison, &envelope)
        .map_err(|_| ArtifactBuildError::InvalidCache("metadata is not canonical version one"))?;
    if comparison.offset != bytes.len() {
        return Err(ArtifactBuildError::InvalidCache(
            "metadata is not canonical version one",
        ));
    }
    Ok(Decoded {
        value: envelope.into_payload(),
        lease,
    })
}

fn validate_manifest(
    manifest: &GroupManifest,
    limits: ArtifactBuildLimits,
) -> Result<(), ArtifactBuildError> {
    validate_seed(&manifest.seed, limits)?;
    if manifest.members.is_empty()
        || manifest
            .members
            .windows(2)
            .any(|pair| pair[0].name >= pair[1].name)
    {
        return invalid("complete member names must be nonempty, unique and canonical");
    }
    if manifest
        .observations
        .windows(2)
        .any(|pair| observation_order(&pair[0], &pair[1]) != Ordering::Less)
    {
        return invalid("tracked observations must be unique and canonical");
    }
    for observation in &manifest.observations {
        validate_observation(observation)?;
    }
    if manifest.seed.recipe_digest(&manifest.observations)? != manifest.recipe {
        return invalid("closed recipe digest mismatch");
    }
    let mut total_bytes = 0;
    // The raw preflight has already admitted this scratch vector before semantic construction.
    let mut products = Vec::new();
    let count = manifest
        .members
        .iter()
        .map(|member| member.products.len())
        .sum();
    products.reserve_exact(count);
    for member in &manifest.members {
        validate_name(&member.name)?;
        validate_mappings(&member.products, &member.dependencies)?;
        let bytes =
            usize::try_from(member.digest.length()).map_err(|_| ArtifactBuildError::Overflow)?;
        check("member-bytes", bytes, limits.member_bytes.get())?;
        total_bytes = add(total_bytes, bytes)?;
        products.extend_from_slice(&member.products);
    }
    check("group-bytes", total_bytes, limits.group_bytes.get())?;
    validate_complete_products(&mut products, manifest.seed.source)
}

fn engine_version() -> Result<EngineVersion, ArtifactBuildError> {
    EngineVersion::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid engine version"))
}

fn format_kind(kind: &str) -> Result<FormatKind, ArtifactBuildError> {
    FormatKind::new(kind)
        .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid metadata format kind"))
}

fn invalid<T>(reason: &'static str) -> Result<T, ArtifactBuildError> {
    Err(ArtifactBuildError::InvalidCandidate(reason))
}
fn cache_error(error: ArtifactBuildError) -> ArtifactBuildError {
    match error {
        ArtifactBuildError::InvalidCandidate(reason) => ArtifactBuildError::InvalidCache(reason),
        ArtifactBuildError::Budget { .. } => {
            ArtifactBuildError::InvalidCache("metadata exceeds its structural bounds")
        }
        error => error,
    }
}

struct CountWriter {
    length: usize,
    maximum: usize,
    failure: Option<ArtifactBuildError>,
}
impl Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = add(self.length, bytes.len()).and_then(|next| {
            check("encoded-metadata", next, self.maximum)?;
            Ok(next)
        });
        match result {
            Ok(next) => {
                self.length = next;
                Ok(bytes.len())
            }
            Err(error) => {
                self.failure = Some(error);
                Err(io::Error::other("metadata encoded byte bound exceeded"))
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CompareWriter<'a> {
    expected: &'a [u8],
    offset: usize,
}
impl Write for CompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("canonical offset overflow"))?;
        if self.expected.get(self.offset..end) != Some(bytes) {
            return Err(io::Error::other("noncanonical metadata"));
        }
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct Counts {
    definitions: usize,
    observations: usize,
    members: usize,
    products: usize,
    dependencies: usize,
    hints: usize,
    recipes: usize,
    octets: usize,
}

#[derive(Clone, Copy)]
enum Location {
    Envelope,
    Payload,
    Seed,
    Definitions,
    Definition,
    Observations,
    Observation,
    Members,
    Member,
    Products,
    Product,
    Dependencies,
    Dependency,
    Hints,
    Hint,
    Recipes,
    Recipe,
    Octets,
    Octet,
    Other,
}

impl Location {
    fn field(self, key: &str) -> Self {
        match (self, key) {
            (Self::Envelope, "payload") => Self::Payload,
            (Self::Payload, "seed") => Self::Seed,
            (Self::Seed, "definitions") => Self::Definitions,
            (Self::Seed, "settings" | "target") => Self::Octets,
            (Self::Payload, "observations") => Self::Observations,
            (Self::Payload, "members") => Self::Members,
            (Self::Payload, "hints") => Self::Hints,
            (Self::Payload, "recipes") => Self::Recipes,
            (Self::Member, "products") => Self::Products,
            (Self::Member, "dependencies") => Self::Dependencies,
            _ => Self::Other,
        }
    }
    fn element(self) -> Self {
        match self {
            Self::Definitions => Self::Definition,
            Self::Observations => Self::Observation,
            Self::Members => Self::Member,
            Self::Products => Self::Product,
            Self::Dependencies => Self::Dependency,
            Self::Hints => Self::Hint,
            Self::Recipes => Self::Recipe,
            Self::Octets => Self::Octet,
            _ => Self::Other,
        }
    }
}

struct PreflightState {
    limits: ArtifactBuildLimits,
    nodes: usize,
    strings: usize,
    work: usize,
    counts: Counts,
    failure: Option<ArtifactBuildError>,
}

impl PreflightState {
    fn enforce<E: de::Error>(&mut self, result: Result<(), ArtifactBuildError>) -> Result<(), E> {
        result.map_err(|error| {
            self.failure = Some(error);
            E::custom("artifact metadata pre-allocation admission refused")
        })
    }
    fn node<E: de::Error>(&mut self, location: Location, depth: usize) -> Result<(), E> {
        let result = (|| {
            check("wire-depth", depth, MAX_DEPTH).map_err(cache_error)?;
            self.nodes = add(self.nodes, 1)?;
            check("wire-nodes", self.nodes, self.limits.metadata_bytes.get())
                .map_err(cache_error)?;
            let counter = match location {
                Location::Definition => {
                    Some((&mut self.counts.definitions, self.limits.members.get()))
                }
                Location::Observation => Some((
                    &mut self.counts.observations,
                    self.limits.observations.get(),
                )),
                Location::Member => Some((&mut self.counts.members, self.limits.members.get())),
                Location::Product => Some((&mut self.counts.products, self.limits.members.get())),
                Location::Dependency => Some((
                    &mut self.counts.dependencies,
                    self.limits.observations.get(),
                )),
                Location::Hint => Some((&mut self.counts.hints, self.limits.index_entries.get())),
                Location::Recipe => {
                    Some((&mut self.counts.recipes, self.limits.index_entries.get()))
                }
                Location::Octet => {
                    Some((&mut self.counts.octets, self.limits.metadata_bytes.get()))
                }
                _ => None,
            };
            if let Some((counter, maximum)) = counter {
                *counter = add(*counter, 1)?;
                check("wire-items", *counter, maximum).map_err(cache_error)?;
            }
            self.work = add(self.work, NODE_WORK_BYTES)?;
            check("work", self.work, self.limits.work_bytes.get())
        })();
        self.enforce(result)
    }
    fn string<E: de::Error>(&mut self, value: &str) -> Result<(), E> {
        let result = (|| {
            check("wire-string", value.len(), MAX_STRING_BYTES).map_err(cache_error)?;
            self.strings = add(self.strings, value.len())?;
            check(
                "wire-strings",
                self.strings,
                self.limits.metadata_bytes.get(),
            )
            .map_err(cache_error)?;
            self.work = add(self.work, mul(value.len(), 2)?)?;
            check("work", self.work, self.limits.work_bytes.get())
        })();
        self.enforce(result)
    }
}

struct ShapeSeed<'a> {
    state: &'a mut PreflightState,
    location: Location,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for ShapeSeed<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        self.state.node(self.location, self.depth)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for ShapeSeed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded artifact JSON")
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.state.string(value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        if matches!(
            self.location,
            Location::Envelope
                | Location::Payload
                | Location::Seed
                | Location::Definition
                | Location::Observation
                | Location::Member
                | Location::Product
                | Location::Hint
                | Location::Recipe
        ) {
            return Err(de::Error::custom("artifact records require objects"));
        }
        while sequence
            .next_element_seed(ShapeSeed {
                state: self.state,
                location: self.location.element(),
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(location) = map.next_key_seed(KeySeed {
            state: self.state,
            location: self.location,
            depth: self.depth + 1,
        })? {
            map.next_value_seed(ShapeSeed {
                state: self.state,
                location,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

struct KeySeed<'a> {
    state: &'a mut PreflightState,
    location: Location,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = Location;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Location, D::Error> {
        self.state.node(Location::Other, self.depth)?;
        deserializer.deserialize_str(self)
    }
}
impl Visitor<'_> for KeySeed<'_> {
    type Value = Location;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded artifact field name")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Location, E> {
        self.state.string(value)?;
        Ok(self.location.field(value))
    }
}
