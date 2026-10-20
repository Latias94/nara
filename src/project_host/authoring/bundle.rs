use std::sync::Arc;

use nara_core::ByteLimit;
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_project::ProjectPath;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, SchemaCompositionFingerprint,
};

use super::{AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError};

pub(super) struct AdmittedAuthoringSchemaBundle {
    path: ProjectPath,
    capability: FileCapability,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    bundle: Arc<AuthoringSchemaBundle>,
}

impl AdmittedAuthoringSchemaBundle {
    pub(super) const fn path(&self) -> &ProjectPath {
        &self.path
    }

    pub(super) const fn capability(&self) -> &FileCapability {
        &self.capability
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) const fn digest(&self) -> ContentDigest {
        self.digest
    }

    pub(super) fn bundle(&self) -> &AuthoringSchemaBundle {
        &self.bundle
    }

    pub(super) fn shared_bundle(&self) -> Arc<AuthoringSchemaBundle> {
        Arc::clone(&self.bundle)
    }

    pub(super) fn fingerprint(&self) -> SchemaCompositionFingerprint {
        self.bundle.fingerprint()
    }
}

pub(super) fn admit_bundle(
    project_root: &DirectoryCapability,
    path: ProjectPath,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<AdmittedAuthoringSchemaBundle, AuthoringProjectAdmissionError> {
    budget.charge_reference(path.as_str().len(), 1, true)?;
    let relative_path =
        RelativePath::new(path.as_str()).map_err(AuthoringProjectAdmissionError::BundlePath)?;
    let capability = project_root
        .open_file(&relative_path)
        .map_err(AuthoringProjectAdmissionError::BundleHost)?;
    budget.charge_open_file(2)?;

    let bundle_limits = AuthoringSchemaBundleLimits::default();
    let aggregate_maximum = budget.maximum();
    let (read_limit, aggregate_limited) = budget.read_limit(bundle_limits.encoded_bytes().get())?;
    let bytes = capability
        .read_to_end_bounded(u64::try_from(read_limit).unwrap_or(u64::MAX))
        .map_err(|error| {
            map_bundle_read_error(error, aggregate_limited, read_limit, aggregate_maximum)
        })?;
    let digest = ContentDigest::of_bytes(&bytes);
    let bundle = AuthoringSchemaBundle::from_json_bytes_with_limits(
        &bytes,
        bundle_limits.with_encoded_bytes(
            ByteLimit::new(read_limit).expect("an admitted bundle read limit is non-zero"),
        ),
    )
    .map_err(AuthoringProjectAdmissionError::BundleInvalid)?;
    budget.charge_bundle_bytes(bytes.len())?;

    Ok(AdmittedAuthoringSchemaBundle {
        path,
        capability,
        bytes: Arc::from(bytes),
        digest,
        bundle: Arc::new(bundle),
    })
}

fn map_bundle_read_error(
    error: FsError,
    aggregate_limited: bool,
    read_limit: usize,
    aggregate_maximum: usize,
) -> AuthoringProjectAdmissionError {
    match error {
        FsError::ByteLimitExceeded { .. } if aggregate_limited => {
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                maximum: aggregate_maximum,
            }
        }
        FsError::ByteLimitExceeded { .. } => AuthoringProjectAdmissionError::BundleTooLarge {
            maximum: read_limit,
        },
        error => AuthoringProjectAdmissionError::BundleHost(error),
    }
}
