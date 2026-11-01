//! Bounded immutable inputs for whole-Session authored-upgrade preparation on a worker.

use super::*;

/// The original Scene and classification, detached from its live Session after admission.
///
/// The complete-set owner must retain every incumbent, including both history branches, before
/// capture. The original Session keeps its history; this input shares only the current immutable
/// source and classification and copies their exact observations. Its original allowance keeps
/// that complete incumbent charged throughout preparation and retries.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct SceneAuthoredSchemaUpgradeInput {
    input: OwnedUpgradeInput<SceneDocument>,
}

/// Nominal Prefab counterpart, sharing the actual original Prefab without a Scene projection.
#[doc(hidden)]
#[derive(Debug)]
#[must_use]
pub struct PrefabAuthoredSchemaUpgradeInput {
    input: OwnedUpgradeInput<PrefabDocument>,
}

#[derive(Debug)]
struct OwnedUpgradeInput<D> {
    document: Arc<D>,
    state: Arc<AuthoringDocumentState>,
    snapshot: UpgradeSourceSnapshot,
    retained: RetainedAuthoredSchemaUpgradeSource,
}

impl<D> OwnedUpgradeInput<D> {
    fn capture(
        source: UpgradeSource<'_, D>,
        retained: RetainedAuthoredSchemaUpgradeSource,
        kind: SourceDocumentKind,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> Result<Self, ScenePatchReport> {
        work.require_retained(
            &retained,
            source.snapshot.revision,
            &source.state.schema_basis(),
            kind,
        )?;
        work.begin_step()?;
        let result = (|| {
            // Wrapper, shared Arc handles and exact snapshot metadata are inline. Their shared
            // bodies/control blocks remain covered by the original incumbent admission. Admit
            // the entire new input before cloning either handle; no history or token is minted.
            let bytes = Some(size_of::<Self>());
            work.reserve("authored-upgrade-input-storage", bytes)?;
            work.retain_output(bytes)?;
            Ok(Self {
                document: Arc::clone(source.document),
                state: Arc::clone(source.state),
                snapshot: source.snapshot,
                retained,
            })
        })();
        // On refusal no input or newly retained output escaped; only this step's scratch ends.
        // The parent's complete-history reservation and cumulative work are never refunded.
        work.budget.end_step();
        result
    }

    fn borrowed(&self) -> UpgradeSource<'_, D> {
        UpgradeSource {
            document: &self.document,
            state: &self.state,
            snapshot: self.snapshot,
        }
    }
}

macro_rules! owned_upgrade_input {
    ($input:ident, $session:ident, $document:ty, $candidate:ty, $prepared:ty, $kind:ident) => {
        impl $session {
            /// Freezes exact source observations using the complete-set owner's retained token.
            ///
            /// All incumbents must already be charged to this same Work before capture begins.
            /// Capture admits its own metadata before sharing Arcs and never copies old history.
            #[doc(hidden)]
            pub fn capture_authored_schema_upgrade_input(
                &self,
                retained: RetainedAuthoredSchemaUpgradeSource,
                work: &mut AuthoredSchemaUpgradeWork,
            ) -> Result<$input, ScenePatchReport> {
                let source =
                    UpgradeSource::from_session(&self.document, &self.authority, &self.history)?;
                Ok($input {
                    input: OwnedUpgradeInput::capture(
                        source,
                        retained,
                        SourceDocumentKind::$kind,
                        work,
                    )?,
                })
            }
        }

        impl $input {
            pub fn document(&self) -> &$document {
                &self.input.document
            }

            pub fn shared_document(&self) -> Arc<$document> {
                Arc::clone(&self.input.document)
            }

            pub fn revision(&self) -> SceneAuthoringRevision {
                self.input.snapshot.revision
            }

            pub fn schema_basis(&self) -> AuthoringSchemaBasis {
                self.input.state.schema_basis()
            }

            pub fn source(&self) -> &AuthoringDocumentSource {
                self.input.state.source()
            }

            pub fn file_limits(&self) -> SceneFileLimits {
                self.input.state.file_limits()
            }

            pub fn history_limits(&self) -> SceneAuthoringHistoryLimits {
                self.input.snapshot.history_limits
            }

            pub fn content_state(&self) -> SceneAuthoringContentState {
                self.input.snapshot.content_state
            }

            /// Prepares through the same source-owned kernel as direct Session preparation.
            ///
            /// This borrows the original token and admits no second incumbent. The enclosing
            /// owner must drop refused outputs inside `work.with_preparation_attempt`; the input
            /// and its pre-entry complete-history retention remain valid for another attempt.
            /// Final installation still binds the exact original live Session observations.
            pub fn prepare(
                &self,
                source_key: &SourceDocumentKey,
                candidate: $candidate,
                upgrade: &AuthoredFieldUpgrade,
                work: &mut AuthoredSchemaUpgradeWork,
            ) -> Result<$prepared, ScenePatchReport> {
                work.require_retained(
                    &self.input.retained,
                    self.revision(),
                    upgrade.previous_basis(),
                    SourceDocumentKind::$kind,
                )?;
                self.input.borrowed().prepare_with_work(
                    source_key,
                    self.revision(),
                    candidate,
                    upgrade,
                    work,
                )
            }
        }
    };
}

owned_upgrade_input!(
    SceneAuthoredSchemaUpgradeInput,
    SceneAuthoringSession,
    SceneDocument,
    SceneDocumentCandidate,
    PreparedSceneAuthoredSchemaUpgrade,
    Scene
);
owned_upgrade_input!(
    PrefabAuthoredSchemaUpgradeInput,
    PrefabAuthoringSession,
    PrefabDocument,
    PrefabDocumentCandidate,
    PreparedPrefabAuthoredSchemaUpgrade,
    Prefab
);
