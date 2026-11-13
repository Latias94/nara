//! Typed source execution and current-input revalidation, without importer discovery.

use std::{
    collections::BTreeMap,
    io::{Seek, SeekFrom},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use nara_asset::{
    AssetBuildSource, AssetImportError, AssetMeta, AssetMetaFileLimits, AssetMetaIdentityCandidate,
    AssetPath, AssetPathScope, AssetProductRef, AssetReferenceDemand, AssetSourceKind,
    AssetSourcePath, ImportedProductId, RuntimeAssetCodec, SourceAssetBuild,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildObservation, ArtifactBuildOutput,
    ArtifactBuildSeed, ArtifactBuildStore, ArtifactCommitReceipt, ArtifactContentDigest,
    ArtifactGroupCandidate, ArtifactPublicationPoint, VerifiedArtifactGroup,
    budget::ArtifactBuildLease,
};

/// Actual authorized source root/scope and compiled operation selected by the Host.
pub struct ArtifactSourceRequest {
    pub(super) directory: DirectoryCapability,
    pub(super) scope: AssetPathScope,
    pub(super) path: AssetPath,
    pub(super) definition: ArtifactBuildDefinition,
}

impl ArtifactSourceRequest {
    #[must_use]
    pub const fn new(
        directory: DirectoryCapability,
        scope: AssetPathScope,
        path: AssetPath,
        definition: ArtifactBuildDefinition,
    ) -> Self {
        Self {
            directory,
            scope,
            path,
            definition,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactBuildStatus {
    Hit,
    Built,
}

/// The observed reason for reuse or rebuilding; this is not a last-good success claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactBuildReason {
    Reused,
    NoPriorSuccess,
    RawInputChanged(AssetSourcePath),
    ProcessedInputChanged(AssetProductRef),
}

/// Verified current build bytes, independent of native resource publication.
pub struct ArtifactBuildResult {
    outcome: ArtifactBuildOutcome,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Keep the receipt inline without a new allocation after successful publication."
)]
enum ArtifactBuildOutcome {
    Reused(VerifiedArtifactGroup),
    Built {
        reason: ArtifactBuildReason,
        receipt: ArtifactCommitReceipt,
    },
}

impl ArtifactBuildResult {
    #[must_use]
    pub const fn status(&self) -> ArtifactBuildStatus {
        match self.outcome {
            ArtifactBuildOutcome::Reused(_) => ArtifactBuildStatus::Hit,
            ArtifactBuildOutcome::Built { .. } => ArtifactBuildStatus::Built,
        }
    }
    #[must_use]
    pub const fn reason(&self) -> &ArtifactBuildReason {
        match &self.outcome {
            ArtifactBuildOutcome::Reused(_) => &ArtifactBuildReason::Reused,
            ArtifactBuildOutcome::Built { reason, .. } => reason,
        }
    }
    #[must_use]
    pub fn group(&self) -> &VerifiedArtifactGroup {
        match &self.outcome {
            ArtifactBuildOutcome::Reused(group) => group,
            ArtifactBuildOutcome::Built { receipt, .. } => receipt.group(),
        }
    }

    /// Actual successful publication evidence; a cache hit performs no index replacement.
    #[must_use]
    pub const fn publication_receipt(&self) -> Option<&ArtifactCommitReceipt> {
        match &self.outcome {
            ArtifactBuildOutcome::Reused(_) => None,
            ArtifactBuildOutcome::Built { receipt, .. } => Some(receipt),
        }
    }

    #[must_use]
    pub fn into_group(self) -> VerifiedArtifactGroup {
        match self.outcome {
            ArtifactBuildOutcome::Reused(group) => group,
            ArtifactBuildOutcome::Built { receipt, .. } => receipt.into_group(),
        }
    }

    pub(super) fn reused(group: VerifiedArtifactGroup) -> Self {
        Self {
            outcome: ArtifactBuildOutcome::Reused(group),
        }
    }

    pub(super) fn built(receipt: ArtifactCommitReceipt, reason: ArtifactBuildReason) -> Self {
        Self {
            outcome: ArtifactBuildOutcome::Built { reason, receipt },
        }
    }
}

enum InputSnapshot {
    Missing,
    Present {
        file: FileCapability,
        digest: ArtifactContentDigest,
        bytes: Arc<[u8]>,
    },
}

impl InputSnapshot {
    fn bytes(&self) -> Option<&Arc<[u8]>> {
        match self {
            Self::Missing => None,
            Self::Present { bytes, .. } => Some(bytes),
        }
    }

    fn digest(&self) -> Option<ArtifactContentDigest> {
        match self {
            Self::Missing => None,
            Self::Present { digest, .. } => Some(*digest),
        }
    }
}

struct Input {
    local: AssetPath,
    snapshot: InputSnapshot,
    record: bool,
    // Input bytes and opened objects drop before returning their allowance.
    _path_lease: ArtifactBuildLease,
    _bytes_lease: ArtifactBuildLease,
}

pub(super) struct SourceInputs<'a> {
    pub(super) store: &'a ArtifactBuildStore,
    pub(super) request: &'a ArtifactSourceRequest,
    inputs: BTreeMap<AssetSourcePath, Input>,
}

impl SourceInputs<'_> {
    pub(super) fn new<'a>(
        store: &'a ArtifactBuildStore,
        request: &'a ArtifactSourceRequest,
    ) -> SourceInputs<'a> {
        SourceInputs {
            store,
            request,
            inputs: BTreeMap::new(),
        }
    }

    pub(super) fn forget_dependencies(&mut self) {
        self.inputs.retain(|_, input| {
            if input.local == self.request.path {
                input.record = true;
                true
            } else if input.local.as_str().strip_suffix(".meta") == Some(self.request.path.as_str())
            {
                input.record = false;
                true
            } else {
                false
            }
        });
    }

    pub(super) fn read(
        &mut self,
        path: &AssetPath,
        limit: ByteLimit,
        record: bool,
    ) -> Result<Option<Arc<[u8]>>, ArtifactBuildError> {
        let prefix_bytes = self.request.scope.prefix().map_or(Ok(0), |prefix| {
            prefix
                .as_str()
                .len()
                .checked_add(1)
                .ok_or(ArtifactBuildError::Overflow)
        })?;
        let path_work = prefix_bytes
            .checked_add(path.as_str().len())
            .and_then(|bytes| bytes.checked_mul(4))
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Input>() + 128))
            .ok_or(ArtifactBuildError::Overflow)?;
        let path_lease = self.store.reserve_work(path_work)?;
        let canonical = self.request.scope.resolve(path);
        if let Some(input) = self.inputs.get_mut(&canonical) {
            if input
                .snapshot
                .bytes()
                .is_some_and(|bytes| bytes.len() > limit.get())
            {
                return Err(ArtifactBuildError::InvalidCandidate(
                    "repeated input exceeds requested bound",
                ));
            }
            input.record |= record;
            return Ok(input.snapshot.bytes().cloned());
        }
        let limits = self.store.limits();
        if self.inputs.len() >= limits.observations.get() {
            return Err(ArtifactBuildError::Budget {
                kind: "input-count",
                required: self.inputs.len().saturating_add(1),
                limit: limits.observations.get(),
            });
        }
        let relative = RelativePath::new(path.as_str())
            .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid source input path"))?;
        let file = match self.request.directory.open_file_exact(&relative) {
            Ok(file) => Some(file),
            Err(error) if is_missing(&error) => None,
            Err(error) => return Err(error.into()),
        };
        let length = match &file {
            Some(file) => file.reader()?.seek(SeekFrom::End(0)).map_err(|source| {
                ArtifactBuildError::Host(FsError::Io {
                    operation: nara_fs::FsOperation::Read,
                    source,
                })
            })?,
            None => 0,
        };
        let length = usize::try_from(length).map_err(|_| ArtifactBuildError::Overflow)?;
        if length > limit.get() {
            return Err(ArtifactBuildError::Budget {
                kind: "source-bytes",
                required: length,
                limit: limit.get(),
            });
        }
        let bytes_lease = self
            .store
            .reserve_work(length.checked_mul(2).ok_or(ArtifactBuildError::Overflow)?)?;
        let snapshot = match file {
            Some(file) => {
                // Concurrent growth must refuse instead of enlarging the admitted allocation.
                let bytes: Arc<[u8]> = file.read_to_end_bounded(length as u64)?.into();
                let digest = ArtifactContentDigest::of_bytes(&bytes);
                InputSnapshot::Present {
                    file,
                    digest,
                    bytes,
                }
            }
            None => InputSnapshot::Missing,
        };
        let bytes = snapshot.bytes().cloned();
        self.inputs.insert(
            canonical,
            Input {
                local: path.clone(),
                snapshot,
                record,
                _path_lease: path_lease,
                _bytes_lease: bytes_lease,
            },
        );
        Ok(bytes)
    }

    pub(super) fn replay(
        &mut self,
        observations: &[ArtifactBuildObservation],
    ) -> Result<Option<AssetSourcePath>, ArtifactBuildError> {
        for observation in observations {
            let ArtifactBuildObservation::Raw { path, digest } = observation else {
                return Err(ArtifactBuildError::InvalidCache(
                    "source build contains a processed observation",
                ));
            };
            if let Some(input) = self.inputs.get_mut(path) {
                input.record = true;
                if input.snapshot.digest() != *digest {
                    return Ok(Some(path.clone()));
                }
                continue;
            }
            let spelling =
                self.request
                    .scope
                    .relative_path(path)
                    .ok_or(ArtifactBuildError::InvalidCache(
                        "input observation escapes the actual source scope",
                    ))?;
            let _path_work = self.store.reserve_work(
                spelling
                    .len()
                    .checked_mul(2)
                    .ok_or(ArtifactBuildError::Overflow)?,
            )?;
            let local = AssetPath::new(spelling)
                .map_err(|_| ArtifactBuildError::InvalidCache("invalid observed input path"))?;
            // Replaying raw input is independent of an encoded output member's size limit.
            // Growth or new presence only invalidates this historical read plan; execution
            // then applies the current domain's own requested input bounds.
            let observed_length = digest.map_or(Ok(1), |digest| {
                usize::try_from(digest.length()).map_err(|_| ArtifactBuildError::Overflow)
            })?;
            let limit =
                ByteLimit::new(observed_length.max(1)).ok_or(ArtifactBuildError::Overflow)?;
            let actual = match self.read(&local, limit, true) {
                Ok(actual) => actual,
                Err(ArtifactBuildError::Budget {
                    kind: "source-bytes",
                    ..
                })
                | Err(ArtifactBuildError::Host(FsError::ByteLimitExceeded { .. })) => {
                    return Ok(Some(path.clone()));
                }
                Err(error) => return Err(error),
            };
            if actual
                .as_ref()
                .map(|bytes| ArtifactContentDigest::of_bytes(bytes))
                != *digest
            {
                return Ok(Some(path.clone()));
            }
        }
        Ok(None)
    }

    pub(super) fn observations(&self) -> Vec<ArtifactBuildObservation> {
        self.inputs
            .iter()
            .filter(|(_, input)| input.record)
            .map(|(path, input)| ArtifactBuildObservation::Raw {
                path: path.clone(),
                digest: input.snapshot.digest(),
            })
            .collect()
    }

    pub(super) fn revalidate(&self) -> Result<(), ArtifactBuildError> {
        for input in self.inputs.values() {
            let path =
                RelativePath::new(input.local.as_str()).map_err(|_| ArtifactBuildError::Stale)?;
            let current = self.request.directory.open_file_exact(&path);
            match (&input.snapshot, current) {
                (InputSnapshot::Missing, Err(error)) if is_missing(&error) => {}
                (InputSnapshot::Present { file, digest, .. }, Ok(current)) => {
                    if !file.same_object(&current)? {
                        return Err(ArtifactBuildError::Stale);
                    }
                    current
                        .verify_digest(digest.content(), digest.length())
                        .map_err(|error| match error {
                            FsError::ByteLimitExceeded { .. } | FsError::DigestMismatch { .. } => {
                                ArtifactBuildError::Stale
                            }
                            error => ArtifactBuildError::Host(error),
                        })?;
                }
                (_, Err(error)) if !is_missing(&error) => return Err(error.into()),
                _ => return Err(ArtifactBuildError::Stale),
            }
        }
        Ok(())
    }
}

/// Executes one explicitly selected source/build/codec family or reuses verified current output.
///
/// Metadata identity and normalized settings are admitted before lookup. Every returned input
/// byte is captured under the actual directory/scope. A failed tracked read rejects the build
/// even if the domain ignores its result. The control callback can revoke the request at
/// publication boundaries; it must not re-enter this store while its commit lock is held.
pub fn build_source<D>(
    store: &ArtifactBuildStore,
    request: &ArtifactSourceRequest,
    mut control: impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    catch_unwind(AssertUnwindSafe(|| {
        build_source_inner::<D>(store, request, &mut control)
    }))
    .unwrap_or(Err(ArtifactBuildError::Panicked))
}

fn build_source_inner<D>(
    store: &ArtifactBuildStore,
    request: &ArtifactSourceRequest,
    control: &mut impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
) -> Result<ArtifactBuildResult, ArtifactBuildError>
where
    D: SourceAssetBuild,
    D::Settings: Serialize,
{
    if request.definition.id != D::BUILD_ID
        || request.definition.revision.get() != D::BUILD_REVISION
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "compiled source build definition mismatch",
        ));
    }
    let import_limits = D::limits();
    let metadata_limit = AssetMetaFileLimits::default().encoded_bytes();
    let fixed_work = import_limits
        .work_bytes()
        .get()
        .checked_add(import_limits.retained_bytes().get())
        .and_then(|bytes| {
            metadata_limit
                .get()
                .checked_mul(8)
                .and_then(|metadata| bytes.checked_add(metadata))
        })
        .ok_or(ArtifactBuildError::Overflow)?;
    let _operation = store.reserve_work(fixed_work)?;
    let mut inputs = SourceInputs::new(store, request);
    let (actual, normalized) = admit_metadata::<D::Settings>(
        &mut inputs,
        &AssetSourceKind::Other(D::SOURCE_KIND.to_owned()),
        D::normalize_settings,
    )?;
    let primary = inputs
        .read(&request.path, import_limits.source_bytes(), true)?
        .ok_or(ArtifactBuildError::InvalidCandidate(
            "primary source is absent",
        ))?;
    let product = AssetProductRef::new(actual.identity().stable_id(), ImportedProductId::PRIMARY)
        .map_err(|_| {
        ArtifactBuildError::InvalidCandidate("source has no persistent Product identity")
    })?;
    let seed = ArtifactBuildSeed {
        source: product,
        location: Some(request.scope.resolve(&request.path)),
        primary: Some(ArtifactContentDigest::of_bytes(&primary)),
        definitions: vec![request.definition.clone()],
        settings: normalized,
        target: Vec::new(),
        format: FormatKind::new(D::Codec::RUNTIME_KIND)
            .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid compiled Runtime format"))?,
        format_version: FormatVersion::new(D::Codec::RUNTIME_VERSION).ok_or(
            ArtifactBuildError::InvalidCandidate("invalid compiled Runtime version"),
        )?,
    };
    let reason = if let Some(group) = store.lookup_seed(&seed)? {
        if let Some(changed) = inputs.replay(group.observations())? {
            inputs.forget_dependencies();
            ArtifactBuildReason::RawInputChanged(changed)
        } else {
            control(ArtifactPublicationPoint::Reuse)?;
            inputs.revalidate()?;
            return Ok(ArtifactBuildResult::reused(group));
        }
    } else {
        ArtifactBuildReason::NoPriorSuccess
    };
    let mut read_failure = None;
    let built = {
        let mut reader = |path: &AssetPath, limit| {
            inputs.read(path, limit, true).map_err(|error| {
                read_failure.get_or_insert(error);
            })
        };
        let mut source = AssetBuildSource::new(&actual, primary, &mut reader);
        D::build(&mut source)
    };
    if let Some(error) = read_failure {
        return Err(error);
    }
    let value = built.map_err(|error| match error {
        AssetImportError::Source(()) => {
            ArtifactBuildError::InvalidCandidate("source dependency read failed")
        }
        AssetImportError::Domain(diagnostic) => ArtifactBuildError::Domain(diagnostic),
    })?;
    let codec_limits = D::Codec::runtime_limits();
    let codec_work = codec_limits
        .work_bytes()
        .get()
        .checked_add(
            codec_limits
                .encoded_bytes()
                .get()
                .checked_mul(2)
                .ok_or(ArtifactBuildError::Overflow)?,
        )
        .ok_or(ArtifactBuildError::Overflow)?;
    let _encode = store.reserve_work(codec_work)?;
    let product_work = store
        .limits()
        .members
        .get()
        .checked_mul(std::mem::size_of::<AssetProductRef>())
        .ok_or(ArtifactBuildError::Overflow)?;
    let _products = store.reserve_work(product_work)?;
    let mut products = Vec::with_capacity(store.limits().members.get());
    D::visit_products(&value, |reference| {
        if products.len() >= store.limits().members.get() {
            return Err(ArtifactBuildError::InvalidCandidate(
                "too many compound Products",
            ));
        }
        if reference.source() != product.source()
            || !D::contains_product(&value, reference.product())
            || !D::Codec::contains_runtime_product(&value, reference)
        {
            return Err(ArtifactBuildError::InvalidCandidate(
                "actual build Product identity mismatch",
            ));
        }
        products.push(reference);
        Ok(())
    })?;
    products.sort_unstable();
    if !products.contains(&product) || products.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ArtifactBuildError::InvalidCandidate(
            "compound build Product set is empty or duplicated",
        ));
    }
    let dependency_capacity = store.limits().observations.get();
    let _dependencies = store.reserve_work(
        dependency_capacity
            .checked_mul(std::mem::size_of::<(AssetProductRef, AssetReferenceDemand)>())
            .ok_or(ArtifactBuildError::Overflow)?,
    )?;
    let mut dependencies = Vec::with_capacity(dependency_capacity);
    D::Codec::visit_runtime_dependencies(&value, |reference, demand| {
        if dependencies.len() >= dependency_capacity {
            return Err(ArtifactBuildError::InvalidCandidate(
                "too many actual Runtime dependency occurrences",
            ));
        }
        dependencies.push((reference, demand));
        Ok(())
    })?;
    dependencies.sort_unstable_by_key(|(reference, _)| *reference);
    let mut write = 0;
    for read in 0..dependencies.len() {
        let (reference, demand) = dependencies[read];
        if write > 0 && dependencies[write - 1].0 == reference {
            if demand == AssetReferenceDemand::Immediate {
                dependencies[write - 1].1 = demand;
            }
        } else {
            dependencies[write] = (reference, demand);
            write += 1;
        }
    }
    dependencies.truncate(write);
    let encoded = D::Codec::encode_runtime(&value).map_err(ArtifactBuildError::Codec)?;
    if encoded.len() > codec_limits.encoded_bytes().get() {
        return Err(ArtifactBuildError::InvalidCandidate(
            "encoder exceeded its original byte promise",
        ));
    }
    // Finish trusted product destruction before any successful group can become visible.
    drop(value);
    let candidate = ArtifactGroupCandidate {
        observations: inputs.observations(),
        outputs: vec![ArtifactBuildOutput {
            name: "runtime".to_owned(),
            format: seed.format.clone(),
            format_version: seed.format_version,
            products,
            dependencies,
            bytes: Arc::from(encoded),
        }],
        seed,
    };
    let committed = store.commit(candidate, |point| {
        control(point)?;
        inputs.revalidate()
    })?;
    Ok(ArtifactBuildResult::built(committed, reason))
}

fn is_missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

pub(super) fn admit_metadata<T: DeserializeOwned + Serialize>(
    inputs: &mut SourceInputs<'_>,
    kind: &AssetSourceKind,
    normalize: impl FnOnce(T) -> Result<T, nara_diagnostic::Diagnostic>,
) -> Result<(AssetMetaIdentityCandidate, Vec<u8>), ArtifactBuildError> {
    let path_work = inputs
        .request
        .path
        .as_str()
        .len()
        .checked_add(5)
        .and_then(|bytes| bytes.checked_mul(3))
        .ok_or(ArtifactBuildError::Overflow)?;
    let _path_work = inputs.store.reserve_work(path_work)?;
    let metadata_path = AssetPath::new(inputs.request.path.meta_path())
        .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid source metadata path"))?;
    let raw = inputs
        .read(
            &metadata_path,
            AssetMetaFileLimits::default().encoded_bytes(),
            false,
        )?
        .ok_or(ArtifactBuildError::InvalidCandidate(
            "source metadata is absent",
        ))?;
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&raw)
        .map_err(|_| ArtifactBuildError::InvalidCandidate("source metadata was rejected"))?;
    if metadata.identity().path() != &inputs.request.path
        || metadata.identity().source_kind() != kind
    {
        return Err(ArtifactBuildError::InvalidCandidate(
            "source metadata binding mismatch",
        ));
    }
    let selected = metadata
        .admit::<T>()
        .map_err(|_| ArtifactBuildError::InvalidCandidate("source settings were rejected"))?;
    let (_, settings) = selected.into_meta().into_parts();
    let settings = normalize(settings)
        .map_err(|diagnostic| ArtifactBuildError::Domain(Box::new(diagnostic)))?;
    let normalized = AssetMeta::new(metadata.identity().clone(), settings)
        .to_json_string()
        .map_err(|_| ArtifactBuildError::InvalidCandidate("normalized settings were rejected"))?;
    let mut normalized: serde_json::Value = serde_json::from_str(&normalized).map_err(|_| {
        ArtifactBuildError::InvalidCandidate("normalized metadata could not be read")
    })?;
    sort_objects(&mut normalized);
    let normalized = serde_json::to_vec(&normalized).map_err(|_| {
        ArtifactBuildError::InvalidCandidate("normalized metadata could not be encoded")
    })?;
    let actual = AssetMetaIdentityCandidate::decode_json_bytes(&normalized).map_err(|_| {
        ArtifactBuildError::InvalidCandidate("normalized metadata failed admission")
    })?;
    Ok((actual, normalized))
}

fn sort_objects(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for value in map.values_mut() {
                sort_objects(value);
            }
            let sorted: BTreeMap<_, _> = std::mem::take(map).into_iter().collect();
            map.extend(sorted);
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(sort_objects),
        _ => {}
    }
}
