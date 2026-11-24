use std::io::{self, Write};

use nara_asset::AssetMetaFileLimits;
use nara_core::{
    EngineVersion, FormatGenerator, FormatKind, PersistentFileContract, PersistentFileEnvelope,
    PersistentFileHeader, SerdeShapePreflightError, preflight_serde_shape,
};
use nara_project::DEFAULT_MANIFEST_BYTE_LIMIT;
use nara_reflect::AuthoringSchemaBundleLimits;

use super::{PrefabComponentApplyRecoveryError as Error, *};
use crate::project_host::authoring::source_inventory::release;

const HEADER_SCRATCH: usize = 16 * 1024;
const OWNER_BYTES: usize = size_of::<PrefabComponentApplyRecoveryData>() + 2 * size_of::<usize>();
const NODE_BYTES: usize = 256 + size_of::<RecoverySource>();

pub(super) fn admit_input(
    input: &PrefabComponentApplyRecoveryInput<'_>,
    limits: PrefabComponentApplyRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), Error> {
    let sources = input.complete_original_sources;
    let guards = input.metadata_guards;
    let roots = input.roots;
    let missing = input.missing_sidecars;
    check_counts(
        sources.len(),
        guards.len(),
        roots.len(),
        missing.len(),
        limits,
    )?;
    let entries = sum(
        sum(sources.len(), guards.len())?,
        sum(roots.len(), missing.len())?,
    )?;
    budget.charge_source_bytes(0, 0, product(entries, 8)?, 0)?;
    let largest_source = sources
        .iter()
        .map(|source| source.original_disk.len())
        .chain([
            input.containing.output.len(),
            input.destination.output.len(),
        ])
        .max()
        .unwrap_or(0);
    check_file_sizes(
        input.ready_manifest.len(),
        input.bundle_bytes.len(),
        largest_source,
        guards
            .iter()
            .map(|guard| guard.observed.len())
            .max()
            .unwrap_or(0),
        limits,
    )?;
    let aggregate = [
        input.ready_manifest.len(),
        input.bundle_bytes.len(),
        input.containing.output.len(),
        input.destination.output.len(),
    ]
    .into_iter()
    .chain(sources.iter().map(|source| source.original_disk.len()))
    .chain(guards.iter().map(|guard| guard.observed.len()))
    .try_fold(0, sum)?;
    check_aggregate(aggregate, limits)?;
    let structural = [
        OWNER_BYTES,
        input.bundle_path.len(),
        input.selected_profile.map_or(0, str::len),
        product(sources.len(), size_of::<RecoverySource>())?,
        product(guards.len(), size_of::<RecoveryMetadataGuard>())?,
        product(roots.len(), size_of::<RecoveryRoot>())?,
        product(missing.len(), size_of::<String>())?,
    ]
    .into_iter()
    .chain(
        sources
            .iter()
            .flat_map(|source| [source.path.len(), source.key.retained_bytes()]),
    )
    .chain(guards.iter().map(|guard| guard.path.len()))
    .chain(
        roots
            .iter()
            .flat_map(|root| [root.path.len(), root.missing_ancestor.map_or(0, str::len)]),
    )
    .chain(missing.iter().map(|path| path.len()))
    .try_fold(0, sum)?;
    let retained = sum(structural, aggregate)?;
    let comparisons = (usize::BITS - entries.leading_zeros()) as usize + 2;
    let sort_work = product(structural, comparisons)?;
    budget.charge_source_bytes(0, 0, sum(product(retained, 3)?, sort_work)?, retained)?;
    Ok(())
}

pub(super) fn encode(
    payload: &RecoveryPayload,
    limits: PrefabComponentApplyRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<u8>, Error> {
    budget.charge_source_bytes(
        0,
        0,
        product(retained_payload(payload)?, 8)?,
        HEADER_SCRATCH,
    )?;
    let result = (|| {
        let envelope = PersistentFileEnvelope::canonical_v1(
            format_kind()?,
            EngineVersion::parse("0.1.0").map_err(|_| Error::Header)?,
            FormatGenerator::new("nara", engine_version()?).map_err(|_| Error::Header)?,
            payload,
        );
        let mut counter = CountingWriter {
            bytes: 0,
            maximum: limits.encoded_bytes.get(),
        };
        serde_json::to_writer(&mut counter, &envelope).map_err(|_| Error::EncodedBytesExceeded)?;
        budget.charge_source_bytes(counter.bytes, 0, product(counter.bytes, 2)?, counter.bytes)?;
        let mut encoded = vec![0; counter.bytes];
        let mut writer = encoded.as_mut_slice();
        serde_json::to_writer(&mut writer, &envelope).map_err(|_| Error::Syntax)?;
        if !writer.is_empty() {
            return Err(SourceInventoryError::AccountingOverflow.into());
        }
        with_temporary_storage(budget, |budget| {
            let (shape, _) = reserve_decode(&encoded, limits.shape, 0, budget)?;
            preflight(&encoded, shape)
        })?;
        Ok(encoded)
    })();
    release(budget, HEADER_SCRATCH)?;
    result
}

pub(super) fn decode(
    encoded: &[u8],
    limits: PrefabComponentApplyRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RecoveryPayload, Error> {
    if encoded.len() > limits.encoded_bytes.get() {
        return Err(Error::EncodedBytesExceeded);
    }
    let (shape, reserved) = reserve_decode(encoded, limits.shape, encoded.len(), budget)?;
    preflight(encoded, shape)?;
    {
        let header: PersistentFileHeader =
            serde_json::from_slice(encoded).map_err(|_| Error::Syntax)?;
        PersistentFileContract::canonical_v1(format_kind()?, engine_version()?)
            .validate_header(&header)
            .map_err(Error::Contract)?;
    }
    preflight_bodies(encoded, limits)?;
    let envelope: PersistentFileEnvelope<RecoveryPayload> =
        serde_json::from_slice(encoded).map_err(|_| Error::Syntax)?;
    let payload = envelope.into_payload();
    let unused = reserved
        .checked_sub(retained_payload(&payload)?)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    release(budget, unused)?;
    Ok(payload)
}

fn reserve_decode(
    encoded: &[u8],
    shape: SerdeShapeLimits,
    encoded_charge: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(SerdeShapeLimits, usize), Error> {
    let fixed = sum(product(encoded.len(), 16)?, HEADER_SCRATCH)?;
    let maximum = budget.limits.retained_candidate_bytes();
    let available = maximum
        .get()
        .saturating_sub(budget.retained_candidate_bytes)
        .saturating_sub(fixed);
    let nodes = (available / NODE_BYTES)
        .min(shape.nodes().get())
        .min(encoded.len().max(1));
    let nodes = ItemLimit::new(nodes).ok_or(
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: maximum.get(),
        },
    )?;
    let container_items = ItemLimit::new(shape.container_items().get().min(nodes.get()))
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    let shape = SerdeShapeLimits::new(
        shape.depth(),
        nodes,
        container_items,
        shape.string_bytes(),
        shape.total_string_bytes(),
    );
    let reservation = sum(fixed, product(nodes.get(), NODE_BYTES)?)?;
    budget.charge_source_bytes(encoded_charge, 0, product(encoded.len(), 16)?, reservation)?;
    Ok((shape, reservation))
}

fn preflight(encoded: &[u8], shape: SerdeShapeLimits) -> Result<(), Error> {
    let mut parser = serde_json::Deserializer::from_slice(encoded);
    preflight_serde_shape(&mut parser, shape).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => Error::Shape(error),
        SerdeShapePreflightError::Parse(_) => Error::Syntax,
    })?;
    parser.end().map_err(|_| Error::Syntax)
}

fn retained_payload(payload: &RecoveryPayload) -> Result<usize, SourceInventoryError> {
    [
        OWNER_BYTES,
        payload.unchanged_bundle.path.capacity(),
        payload
            .selected_profile
            .as_ref()
            .map_or(0, String::capacity),
        product(
            payload.complete_original_sources.capacity(),
            size_of::<RecoverySource>(),
        )?,
        product(
            payload.metadata_guards.capacity(),
            size_of::<RecoveryMetadataGuard>(),
        )?,
        product(payload.roots.capacity(), size_of::<RecoveryRoot>())?,
        product(payload.missing_sidecars.capacity(), size_of::<String>())?,
    ]
    .into_iter()
    .chain(payload.blobs().map(|blob| blob.bytes.capacity()))
    .chain(
        payload
            .complete_original_sources
            .iter()
            .flat_map(|source| [source.path.capacity(), source.key.retained_bytes()]),
    )
    .chain(
        payload
            .metadata_guards
            .iter()
            .map(|guard| guard.path.capacity()),
    )
    .chain(payload.roots.iter().flat_map(|root| {
        [
            root.path.capacity(),
            root.missing_ancestor.as_ref().map_or(0, String::capacity),
        ]
    }))
    .chain(payload.missing_sidecars.iter().map(String::capacity))
    .try_fold(0, sum)
}

fn check_counts(
    sources: usize,
    guards: usize,
    roots: usize,
    missing: usize,
    limits: PrefabComponentApplyRecoveryLimits,
) -> Result<(), Error> {
    if !(2..=limits.sources.get()).contains(&sources) {
        return Err(Error::SourceCount);
    }
    if sum(sum(guards, roots)?, missing)? > limits.observations.get() {
        return Err(Error::ObservationCount);
    }
    Ok(())
}

fn check_aggregate(bytes: usize, limits: PrefabComponentApplyRecoveryLimits) -> Result<(), Error> {
    if bytes > limits.aggregate_bytes.get() {
        return Err(Error::AggregateBytesExceeded);
    }
    Ok(())
}

fn check_file_sizes(
    manifest: usize,
    bundle: usize,
    source: usize,
    metadata: usize,
    limits: PrefabComponentApplyRecoveryLimits,
) -> Result<(), Error> {
    if manifest > DEFAULT_MANIFEST_BYTE_LIMIT as usize
        || bundle > AuthoringSchemaBundleLimits::default().encoded_bytes().get()
        || source > limits.scene.encoded_bytes().get()
        || metadata > AssetMetaFileLimits::default().encoded_bytes().get()
    {
        return Err(Error::BlobBytesExceeded);
    }
    Ok(())
}

fn format_kind() -> Result<FormatKind, Error> {
    FormatKind::new(KIND).map_err(|_| Error::Header)
}

fn engine_version() -> Result<EngineVersion, Error> {
    EngineVersion::parse(env!("CARGO_PKG_VERSION")).map_err(|_| Error::Header)
}

struct CountingWriter {
    bytes: usize,
    maximum: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, chunk: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(chunk.len())
            .filter(|bytes| *bytes <= self.maximum)
            .ok_or_else(|| io::Error::other("component Apply recovery byte limit exceeded"))?;
        Ok(chunk.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Deserialize)]
struct PackageMeasure {
    payload: PayloadMeasure,
}

#[derive(Deserialize)]
struct PayloadMeasure {
    ready_manifest: BlobMeasure,
    unchanged_bundle: BundleMeasure,
    containing: OutputMeasure,
    destination: OutputMeasure,
    #[serde(deserialize_with = "measure_sources")]
    complete_original_sources: TableMeasure,
    #[serde(deserialize_with = "measure_guards")]
    metadata_guards: TableMeasure,
    #[serde(deserialize_with = "measure_observations")]
    roots: TableMeasure,
    #[serde(deserialize_with = "measure_observations")]
    missing_sidecars: TableMeasure,
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
}

#[derive(Deserialize)]
struct GuardMeasure {
    observed: BlobMeasure,
}

#[derive(Default)]
struct TableMeasure {
    items: usize,
    bytes: usize,
    largest: usize,
}

fn preflight_bodies(
    encoded: &[u8],
    limits: PrefabComponentApplyRecoveryLimits,
) -> Result<(), Error> {
    let package: PackageMeasure = serde_json::from_slice(encoded).map_err(|_| Error::Syntax)?;
    let payload = package.payload;
    check_counts(
        payload.complete_original_sources.items,
        payload.metadata_guards.items,
        payload.roots.items,
        payload.missing_sidecars.items,
        limits,
    )?;
    check_file_sizes(
        payload.ready_manifest.bytes,
        payload.unchanged_bundle.blob.bytes,
        payload
            .complete_original_sources
            .largest
            .max(payload.containing.output.bytes)
            .max(payload.destination.output.bytes),
        payload.metadata_guards.largest,
        limits,
    )?;
    let aggregate = [
        payload.ready_manifest.bytes,
        payload.unchanged_bundle.blob.bytes,
        payload.containing.output.bytes,
        payload.destination.output.bytes,
        payload.complete_original_sources.bytes,
        payload.metadata_guards.bytes,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    check_aggregate(aggregate, limits)
}

fn measure_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    struct Text;
    impl serde::de::Visitor<'_> for Text {
        type Value = usize;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("stored UTF-8 bytes")
        }

        fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<usize, E> {
            Ok(text.len())
        }
    }
    deserializer.deserialize_str(Text)
}

fn measure_sources<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TableMeasure, D::Error> {
    measure_table::<D, SourceMeasure>(deserializer, |source| source.original_disk.bytes)
}

fn measure_guards<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TableMeasure, D::Error> {
    measure_table::<D, GuardMeasure>(deserializer, |guard| guard.observed.bytes)
}

fn measure_observations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TableMeasure, D::Error> {
    measure_table::<D, serde::de::IgnoredAny>(deserializer, |_| 0)
}

fn measure_table<'de, D, T>(deserializer: D, size: fn(T) -> usize) -> Result<TableMeasure, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Table<T>(fn(T) -> usize);

    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Table<T> {
        type Value = TableMeasure;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded component Apply inventory array")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut measured = TableMeasure::default();
            while let Some(value) = sequence.next_element::<T>()? {
                let bytes = (self.0)(value);
                measured.items = measured
                    .items
                    .checked_add(1)
                    .ok_or_else(|| serde::de::Error::custom("recovery item count overflow"))?;
                measured.bytes = measured
                    .bytes
                    .checked_add(bytes)
                    .ok_or_else(|| serde::de::Error::custom("recovery byte count overflow"))?;
                measured.largest = measured.largest.max(bytes);
            }
            Ok(measured)
        }
    }

    deserializer.deserialize_seq(Table(size))
}
