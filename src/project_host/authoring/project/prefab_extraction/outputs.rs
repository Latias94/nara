//! Fixed in-memory publication members issued from the actual reviewed source owners.
//!
//! This owner grants no Pending authority, file replacement or Workspace installation. A failed
//! attempt retains its Review, completed buffers and current member under the original allowances.

use nara_asset::AssetMeta;
use nara_scene::{
    __private::{EncodedPrefabExtractionSource, PrefabExtractionSourceOutput},
    SceneFileEncodingAdmissionError,
};
use nara_tooling::{
    EditorPersistenceCheckpoint, EditorPrefabExtractionDisposition, EditorWorkspaceCommandReport,
    EditorWorkspaceExtractionEncodingFailure, EncodedEditorPersistenceCheckpoint,
};

use super::*;

mod checkpoints;
pub(super) mod recovery;
pub(super) mod storage;

/// Physical originals keep their inventory index even when intervening guards have no output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectPrefabExtractionOutputRole {
    Existing {
        source: usize,
        document: Option<EditorDocumentId>,
    },
    ExtractedPrefab,
    ExtractedMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectPrefabExtractionOutputEncoding {
    Source(SceneFileEncoding),
    MetadataJson,
}

/// These facts describe the reviewed original, independently of the successor's encoded digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectPrefabExtractionOutputObservation {
    Existing {
        identity: FileIdentity,
        parent_identity: Option<FileIdentity>,
        digest: ContentDigest,
    },
    ObservedAbsent,
}

#[derive(Debug)]
enum Payload {
    Open(EncodedEditorPersistenceCheckpoint),
    Source(EncodedPrefabExtractionSource),
    Metadata(Vec<u8>),
    // Explicit cancellation releases encoding before returning the exact nominal checkpoint.
    // A refused return retains it here; preparation cannot resume after release has started.
    Returning(EditorPersistenceCheckpoint),
    // The exact encoded checkpoint moved into the concrete file result's acceptance state.
    // Its fixed bytes remain in the already published immutable recovery package.
    Transferred,
}

impl Payload {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Open(encoded) => Some(encoded.bytes()),
            Self::Source(encoded) => Some(encoded.bytes()),
            Self::Metadata(bytes) => Some(bytes),
            Self::Returning(_) | Self::Transferred => None,
        }
    }
}

#[derive(Debug)]
struct OutputMember {
    role: ProjectPrefabExtractionOutputRole,
    path: String,
    encoding: ProjectPrefabExtractionOutputEncoding,
    observation: ProjectPrefabExtractionOutputObservation,
    payload: Option<Payload>,
    digest: Option<ContentDigest>,
}

/// A borrowed view never transfers or reconstructs the sealed payload or its checkpoint.
pub(crate) struct ProjectPrefabExtractionOutput<'a> {
    member: &'a OutputMember,
    review: &'a PreparedProjectPrefabExtractionReview,
}

impl ProjectPrefabExtractionOutput<'_> {
    pub(crate) fn role(&self) -> ProjectPrefabExtractionOutputRole {
        self.member.role
    }

    pub(crate) fn path(&self) -> &str {
        &self.member.path
    }

    pub(crate) fn encoding(&self) -> ProjectPrefabExtractionOutputEncoding {
        self.member.encoding
    }

    pub(crate) fn observation(&self) -> ProjectPrefabExtractionOutputObservation {
        self.member.observation
    }

    pub(crate) fn source_key(&self) -> Option<&SourceDocumentKey> {
        match self.member.role {
            ProjectPrefabExtractionOutputRole::Existing { source, .. } => {
                Some(self.review.saved.sources()[source].key())
            }
            ProjectPrefabExtractionOutputRole::ExtractedPrefab => {
                Some(&self.review.context.destination.key)
            }
            ProjectPrefabExtractionOutputRole::ExtractedMetadata => None,
        }
    }

    pub(crate) fn bytes(&self) -> Option<&[u8]> {
        self.member.payload.as_ref()?.bytes()
    }

    pub(crate) fn digest(&self) -> Option<ContentDigest> {
        self.member.digest
    }

    pub(crate) fn encoded_checkpoint(&self) -> Option<&EncodedEditorPersistenceCheckpoint> {
        match self.member.payload.as_ref()? {
            Payload::Open(encoded) => Some(encoded),
            _ => None,
        }
    }

    pub(crate) fn checkpoint(&self) -> Option<&EditorPersistenceCheckpoint> {
        match self.member.payload.as_ref()? {
            Payload::Open(encoded) => Some(encoded.checkpoint()),
            Payload::Returning(checkpoint) => Some(checkpoint),
            _ => None,
        }
    }

    pub(crate) fn encoded_source(&self) -> Option<&EncodedPrefabExtractionSource> {
        match self.member.payload.as_ref()? {
            Payload::Source(encoded) => Some(encoded),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProjectPrefabExtractionOutputError {
    Project(PrefabExtractionProjectError),
    Workspace(EditorWorkspaceCommandReport),
    Open(EditorWorkspaceExtractionEncodingFailure),
    Source(SceneFileEncodingAdmissionError<ScenePatchReport>),
    Metadata(AssetMetaFormatError),
    Invalid(&'static str),
    #[cfg(test)]
    Injected,
}

impl From<AuthoringProjectSourceAdmissionError> for ProjectPrefabExtractionOutputError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Project(error.into())
    }
}

impl From<AuthoringProjectAdmissionError> for ProjectPrefabExtractionOutputError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Project(error.into())
    }
}

/// The complete Review remains the owner of disk guards, custody, saved proof and nominal Work.
#[must_use]
pub(crate) struct PreparedProjectPrefabExtractionOutputs {
    review: PreparedProjectPrefabExtractionReview,
    members: Vec<OutputMember>,
    initialized: bool,
    position: usize,
    release_position: Option<usize>,
    #[cfg(test)]
    refuse_once_before: Option<usize>,
}

impl fmt::Debug for PreparedProjectPrefabExtractionOutputs {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProjectPrefabExtractionOutputs")
            .field("review", &self.review)
            .field("members", &self.members.len())
            .field("position", &self.position)
            .field("release_position", &self.release_position)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
#[must_use]
pub(crate) struct ProjectPrefabExtractionOutputFailure {
    outputs: PreparedProjectPrefabExtractionOutputs,
    error: ProjectPrefabExtractionOutputError,
}

impl ProjectPrefabExtractionOutputFailure {
    pub(crate) fn error(&self) -> &ProjectPrefabExtractionOutputError {
        &self.error
    }

    pub(crate) fn outputs(&self) -> &PreparedProjectPrefabExtractionOutputs {
        &self.outputs
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn outputs_mut(
        &mut self,
    ) -> &mut PreparedProjectPrefabExtractionOutputs {
        &mut self.outputs
    }

    pub(crate) fn retry(self) -> Result<PreparedProjectPrefabExtractionOutputs, Self> {
        self.outputs.resume()
    }

    pub(crate) fn cancel(
        self,
    ) -> Result<PreparedProjectPrefabExtractionReview, ProjectPrefabExtractionOutputCancelFailure>
    {
        self.outputs.cancel()
    }
}

/// Release refusal retains all unreturned checkpoints and the Review's accepted evidence.
/// Only release can continue after encoded checkpoint bytes have explicitly been discarded.
#[derive(Debug)]
#[must_use]
pub(crate) struct ProjectPrefabExtractionOutputCancelFailure {
    outputs: PreparedProjectPrefabExtractionOutputs,
    error: ProjectPrefabExtractionOutputError,
}

impl ProjectPrefabExtractionOutputCancelFailure {
    pub(crate) fn error(&self) -> &ProjectPrefabExtractionOutputError {
        &self.error
    }

    pub(crate) fn outputs(&self) -> &PreparedProjectPrefabExtractionOutputs {
        &self.outputs
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn outputs_mut(
        &mut self,
    ) -> &mut PreparedProjectPrefabExtractionOutputs {
        &mut self.outputs
    }

    pub(crate) fn retry(self) -> Result<PreparedProjectPrefabExtractionReview, Self> {
        self.outputs.cancel()
    }
}

impl PreparedProjectPrefabExtractionReview {
    pub(crate) fn prepare_outputs(
        self,
    ) -> Result<PreparedProjectPrefabExtractionOutputs, ProjectPrefabExtractionOutputFailure> {
        PreparedProjectPrefabExtractionOutputs::new(self).resume()
    }

    #[cfg(test)]
    pub(super) fn test_prepare_outputs_refusing_before(
        self,
        position: usize,
    ) -> Result<PreparedProjectPrefabExtractionOutputs, ProjectPrefabExtractionOutputFailure> {
        let mut outputs = PreparedProjectPrefabExtractionOutputs::new(self);
        outputs.refuse_once_before = Some(position);
        outputs.resume()
    }
}

impl PreparedProjectPrefabExtractionOutputs {
    fn new(review: PreparedProjectPrefabExtractionReview) -> Self {
        Self {
            review,
            members: Vec::new(),
            initialized: false,
            position: 0,
            release_position: None,
            #[cfg(test)]
            refuse_once_before: None,
        }
    }

    pub(crate) fn review(&self) -> &PreparedProjectPrefabExtractionReview {
        &self.review
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context_mut(
        &mut self,
    ) -> &mut ProjectExtractionContext {
        &mut self.review.context
    }

    pub(crate) fn position(&self) -> usize {
        self.position
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.initialized && self.position == self.members.len() && self.release_position.is_none()
    }

    pub(crate) fn outputs(
        &self,
    ) -> impl ExactSizeIterator<Item = ProjectPrefabExtractionOutput<'_>> + '_ {
        self.members
            .iter()
            .map(|member| ProjectPrefabExtractionOutput {
                member,
                review: &self.review,
            })
    }

    pub(crate) fn require_saved_generation(&mut self) -> Result<(), EditorWorkspaceCommandReport> {
        self.review.require_saved_generation()
    }

    pub(crate) fn revalidate_project(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.review.revalidate_project(project)
    }

    fn resume(mut self) -> Result<Self, ProjectPrefabExtractionOutputFailure> {
        match self.advance() {
            Ok(()) => Ok(self),
            Err(error) => Err(ProjectPrefabExtractionOutputFailure {
                outputs: self,
                error,
            }),
        }
    }

    fn admit(&mut self, bytes: Option<usize>) -> Result<(), ProjectPrefabExtractionOutputError> {
        self.review
            .persistence
            .admit_persistence_storage(bytes)
            .map_err(ProjectPrefabExtractionOutputError::Workspace)
    }

    fn initialize(&mut self) -> Result<(), ProjectPrefabExtractionOutputError> {
        let count = self.review.saved.sources().len();
        let bindings = self
            .review
            .persistence
            .sources()
            .prepared()
            .bindings()
            .len();
        // Admit both complete tables and all owner/refusal wrappers before allocating either.
        self.admit(Some(checked_sum([
            checked_product(count, size_of::<Option<OpenBinding>>())?,
            checked_product(checked_sum([count, 2])?, size_of::<OutputMember>())?,
            checked_product(bindings, size_of::<EditorPrefabExtractionCorpusBinding>())?,
            size_of::<Self>(),
            size_of::<ProjectPrefabExtractionOutputFailure>(),
            size_of::<ProjectPrefabExtractionOutputCancelFailure>(),
        ])?))?;
        self.review
            .require_saved_generation()
            .map_err(ProjectPrefabExtractionOutputError::Workspace)?;
        if self.review.context.inventory.participants().len() != count
            || self.review.context.disk.len() != count
            || self.review.saved.published().len() != count
        {
            return Err(ProjectPrefabExtractionOutputError::Invalid(
                "Output selection requires the complete physical inventory",
            ));
        }
        // The first admitted traversal computes the full path-copy and key-comparison bound.
        let mut path_bytes = checked_sum([
            self.review.context.destination.path.as_str().len(),
            self.review.context.destination.sidecar.as_str().len(),
        ])?;
        let mut comparison_bytes = 0;
        for (index, participant) in self.review.context.inventory.participants().enumerate() {
            comparison_bytes = checked_sum([
                comparison_bytes,
                participant.source_key().retained_bytes(),
                self.review.saved.sources()[index].key().retained_bytes(),
            ])?;
            if self.review.saved.published()[index] {
                path_bytes = checked_sum([path_bytes, participant.file().path().len()])?;
            }
        }
        self.admit(Some(checked_sum([path_bytes, comparison_bytes])?))?;

        let mut open = vec![None; count];
        let stage = &self.review.persistence;
        if stage.documents().len() != bindings {
            return Err(ProjectPrefabExtractionOutputError::Invalid(
                "Output bindings require the complete captured open set",
            ));
        }
        for (binding, (document, disposition)) in stage
            .sources()
            .prepared()
            .bindings()
            .iter()
            .zip(stage.documents())
        {
            let slot =
                open.get_mut(binding.source)
                    .ok_or(ProjectPrefabExtractionOutputError::Invalid(
                        "Open source is outside the inventory",
                    ))?;
            if binding.document != document || slot.is_some() {
                return Err(ProjectPrefabExtractionOutputError::Invalid(
                    "Open sources require distinct actual corpus bindings",
                ));
            }
            *slot = Some(OpenBinding {
                document,
                disposition,
            });
        }
        let published = self.review.saved.published();
        let count = checked_sum([published.iter().filter(|selected| **selected).count(), 2])?;
        let mut members = Vec::with_capacity(count);
        for (source, participant) in self.review.context.inventory.participants().enumerate() {
            if participant.source_key() != self.review.saved.sources()[source].key()
                || open[source].is_some_and(|binding| {
                    published[source]
                        == (binding.disposition == EditorPrefabExtractionDisposition::Guard)
                })
            {
                return Err(ProjectPrefabExtractionOutputError::Invalid(
                    "Every published original requires its exact reviewed output",
                ));
            }
            if !published[source] {
                continue;
            }
            let file = participant.file();
            members.push(OutputMember {
                role: ProjectPrefabExtractionOutputRole::Existing {
                    source,
                    document: open[source].map(|binding| binding.document),
                },
                path: file.path().to_owned(),
                encoding: ProjectPrefabExtractionOutputEncoding::Source(
                    match participant.encoding() {
                        SourceEncoding::Json => SceneFileEncoding::Json,
                        SourceEncoding::Ron => SceneFileEncoding::Ron,
                    },
                ),
                observation: ProjectPrefabExtractionOutputObservation::Existing {
                    identity: file.identity(),
                    parent_identity: file.parent_identity(),
                    digest: file.digest(),
                },
                payload: None,
                digest: None,
            });
        }
        let destination = &self.review.context.destination;
        members.push(OutputMember {
            role: ProjectPrefabExtractionOutputRole::ExtractedPrefab,
            path: destination.path.as_str().to_owned(),
            encoding: ProjectPrefabExtractionOutputEncoding::Source(destination.encoding),
            observation: ProjectPrefabExtractionOutputObservation::ObservedAbsent,
            payload: None,
            digest: None,
        });
        members.push(OutputMember {
            role: ProjectPrefabExtractionOutputRole::ExtractedMetadata,
            path: destination.sidecar.as_str().to_owned(),
            encoding: ProjectPrefabExtractionOutputEncoding::MetadataJson,
            observation: ProjectPrefabExtractionOutputObservation::ObservedAbsent,
            payload: None,
            digest: None,
        });
        self.members = members;
        self.initialized = true;
        Ok(())
    }

    fn advance(&mut self) -> Result<(), ProjectPrefabExtractionOutputError> {
        if self.release_position.is_some() {
            return Err(ProjectPrefabExtractionOutputError::Invalid(
                "Only cancellation can continue after checkpoint release has started",
            ));
        }
        if !self.initialized {
            self.initialize()?;
        }
        while self.position < self.members.len() {
            // Every attempt, including a refusal with retained bytes, spends original Work.
            self.admit(Some(size_of::<OutputMember>()))?;
            #[cfg(test)]
            if self.refuse_once_before == Some(self.position) {
                self.refuse_once_before = None;
                return Err(ProjectPrefabExtractionOutputError::Injected);
            }
            if self.members[self.position].payload.is_none() {
                self.encode_current()?;
            }
            if self.members[self.position].digest.is_none() {
                self.hash_current()?;
            }
            self.position += 1;
        }
        Ok(())
    }

    fn encode_current(&mut self) -> Result<(), ProjectPrefabExtractionOutputError> {
        let member = &self.members[self.position];
        let encoding = match member.encoding {
            ProjectPrefabExtractionOutputEncoding::Source(encoding) => encoding,
            ProjectPrefabExtractionOutputEncoding::MetadataJson => {
                let bytes = encode_metadata(&mut self.review.context)?;
                self.members[self.position].payload = Some(Payload::Metadata(bytes));
                return Ok(());
            }
        };
        if let ProjectPrefabExtractionOutputRole::Existing {
            document: Some(document),
            ..
        } = member.role
        {
            // The actual checkpoint encoder admits its digest immediately before hashing its
            // fixed bytes, through the same original Work used for canonical/codec admission.
            let encoded = self
                .review
                .persistence
                .encode_checkpoint(document, encoding)
                .map_err(ProjectPrefabExtractionOutputError::Open)?;
            self.members[self.position].digest = Some(encoded.digest());
            self.members[self.position].payload = Some(Payload::Open(encoded));
        } else {
            let selection = match member.role {
                ProjectPrefabExtractionOutputRole::Existing { source, .. } => {
                    PrefabExtractionSourceOutput::Existing(source)
                }
                ProjectPrefabExtractionOutputRole::ExtractedPrefab => {
                    PrefabExtractionSourceOutput::Extracted
                }
                ProjectPrefabExtractionOutputRole::ExtractedMetadata => {
                    return Err(ProjectPrefabExtractionOutputError::Invalid(
                        "Metadata requires its own fixed JSON encoder",
                    ));
                }
            };
            let limits = self.review.context.inventory.scene_limits();
            let encoded = self
                .review
                .persistence
                .encode_closed_source_output(selection, encoding, limits)
                .map_err(ProjectPrefabExtractionOutputError::Source)?;
            self.members[self.position].payload = Some(Payload::Source(encoded));
        }
        Ok(())
    }

    fn hash_current(&mut self) -> Result<(), ProjectPrefabExtractionOutputError> {
        let payload = self.members[self.position].payload.as_ref().ok_or(
            ProjectPrefabExtractionOutputError::Invalid("Hashing requires fixed encoded bytes"),
        )?;
        let bytes = payload
            .bytes()
            .ok_or(ProjectPrefabExtractionOutputError::Invalid(
                "A released checkpoint cannot resume output preparation",
            ))?
            .len();
        match payload {
            Payload::Source(_) => {
                self.admit(Some(checked_sum([bytes, size_of::<ContentDigest>()])?))?;
                let Some(Payload::Source(encoded)) = &self.members[self.position].payload else {
                    unreachable!("the same fixed payload remains retained")
                };
                self.review
                    .persistence
                    .require_encoded_source_output(encoded)
                    .map_err(ProjectPrefabExtractionOutputError::Workspace)?;
            }
            Payload::Metadata(_) => {
                self.review
                    .context
                    .budget
                    .charge_source_bytes(0, 0, bytes, 0)?;
            }
            _ => {
                return Err(ProjectPrefabExtractionOutputError::Invalid(
                    "Only sealed source or metadata bytes need a Host digest",
                ));
            }
        }
        let payload = self.members[self.position]
            .payload
            .as_ref()
            .expect("retained above");
        let digest = ContentDigest::of_bytes(
            payload
                .bytes()
                .expect("fixed encoded bytes were checked above"),
        );
        self.members[self.position].digest = Some(digest);
        Ok(())
    }

    /// Releases only this owner's in-memory encodings. Accepted evidence remains in the same
    /// Review; its existing revision gate still decides whether a save selection can change.
    pub(crate) fn cancel(
        mut self,
    ) -> Result<PreparedProjectPrefabExtractionReview, ProjectPrefabExtractionOutputCancelFailure>
    {
        let mut position = *self.release_position.get_or_insert(0);
        while position < self.members.len() {
            let payload = self.members[position].payload.take();
            let checkpoint = match payload {
                Some(Payload::Open(encoded)) => encoded.into_checkpoint(),
                Some(Payload::Returning(checkpoint)) => checkpoint,
                other => {
                    self.members[position].payload = other;
                    position += 1;
                    self.release_position = Some(position);
                    continue;
                }
            };
            if let Err(failure) = self.review.persistence.return_checkpoint(checkpoint) {
                let (checkpoint, report) = failure.into_parts();
                self.members[position].payload = Some(Payload::Returning(checkpoint));
                return Err(ProjectPrefabExtractionOutputCancelFailure {
                    outputs: self,
                    error: ProjectPrefabExtractionOutputError::Workspace(report),
                });
            }
            position += 1;
            self.release_position = Some(position);
        }
        for position in 0..self.members.len() {
            let Some(Payload::Metadata(bytes)) = &self.members[position].payload else {
                continue;
            };
            // Check while the entire metadata owner is still retained. Publish the reduced
            // live-storage count only after dropping the actual Vec; cumulative work stays spent.
            let Some(retained) = self
                .review
                .context
                .budget
                .retained_candidate_bytes
                .checked_sub(bytes.capacity())
            else {
                return Err(ProjectPrefabExtractionOutputCancelFailure {
                    outputs: self,
                    error: AuthoringProjectSourceAdmissionError::AccountingOverflow.into(),
                });
            };
            let payload = self.members[position].payload.take();
            drop(payload);
            self.review.context.budget.retained_candidate_bytes = retained;
        }
        Ok(self.review)
    }
}

#[derive(Clone, Copy)]
struct OpenBinding {
    document: EditorDocumentId,
    disposition: EditorPrefabExtractionDisposition,
}

fn encode_metadata(
    context: &mut ProjectExtractionContext,
) -> Result<Vec<u8>, ProjectPrefabExtractionOutputError> {
    let path_bytes = context.destination.metadata.path().as_str().len();
    // Prefab kind, UUID, unit settings and the canonical envelope have fixed structure. Six
    // bytes per path byte cover JSON escaping; 4096 covers the fixed envelope and shape scratch.
    let wire_bound = checked_sum([checked_product(path_bytes, 6)?, 4096])?;
    let reservation = checked_sum([checked_product(wire_bound, 4)?, path_bytes])?;
    let budget = &mut context.budget;
    let before = budget.retained_candidate_bytes;
    let admitted =
        budget.charge_source_bytes(0, wire_bound, checked_product(wire_bound, 8)?, reservation);
    if let Err(error) = admitted {
        let reserved = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(reserved, 0)?;
        return Err(error.into());
    }
    let result = AssetMeta::new(context.destination.metadata.clone(), ())
        .to_json_string()
        .map(String::into_bytes)
        .map_err(ProjectPrefabExtractionOutputError::Metadata);
    match result {
        Ok(bytes) if bytes.len() <= wire_bound && bytes.capacity() <= reservation => {
            budget.reconcile_retained_reservation(reservation, bytes.capacity())?;
            Ok(bytes)
        }
        Ok(bytes) => {
            drop(bytes);
            budget.reconcile_retained_reservation(reservation, 0)?;
            Err(ProjectPrefabExtractionOutputError::Invalid(
                "Metadata exceeded its admitted fixed Prefab envelope bound",
            ))
        }
        Err(error) => {
            budget.reconcile_retained_reservation(reservation, 0)?;
            Err(error)
        }
    }
}
