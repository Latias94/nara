//! Fixed canonical outputs issued from this extraction's actual immutable source construction.

use crate::{SceneFileEncoding, SceneFileEncodingAdmissionError, prefab::SourceDocumentInput};

use super::*;

/// Selects an actual verified physical source, independently of whether it has an open Session.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExtractionSourceOutput {
    Existing(usize),
    Extracted,
}

/// Canonical bytes associated with one verified extraction and its selected source.
/// This owner grants no filesystem receipt, saved-state acknowledgment or Ready authority.
#[doc(hidden)]
#[must_use]
pub struct EncodedPrefabExtractionSource {
    source: PrefabExtractionSourceOutput,
    bytes: Vec<u8>,
    work: Arc<ExtractionWorkIdentity>,
}

impl fmt::Debug for EncodedPrefabExtractionSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedPrefabExtractionSource")
            .field("source", &self.source)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl EncodedPrefabExtractionSource {
    pub const fn source(&self) -> PrefabExtractionSourceOutput {
        self.source
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl PreparedPrefabExtraction {
    /// Encodes an actual reviewed working input for recovery data, including unchanged guards.
    /// These bytes are data only; they cannot stand in for an issued publication output.
    pub fn encode_original_source(
        &mut self,
        source: usize,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
    ) -> Result<Vec<u8>, SceneFileEncodingAdmissionError<ScenePatchReport>> {
        use SceneFileEncodingAdmissionError::Admission;

        self.work
            .admit_capture_storage(Some(size_of::<Vec<u8>>()))
            .map_err(Admission)?;
        let source = self
            .corpus
            .original()
            .sources
            .get(source)
            .ok_or_else(|| Admission(stale()))?;
        encode_source(source_input(source), encoding, limits, &mut self.work)
    }

    /// Encodes the selected immutable output without creating another source, Session or history.
    /// Every retry debits the original Work, including raw failed prefixes and buffer growth.
    pub fn encode_source_output(
        &mut self,
        source: PrefabExtractionSourceOutput,
        encoding: SceneFileEncoding,
        limits: SceneFileLimits,
    ) -> Result<EncodedPrefabExtractionSource, SceneFileEncodingAdmissionError<ScenePatchReport>>
    {
        use SceneFileEncodingAdmissionError::Admission;

        self.work
            .admit_capture_storage(Some(size_of::<EncodedPrefabExtractionSource>()))
            .map_err(Admission)?;
        let input = match source {
            PrefabExtractionSourceOutput::Existing(index) => source_input(
                self.corpus
                    .sources()
                    .get(index)
                    .ok_or_else(|| Admission(stale()))?,
            ),
            PrefabExtractionSourceOutput::Extracted => {
                SourceDocumentInput::Prefab(self.corpus.extracted().document())
            }
        };
        let bytes = encode_source(input, encoding, limits, &mut self.work)?;
        Ok(EncodedPrefabExtractionSource {
            source,
            bytes,
            work: Arc::clone(&self.work.identity),
        })
    }

    /// Matching bytes from another operation do not replace this operation's issued output.
    pub fn require_encoded_source_output(
        &mut self,
        output: &EncodedPrefabExtractionSource,
    ) -> Result<(), ScenePatchReport> {
        self.work
            .admit_capture_storage(Some(size_of::<EncodedPrefabExtractionSource>()))?;
        if !Arc::ptr_eq(&self.work.identity, &output.work) {
            return Err(stale());
        }
        Ok(())
    }
}

fn source_input(source: &CapturedExtractionSource) -> SourceDocumentInput<'_> {
    match source {
        CapturedExtractionSource::Scene { document, .. } => SourceDocumentInput::Scene(document),
        CapturedExtractionSource::Prefab { source } => {
            SourceDocumentInput::Prefab(source.document())
        }
    }
}

fn encode_source(
    input: SourceDocumentInput<'_>,
    encoding: SceneFileEncoding,
    limits: SceneFileLimits,
    work: &mut PrefabExtractionWork,
) -> Result<Vec<u8>, SceneFileEncodingAdmissionError<ScenePatchReport>> {
    work.kernel
        .admit_source_encoding(input)
        .map_err(|error| SceneFileEncodingAdmissionError::Admission(refusal_report(error).0))?;
    let mut admission = |capacity| work.admit_capture_storage(Some(capacity));
    let encoded = match input {
        SourceDocumentInput::Scene(document) => {
            document.encode_with_admission(encoding, limits, &mut admission)
        }
        SourceDocumentInput::Prefab(document) => {
            document.encode_with_admission(encoding, limits, &mut admission)
        }
    }?;
    Ok(encoded.into_bytes())
}
