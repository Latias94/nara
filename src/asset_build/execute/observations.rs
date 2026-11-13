use super::{
    ArtifactBuildError, ArtifactBuildLease, ArtifactBuildStore, ArtifactContentDigest,
    ArtifactSourceRequest, AssetPath, AssetSourcePath, BTreeMap, FileCapability, InputSnapshot,
    RelativePath, is_missing,
};
use nara_core::ByteLimit;
use std::io::{Seek, SeekFrom};

#[derive(Default)]
pub(in crate::asset_build) struct SourceBuildObservations {
    inputs: BTreeMap<AssetSourcePath, ObservedInput>,
    inputs_executed: bool,
}

struct ObservedInput {
    local: AssetPath,
    snapshot: ObservedSnapshot,
    failed_probe: Option<ByteLimit>,
    _lease: ArtifactBuildLease,
}

enum ObservedSnapshot {
    Unresolved,
    Missing,
    Present {
        file: FileCapability,
        digest: ArtifactContentDigest,
    },
}

impl SourceBuildObservations {
    pub(super) fn begin_input_execution(&mut self) {
        self.inputs_executed = true;
    }

    pub(in crate::asset_build) fn may_replace_retry(&self) -> bool {
        self.inputs_executed
    }

    pub(super) fn track(
        &mut self,
        store: &ArtifactBuildStore,
        canonical: &AssetSourcePath,
        local: &AssetPath,
    ) -> Result<(), ArtifactBuildError> {
        if self.inputs.contains_key(canonical) {
            return Ok(());
        }
        if self.inputs.len() >= store.limits().observations.get() {
            return Err(ArtifactBuildError::Budget {
                kind: "attempt-input-count",
                required: self.inputs.len().saturating_add(1),
                limit: store.limits().observations.get(),
            });
        }
        let bytes = canonical
            .as_str()
            .len()
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(local.as_str().len()))
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<ObservedInput>()))
            .and_then(|bytes| bytes.checked_add(2 * std::mem::size_of::<AssetSourcePath>() + 256))
            .ok_or(ArtifactBuildError::Overflow)?;
        let lease = store.reserve_retained(bytes)?;
        self.inputs.insert(
            canonical.clone(),
            ObservedInput {
                local: local.clone(),
                snapshot: ObservedSnapshot::Unresolved,
                failed_probe: None,
                _lease: lease,
            },
        );
        Ok(())
    }

    pub(super) fn capture(&mut self, path: &AssetSourcePath, snapshot: InputSnapshot) {
        if let Some(input) = self.inputs.get_mut(path) {
            input.snapshot = match snapshot {
                InputSnapshot::Missing => ObservedSnapshot::Missing,
                InputSnapshot::Present { file, digest, .. } => {
                    ObservedSnapshot::Present { file, digest }
                }
            };
        }
    }

    pub(super) fn failed(
        &mut self,
        local: &AssetPath,
        limit: ByteLimit,
        error: &ArtifactBuildError,
    ) {
        if matches!(
            error,
            ArtifactBuildError::Host(_)
                | ArtifactBuildError::Budget {
                    kind: "source-bytes",
                    ..
                }
        ) && let Some(input) = self.inputs.values_mut().find(|input| &input.local == local)
        {
            input.failed_probe.get_or_insert(limit);
        }
    }

    pub(super) fn retain_selection(&mut self, request: &ArtifactSourceRequest) {
        self.inputs.retain(|_, input| {
            input.local == request.path
                || input.local.as_str().strip_suffix(".meta") == Some(request.path.as_str())
        });
    }

    pub(in crate::asset_build) fn paths(&self) -> impl Iterator<Item = &AssetSourcePath> {
        self.inputs.keys()
    }

    pub(in crate::asset_build) fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    pub(in crate::asset_build) fn is_complete(&self) -> bool {
        self.inputs
            .values()
            .all(|input| !matches!(input.snapshot, ObservedSnapshot::Unresolved))
    }

    /// Verifies every successful observation against its original opened object. An unresolved
    /// failed read supplies a retry path, not an invented presence/content observation.
    pub(in crate::asset_build) fn revalidate(
        &self,
        store: &ArtifactBuildStore,
        request: &ArtifactSourceRequest,
    ) -> Result<(), ArtifactBuildError> {
        if self.is_empty() {
            return Ok(());
        }
        let _scratch = store.reserve_work(64 * 1024 + 4096)?;
        for input in self.inputs.values() {
            if let Some(limit) = input.failed_probe
                && recovered_read(store, request, &input.local, limit)?
            {
                return Err(ArtifactBuildError::Stale);
            }
            if matches!(input.snapshot, ObservedSnapshot::Unresolved) {
                continue;
            }
            let local =
                RelativePath::new(input.local.as_str()).map_err(|_| ArtifactBuildError::Stale)?;
            let current = request.directory.open_file_exact(&local);
            match (&input.snapshot, current) {
                (ObservedSnapshot::Missing, Err(error)) if is_missing(&error) => {}
                (ObservedSnapshot::Present { file, digest }, Ok(current)) => {
                    if !file.same_object(&current)? {
                        return Err(ArtifactBuildError::Stale);
                    }
                    current
                        .verify_digest(digest.content(), digest.length())
                        .map_err(|error| match error {
                            nara_fs::FsError::ByteLimitExceeded { .. }
                            | nara_fs::FsError::DigestMismatch { .. } => ArtifactBuildError::Stale,
                            error => error.into(),
                        })?;
                }
                (_, Err(error)) if !is_missing(&error) => return Err(error.into()),
                _ => return Err(ArtifactBuildError::Stale),
            }
        }
        Ok(())
    }
}

fn recovered_read(
    store: &ArtifactBuildStore,
    request: &ArtifactSourceRequest,
    local: &AssetPath,
    limit: ByteLimit,
) -> Result<bool, ArtifactBuildError> {
    let local = RelativePath::new(local.as_str())
        .map_err(|_| ArtifactBuildError::InvalidCandidate("invalid failed source input path"))?;
    let file = match request.directory.open_file_exact(&local) {
        Ok(file) => file,
        Err(error) => return Ok(is_missing(&error)),
    };
    let length = match file.reader().and_then(|mut reader| {
        reader
            .seek(SeekFrom::End(0))
            .map_err(|source| nara_fs::FsError::Io {
                operation: nara_fs::FsOperation::Read,
                source,
            })
    }) {
        Ok(length) => usize::try_from(length).map_err(|_| ArtifactBuildError::Overflow)?,
        Err(_) => return Ok(false),
    };
    if length > limit.get() {
        return Ok(false);
    }
    let _work = store.reserve_work(length.checked_mul(2).ok_or(ArtifactBuildError::Overflow)?)?;
    Ok(file.read_to_end_bounded(length as u64).is_ok())
}
