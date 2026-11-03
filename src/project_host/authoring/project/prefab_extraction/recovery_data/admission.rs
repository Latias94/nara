//! Accounting and serde admission for the concrete extraction package, not source semantics.

use std::io::{self, Write};

use nara_core::{
    EngineVersion, FormatGenerator, FormatKind, PersistentFileContract, PersistentFileEnvelope,
    PersistentFileHeader, SerdeShapePreflightError, preflight_serde_shape,
};
use nara_project::ProjectManifest;

use super::*;

pub(super) fn checked_add(
    left: usize,
    right: usize,
) -> Result<usize, PrefabExtractionRecoveryError> {
    left.checked_add(right)
        .ok_or(PrefabExtractionRecoveryError::AccountingOverflow)
}

pub(super) fn checked_mul(
    left: usize,
    right: usize,
) -> Result<usize, PrefabExtractionRecoveryError> {
    left.checked_mul(right)
        .ok_or(PrefabExtractionRecoveryError::AccountingOverflow)
}

pub(super) fn digest(bytes: &[u8]) -> ProjectAuthoringRecoveryDigest {
    let digest = ContentDigest::of_bytes(bytes);
    ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes())
}

pub(super) fn check_encoded(
    length: usize,
    limits: PrefabExtractionRecoveryLimits,
) -> Result<(), PrefabExtractionRecoveryError> {
    if length > limits.encoded_bytes.get() {
        return Err(PrefabExtractionRecoveryError::EncodedBytesExceeded);
    }
    Ok(())
}

pub(super) fn check_counts(
    originals: usize,
    metadata: usize,
    roots: usize,
    missing: usize,
    limits: PrefabExtractionRecoveryLimits,
) -> Result<(), PrefabExtractionRecoveryError> {
    // Q and its sidecar are single required roles; this ceiling bounds the original inventory.
    if originals == 0 || originals > limits.sources.get() {
        return Err(PrefabExtractionRecoveryError::SourceCountExceeded);
    }
    if checked_add(checked_add(metadata, roots)?, missing)? > limits.observations.get() {
        return Err(PrefabExtractionRecoveryError::ObservationCountExceeded);
    }
    Ok(())
}

pub(super) fn aggregate(payload: &RecoveryPayload) -> Result<usize, PrefabExtractionRecoveryError> {
    payload
        .blobs()
        .try_fold(0, |bytes, blob| checked_add(bytes, blob.bytes.len()))
}

pub(super) fn check_aggregate(
    bytes: usize,
    limits: PrefabExtractionRecoveryLimits,
) -> Result<(), PrefabExtractionRecoveryError> {
    if bytes > limits.aggregate_bytes.get() {
        return Err(PrefabExtractionRecoveryError::AggregateBytesExceeded);
    }
    Ok(())
}

pub(super) fn admit_input(
    input: &PrefabExtractionRecoveryInput<'_>,
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), PrefabExtractionRecoveryError> {
    check_counts(
        input.complete_original_sources.len(),
        input.metadata_guards.len(),
        input.roots.len(),
        input.missing_sidecars.len(),
        limits,
    )?;
    let entries = [
        input.complete_original_sources.len(),
        input.metadata_guards.len(),
        input.roots.len(),
        input.missing_sidecars.len(),
        5,
    ]
    .into_iter()
    .try_fold(0, checked_add)?;
    budget.charge_source_bytes(0, 0, checked_mul(entries, 8)?, 0)?;
    let mut aggregate = [
        input.old_ready_manifest.len(),
        input.final_ready_manifest.len(),
        input.unchanged_bundle.blob.len(),
        input.q.output.len(),
        input.q_sidecar.output.len(),
    ]
    .into_iter()
    .try_fold(0, checked_add)?;
    let mut retained = size_of::<PrefabExtractionRecovery>();
    let mut work = 0;
    for bytes in [
        input.unchanged_bundle.path.len(),
        input.q.path.len(),
        input.q_sidecar.path.len(),
        input.selected_profile.map_or(0, str::len),
        input.selection.root.as_str().len(),
        input.selection.source.retained_bytes(),
        input.q.key.retained_bytes(),
    ] {
        retained = checked_add(retained, bytes)?;
        work = checked_add(work, bytes)?;
    }
    let mut source_paths = 0;
    for source in input.complete_original_sources {
        let blobs = checked_add(
            checked_add(source.original_disk.len(), source.reviewed_working.len())?,
            source.output.map_or(0, <[u8]>::len),
        )?;
        aggregate = checked_add(aggregate, blobs)?;
        retained = checked_add(retained, size_of::<RecoveryOriginalSource>())?;
        retained = checked_add(retained, source.path.len())?;
        retained = checked_add(retained, source.key.retained_bytes())?;
        source_paths = checked_add(source_paths, source.path.len())?;
        work = checked_add(work, source.key.retained_bytes())?;
    }
    let mut metadata_paths = 0;
    for guard in input.metadata_guards {
        aggregate = checked_add(aggregate, guard.observed.len())?;
        retained = checked_add(retained, size_of::<RecoveryMetadataGuard>())?;
        retained = checked_add(retained, guard.path.len())?;
        metadata_paths = checked_add(metadata_paths, guard.path.len())?;
    }
    let mut root_paths = 0;
    for root in input.roots {
        retained = checked_add(retained, size_of::<RecoveryRoot>())?;
        retained = checked_add(retained, root.path.len())?;
        retained = checked_add(retained, root.missing_ancestor.map_or(0, str::len))?;
        root_paths = checked_add(root_paths, root.path.len())?;
        work = checked_add(work, root.missing_ancestor.map_or(0, str::len))?;
    }
    let mut missing_paths = 0;
    for path in input.missing_sidecars {
        retained = checked_add(retained, checked_add(size_of::<String>(), path.len())?)?;
        missing_paths = checked_add(missing_paths, path.len())?;
    }
    check_aggregate(aggregate, limits)?;
    retained = checked_add(retained, aggregate)?;
    work = checked_add(work, checked_mul(aggregate, 3)?)?;
    for (count, bytes) in [
        (input.complete_original_sources.len(), source_paths),
        (input.metadata_guards.len(), metadata_paths),
        (input.roots.len(), root_paths),
        (input.missing_sidecars.len(), missing_paths),
    ] {
        // Path copying and O(n log n) string comparison work precede the first allocation.
        work = checked_add(work, ordering_work(count, bytes)?)?;
    }
    budget.charge_source_bytes(0, 0, work, retained)?;
    Ok(())
}

pub(super) fn ordering_work(
    count: usize,
    bytes: usize,
) -> Result<usize, PrefabExtractionRecoveryError> {
    let comparisons = checked_add((usize::BITS - count.leading_zeros()) as usize, 2)?;
    checked_mul(checked_add(bytes, count)?, comparisons)
}

/// Models the actual decoded allocation, including serde Vec growth. Blob contents are retained
/// once in this AST and once escaped in the immutable package; there is no archive or hidden copy.
pub(super) fn retained_payload(
    payload: &RecoveryPayload,
) -> Result<usize, PrefabExtractionRecoveryError> {
    let mut bytes = size_of::<PrefabExtractionRecovery>();
    for blob in payload.blobs() {
        bytes = checked_add(bytes, blob.bytes.capacity())?;
    }
    for length in [
        payload.unchanged_bundle.path.capacity(),
        payload.q.path.capacity(),
        payload.q_sidecar.path.capacity(),
        payload
            .selected_profile
            .as_ref()
            .map_or(0, String::capacity),
        payload.selection.root.as_str().len(),
        payload.selection.source.retained_bytes(),
        payload.q.key.retained_bytes(),
        checked_mul(
            payload.complete_original_sources.capacity(),
            size_of::<RecoveryOriginalSource>(),
        )?,
        checked_mul(
            payload.metadata_guards.capacity(),
            size_of::<RecoveryMetadataGuard>(),
        )?,
        checked_mul(payload.roots.capacity(), size_of::<RecoveryRoot>())?,
        checked_mul(payload.missing_sidecars.capacity(), size_of::<String>())?,
    ] {
        bytes = checked_add(bytes, length)?;
    }
    for source in &payload.complete_original_sources {
        bytes = checked_add(bytes, source.path.capacity())?;
        bytes = checked_add(bytes, source.key.retained_bytes())?;
    }
    for guard in &payload.metadata_guards {
        bytes = checked_add(bytes, guard.path.capacity())?;
    }
    for root in &payload.roots {
        bytes = checked_add(bytes, root.path.capacity())?;
        if let RecoveryRootObservation::Missing { ancestor } = &root.observation {
            bytes = checked_add(bytes, ancestor.capacity())?;
        }
    }
    for path in &payload.missing_sidecars {
        bytes = checked_add(bytes, path.capacity())?;
    }
    Ok(bytes)
}

pub(super) fn decode_payload(
    encoded: &[u8],
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RecoveryPayload, PrefabExtractionRecoveryError> {
    check_encoded(encoded.len(), limits)?;
    let (reservation, shape) = package_reservation(encoded.len(), limits.shape, budget)?;
    budget.charge_source_bytes(
        encoded.len(),
        0,
        checked_mul(encoded.len(), 4)?,
        reservation,
    )?;
    preflight(encoded, shape)?;
    preflight_counts(encoded, limits)?;
    {
        let header: PersistentFileHeader =
            serde_json::from_slice(encoded).map_err(|_| PrefabExtractionRecoveryError::Syntax)?;
        PersistentFileContract::canonical_v1(format_kind()?, engine_version()?)
            .validate_header(&header)
            .map_err(PrefabExtractionRecoveryError::Contract)?;
    }
    let envelope: PersistentFileEnvelope<RecoveryPayload> =
        serde_json::from_slice(encoded).map_err(|_| PrefabExtractionRecoveryError::Syntax)?;
    let payload = envelope.into_payload();
    let retained = retained_payload(&payload)?;
    // No refund while the returned AST still lives. Transition the maximum parser reservation
    // directly to its owned payload; all other header/parser locals have already been dropped.
    budget
        .reconcile_retained_reservation(reservation, retained)
        .map_err(|_| PrefabExtractionRecoveryError::AccountingOverflow)?;
    Ok(payload)
}

pub(super) fn encode(
    payload: &RecoveryPayload,
    limits: PrefabExtractionRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<u8>, PrefabExtractionRecoveryError> {
    with_temporary(OWNER_FIXED_BYTES, budget, |budget| {
        // Counting and final serialization each traverse stored data, keys and control twice
        // at most in this finite envelope. Buffer writes, shape and digest are charged separately.
        budget.charge_source_bytes(0, 0, checked_mul(retained_payload(payload)?, 4)?, 0)?;
        let envelope = envelope(payload)?;
        let mut counter = CountingWriter {
            written: 0,
            maximum: limits.encoded_bytes.get(),
        };
        serde_json::to_writer_pretty(&mut counter, &envelope)
            .map_err(|_| PrefabExtractionRecoveryError::EncodedBytesExceeded)?;
        let length = counter.written;
        // Reserve the complete final buffer before Vec grows, including JSON escaping.
        budget.charge_source_bytes(length, 0, checked_mul(length, 2)?, length)?;
        let mut writer = PackageWriter {
            bytes: Vec::with_capacity(length),
            maximum: length,
        };
        serde_json::to_writer_pretty(&mut writer, &envelope)
            .map_err(|_| PrefabExtractionRecoveryError::EncodedBytesExceeded)?;
        let (reservation, shape) = package_reservation(length, limits.shape, budget)?;
        budget.charge_source_bytes(0, 0, length, 0)?;
        with_temporary(reservation, budget, |_| preflight(&writer.bytes, shape))?;
        Ok(writer.bytes)
    })
}

pub(super) fn envelope<T>(
    payload: T,
) -> Result<PersistentFileEnvelope<T>, PrefabExtractionRecoveryError> {
    Ok(PersistentFileEnvelope::canonical_v1(
        format_kind()?,
        "0.1.0"
            .parse()
            .map_err(|_| PrefabExtractionRecoveryError::HeaderInvalid)?,
        FormatGenerator::new("nara", engine_version()?)
            .map_err(|_| PrefabExtractionRecoveryError::HeaderInvalid)?,
        payload,
    ))
}

fn format_kind() -> Result<FormatKind, PrefabExtractionRecoveryError> {
    FormatKind::new(KIND).map_err(|_| PrefabExtractionRecoveryError::HeaderInvalid)
}

fn engine_version() -> Result<EngineVersion, PrefabExtractionRecoveryError> {
    env!("CARGO_PKG_VERSION")
        .parse()
        .map_err(|_| PrefabExtractionRecoveryError::HeaderInvalid)
}

fn preflight(encoded: &[u8], shape: SerdeShapeLimits) -> Result<(), PrefabExtractionRecoveryError> {
    let mut deserializer = serde_json::Deserializer::from_slice(encoded);
    preflight_serde_shape(&mut deserializer, shape).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => PrefabExtractionRecoveryError::Shape(error),
        SerdeShapePreflightError::Parse(_) => PrefabExtractionRecoveryError::Syntax,
    })?;
    deserializer
        .end()
        .map_err(|_| PrefabExtractionRecoveryError::Syntax)
}

pub(super) fn package_reservation(
    encoded: usize,
    shape: SerdeShapeLimits,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(usize, SerdeShapeLimits), PrefabExtractionRecoveryError> {
    let record = [
        size_of::<RecoveryPayload>(),
        size_of::<RecoveryOriginalSource>(),
        size_of::<RecoveryMetadataGuard>(),
        size_of::<RecoveryRoot>(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    adaptive_reservation(
        encoded,
        shape,
        checked_add(record, 192 + 32 + 64)?,
        8,
        budget,
    )
}

/// Narrows the actual serde shape before decoding. Memory capacity is not work performed and
/// source text inside a Blob is not a collection of Scene/catalog records at this boundary.
pub(super) fn adaptive_reservation(
    encoded: usize,
    shape: SerdeShapeLimits,
    node_bytes: usize,
    text_copies: usize,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(usize, SerdeShapeLimits), PrefabExtractionRecoveryError> {
    let fixed = checked_add(checked_mul(encoded, text_copies)?, OWNER_FIXED_BYTES)?;
    let available = budget
        .limits
        .retained_candidate_bytes()
        .get()
        .saturating_sub(budget.retained_candidate_bytes);
    let mut lower = 0;
    let mut upper = shape.nodes().get().min(encoded.max(1));
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2 + 1;
        if checked_add(fixed, checked_mul(midpoint, node_bytes)?)? <= available {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    let nodes = ItemLimit::new(lower).ok_or_else(|| {
        PrefabExtractionRecoveryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: budget.limits.retained_candidate_bytes().get(),
            },
        )
    })?;
    let narrowed = SerdeShapeLimits::new(
        shape.depth(),
        nodes,
        ItemLimit::new(shape.container_items().get().min(nodes.get()))
            .ok_or(PrefabExtractionRecoveryError::AccountingOverflow)?,
        shape.string_bytes(),
        shape.total_string_bytes(),
    );
    Ok((
        checked_add(fixed, checked_mul(nodes.get(), node_bytes)?)?,
        narrowed,
    ))
}

/// Bounded manifest/metadata owners only. Their own preflights cap manifests at 4096 nodes
/// and identity metadata at 256 nodes. Two manifests and their finite profile projections fit
/// this conservative model; Scene and bundle allocations use their own narrowed models.
pub(super) fn with_small_document<T>(
    encoded: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, PrefabExtractionRecoveryError>,
) -> Result<T, PrefabExtractionRecoveryError> {
    let storage = checked_add(
        checked_add(
            checked_mul(encoded, 8)?,
            checked_mul(
                encoded.min(2 * 4_096),
                checked_add(size_of::<ProjectManifest>(), 192)?,
            )?,
        )?,
        OWNER_FIXED_BYTES,
    )?;
    budget.charge_source_bytes(0, 0, checked_mul(encoded, 4)?, 0)?;
    with_temporary(storage, budget, work)
}

/// The closure's temporary owners must be destroyed before it returns. A returned owned value
/// needs a separate continuing reservation; this helper never refunds such ownership for it.
pub(super) fn with_temporary<T>(
    bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, PrefabExtractionRecoveryError>,
) -> Result<T, PrefabExtractionRecoveryError> {
    budget.charge_source_bytes(0, 0, 0, bytes)?;
    let result = work(budget);
    budget
        .reconcile_retained_reservation(bytes, 0)
        .map_err(|_| PrefabExtractionRecoveryError::AccountingOverflow)?;
    result
}

/// Refusal has dropped all local allocations before this releases their reservations. Preserve
/// performed work/encoded/decoded accounting and all preexisting retained storage and handles.
pub(super) fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<T, PrefabExtractionRecoveryError>,
) -> Result<T, PrefabExtractionRecoveryError> {
    let before = budget.retained_candidate_bytes;
    let result = prepare(budget);
    if result.is_err() {
        let released = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .ok_or(PrefabExtractionRecoveryError::AccountingOverflow)?;
        budget
            .reconcile_retained_reservation(released, 0)
            .map_err(|_| PrefabExtractionRecoveryError::AccountingOverflow)?;
    }
    result
}

// A serde measurement pass records counts/UTF-8 lengths without constructing the owned payload.
// Unknown fields are fully checked by the subsequent strict wire decode, after bounded shape.
#[derive(Deserialize)]
struct PackageMeasure {
    payload: PayloadMeasure,
}

#[derive(Deserialize)]
struct PayloadMeasure {
    old_ready_manifest: BlobMeasure,
    final_ready_manifest: BlobMeasure,
    unchanged_bundle: BundleMeasure,
    q: OutputMeasure,
    q_sidecar: OutputMeasure,
    #[serde(deserialize_with = "measure_sources")]
    complete_original_sources: ArrayMeasure,
    #[serde(deserialize_with = "measure_guards")]
    metadata_guards: ArrayMeasure,
    #[serde(deserialize_with = "measure_observations")]
    roots: ArrayMeasure,
    #[serde(deserialize_with = "measure_observations")]
    missing_sidecars: ArrayMeasure,
}

#[derive(Deserialize)]
struct BlobMeasure {
    #[serde(deserialize_with = "measure_text")]
    bytes: usize,
}

#[derive(Deserialize)]
struct BundleMeasure {
    blob: BlobMeasure,
}

#[derive(Deserialize)]
struct OutputMeasure {
    output: BlobMeasure,
}

#[derive(Deserialize)]
struct SourceMeasure {
    original_disk: BlobMeasure,
    reviewed_working: BlobMeasure,
    output: Option<BlobMeasure>,
}

#[derive(Deserialize)]
struct GuardMeasure {
    observed: BlobMeasure,
}

#[derive(Default)]
struct ArrayMeasure {
    items: usize,
    bytes: usize,
}

fn preflight_counts(
    encoded: &[u8],
    limits: PrefabExtractionRecoveryLimits,
) -> Result<(), PrefabExtractionRecoveryError> {
    let package: PackageMeasure =
        serde_json::from_slice(encoded).map_err(|_| PrefabExtractionRecoveryError::Syntax)?;
    let payload = package.payload;
    check_counts(
        payload.complete_original_sources.items,
        payload.metadata_guards.items,
        payload.roots.items,
        payload.missing_sidecars.items,
        limits,
    )?;
    let aggregate = [
        payload.old_ready_manifest.bytes,
        payload.final_ready_manifest.bytes,
        payload.unchanged_bundle.blob.bytes,
        payload.q.output.bytes,
        payload.q_sidecar.output.bytes,
        payload.complete_original_sources.bytes,
        payload.metadata_guards.bytes,
    ]
    .into_iter()
    .try_fold(0, checked_add)?;
    check_aggregate(aggregate, limits)
}

fn measure_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    struct TextMeasure;
    impl serde::de::Visitor<'_> for TextMeasure {
        type Value = usize;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("stored UTF-8 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<usize, E> {
            Ok(text.len())
        }
    }
    deserializer.deserialize_str(TextMeasure)
}

fn measure_sources<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, SourceMeasure>(deserializer, |source| {
        source
            .original_disk
            .bytes
            .checked_add(source.reviewed_working.bytes)?
            .checked_add(source.output.as_ref().map_or(0, |blob| blob.bytes))
    })
}

fn measure_guards<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, GuardMeasure>(deserializer, |guard| Some(guard.observed.bytes))
}

fn measure_observations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, serde::de::IgnoredAny>(deserializer, |_| Some(0))
}

fn measure_array<'de, D, T>(
    deserializer: D,
    bytes: fn(&T) -> Option<usize>,
) -> Result<ArrayMeasure, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ArrayVisitor<T> {
        bytes: fn(&T) -> Option<usize>,
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ArrayVisitor<T> {
        type Value = ArrayMeasure;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded extraction recovery array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<ArrayMeasure, A::Error> {
            let mut measured = ArrayMeasure::default();
            while let Some(value) = sequence.next_element::<T>()? {
                measured.items = measured
                    .items
                    .checked_add(1)
                    .ok_or_else(|| serde::de::Error::custom("recovery count overflow"))?;
                measured.bytes = (self.bytes)(&value)
                    .and_then(|bytes| measured.bytes.checked_add(bytes))
                    .ok_or_else(|| serde::de::Error::custom("recovery byte count overflow"))?;
            }
            Ok(measured)
        }
    }
    deserializer.deserialize_seq(ArrayVisitor { bytes })
}

struct CountingWriter {
    written: usize,
    maximum: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(bytes.len())
            .filter(|length| *length <= self.maximum)
            .ok_or_else(|| io::Error::other("extraction recovery byte limit exceeded"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PackageWriter {
    bytes: Vec<u8>,
    maximum: usize,
}

impl Write for PackageWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("extraction recovery byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
