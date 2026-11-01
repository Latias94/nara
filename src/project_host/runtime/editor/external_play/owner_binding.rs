//! Nominal custody for the Application's supervised Play owner.
//!
//! Root can verify the exact registered custodian and request. The custodian lives in another
//! crate and remains trusted to attest physical child, worker and generation-store cleanup.

use std::sync::Arc;

use nara_project::ProjectAuthoringRecoveryDigest;

use crate::project_host::authoring::AuthoringSchemaOwnerProjectInput;

use super::super::{EditorAuthoredSchemaUpgradeId, EditorProjectSession};

/// Authority issued once to the supervised Play custodian for a published project generation.
///
/// This value cannot be cloned or reconstructed from the generation label. The Application keeps
/// it with the actual published project and uses it for requests, observations and retirement.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorSupervisedPlayBinding {
    registration: Arc<Registration>,
}

#[derive(Debug)]
struct Registration {
    published_generation: u64,
}

pub(in crate::project_host::runtime::editor) struct SupervisedPlayOwnerRegistration {
    identity: Arc<Registration>,
    retirement: Option<Retirement>,
}

struct Retirement {
    request: Arc<RetirementIdentity>,
    acknowledged: bool,
}

#[derive(Debug)]
struct RetirementIdentity {
    registration: Arc<Registration>,
    operation: EditorAuthoredSchemaUpgradeId,
    recovery_digest: ProjectAuthoringRecoveryDigest,
    incumbent: AuthoringSchemaOwnerProjectInput,
}

/// The retained operation's exact request to its registered child-process custodian.
///
/// Repeated polling produces handles to the same nominal request, not newly admitted requests.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorSupervisedPlayRetirementRequest {
    identity: Arc<RetirementIdentity>,
}

/// A trusted custodian's attestation for one exact retained retirement request.
///
/// Only the original binding can issue it. This is not an independent Root observation of a child
/// process owned by the Editor crate; the concrete custodian must obey `attest_retirement`.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorSupervisedPlayRetirementAck {
    request: Arc<RetirementIdentity>,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditorSupervisedPlayOwnerError {
    #[error("the Session is closing")]
    Closing,
    #[error("a supervised Play owner is already registered for this Session")]
    AlreadyRegistered,
    #[error("the binding is not this Session's registered supervised Play owner")]
    ForeignOwner,
    #[error("the authored upgrade operation is not the current fenced operation")]
    Operation,
    #[error("retirement requires an incumbent Authoring project")]
    NotAuthoring,
    #[error("the retirement request does not match the retained request")]
    Request,
    #[error("the incumbent Schema or nominal basis changed before retirement completed")]
    Stale,
    #[error("the registered custodian has not completed retirement and replacement")]
    Pending,
    #[error("the retirement acknowledgment has already been accepted")]
    AlreadyAcknowledged,
}

impl EditorSupervisedPlayBinding {
    #[must_use]
    pub fn published_generation(&self) -> u64 {
        self.registration.published_generation
    }

    /// Checks ownership before a custodian starts closing its actual process owner.
    pub fn validate_retirement_request(
        &self,
        request: &EditorSupervisedPlayRetirementRequest,
    ) -> Result<(), EditorSupervisedPlayOwnerError> {
        if !Arc::ptr_eq(&self.registration, &request.identity.registration) {
            return Err(EditorSupervisedPlayOwnerError::ForeignOwner);
        }
        Ok(())
    }

    /// Attests physical return at the trusted child-custodian boundary.
    ///
    /// The concrete custodian MUST first finish its real Close, including build, preparation,
    /// child, pipe and generation-store cleanup, and install its prepared empty replacement.
    /// Neither Idle, an accepted Close/Stop nor a public state value proves that completion.
    /// Root verifies this binding and the exact retained request when accepting the result; it
    /// cannot inspect another crate's child process. This method does not release the write fence.
    pub fn attest_retirement(
        &self,
        request: EditorSupervisedPlayRetirementRequest,
    ) -> Result<EditorSupervisedPlayRetirementAck, EditorSupervisedPlayOwnerError> {
        self.validate_retirement_request(&request)?;
        Ok(EditorSupervisedPlayRetirementAck {
            request: request.identity,
        })
    }
}

impl EditorSupervisedPlayRetirementRequest {
    #[must_use]
    pub fn operation(&self) -> EditorAuthoredSchemaUpgradeId {
        self.identity.operation
    }

    #[must_use]
    pub fn recovery_digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.identity.recovery_digest
    }

    #[must_use]
    pub fn published_generation(&self) -> u64 {
        self.identity.registration.published_generation
    }

    /// Compares the actual retained request, including its nominal incumbent authority.
    #[must_use]
    pub fn is_same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
    }
}

impl EditorProjectSession {
    /// Attaches the actual Application custodian once when publishing this project generation.
    /// Dropping the binding does not detach or replace the registration.
    #[doc(hidden)]
    pub fn register_supervised_play_owner(
        &mut self,
        published_generation: u64,
    ) -> Result<EditorSupervisedPlayBinding, EditorSupervisedPlayOwnerError> {
        if !self.accepts_new_work() {
            return Err(EditorSupervisedPlayOwnerError::Closing);
        }
        if self.supervised_play_owner.is_some() {
            return Err(EditorSupervisedPlayOwnerError::AlreadyRegistered);
        }
        if self.authoring_operation_fenced() {
            return Err(EditorSupervisedPlayOwnerError::Operation);
        }
        let identity = Arc::new(Registration {
            published_generation,
        });
        self.supervised_play_owner = Some(SupervisedPlayOwnerRegistration {
            identity: Arc::clone(&identity),
            retirement: None,
        });
        Ok(EditorSupervisedPlayBinding {
            registration: identity,
        })
    }

    /// Issues one retained retirement request for the current fenced authored upgrade.
    /// An unattached Session has no external participant and cannot export external launches.
    #[doc(hidden)]
    pub fn begin_supervised_play_retirement(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
        recovery_digest: ProjectAuthoringRecoveryDigest,
    ) -> Result<(), EditorSupervisedPlayOwnerError> {
        self.validate_supervised_play_retirement_operation(operation)?;
        let Some(owner) = self.supervised_play_owner.as_ref() else {
            return Ok(());
        };
        let project = self
            .authoring_project()
            .ok_or(EditorSupervisedPlayOwnerError::NotAuthoring)?;
        if let Some(retirement) = owner.retirement.as_ref() {
            if retirement.request.operation != operation
                || retirement.request.recovery_digest != recovery_digest
            {
                return Err(EditorSupervisedPlayOwnerError::Request);
            }
            if !retirement.request.incumbent.matches(project) {
                return Err(EditorSupervisedPlayOwnerError::Stale);
            }
            return Ok(());
        }
        let request = Arc::new(RetirementIdentity {
            registration: Arc::clone(&owner.identity),
            operation,
            recovery_digest,
            incumbent: project.schema_owner_input(),
        });
        self.supervised_play_owner
            .as_mut()
            .expect("the owner remains registered during request construction")
            .retirement = Some(Retirement {
            request,
            acknowledged: false,
        });
        // A launch reserved before confirmation cannot escape after physical retirement starts.
        self.external_play_request = None;
        Ok(())
    }

    #[doc(hidden)]
    pub fn supervised_play_retirement_request(
        &self,
        binding: &EditorSupervisedPlayBinding,
    ) -> Result<Option<EditorSupervisedPlayRetirementRequest>, EditorSupervisedPlayOwnerError> {
        let owner = self.supervised_play_owner_for(binding)?;
        Ok(owner
            .retirement
            .as_ref()
            .filter(|retirement| !retirement.acknowledged)
            .map(|retirement| EditorSupervisedPlayRetirementRequest {
                identity: Arc::clone(&retirement.request),
            }))
    }

    /// Reports actual outstanding custodian work, independently of displayed Play state.
    #[must_use]
    #[doc(hidden)]
    pub fn supervised_play_retirement_pending(&self) -> bool {
        self.supervised_play_owner
            .as_ref()
            .and_then(|owner| owner.retirement.as_ref())
            .is_some_and(|retirement| !retirement.acknowledged)
    }

    #[doc(hidden)]
    pub fn apply_supervised_play_retirement_ack(
        &mut self,
        binding: &EditorSupervisedPlayBinding,
        acknowledgment: EditorSupervisedPlayRetirementAck,
    ) -> Result<(), EditorSupervisedPlayOwnerError> {
        let owner = self.supervised_play_owner_for(binding)?;
        let retirement = owner
            .retirement
            .as_ref()
            .ok_or(EditorSupervisedPlayOwnerError::Request)?;
        if !Arc::ptr_eq(&retirement.request, &acknowledgment.request) {
            return Err(EditorSupervisedPlayOwnerError::Request);
        }
        if retirement.acknowledged {
            return Err(EditorSupervisedPlayOwnerError::AlreadyAcknowledged);
        }
        self.validate_supervised_play_retirement_operation(retirement.request.operation)?;
        let project = self
            .authoring_project()
            .ok_or(EditorSupervisedPlayOwnerError::NotAuthoring)?;
        if !retirement.request.incumbent.matches(project) {
            return Err(EditorSupervisedPlayOwnerError::Stale);
        }
        self.supervised_play_owner
            .as_mut()
            .and_then(|owner| owner.retirement.as_mut())
            .expect("the validated retirement remains retained")
            .acknowledged = true;
        self.external_play_request = None;
        self.external_play_attempt = None;
        self.external_play_view = None;
        self.external_play_bound_source_digest = None;
        self.play_result = None;
        Ok(())
    }

    /// Retires an acknowledged request after coordinated adoption or pre-Pending cancellation.
    /// The operation owner must call this before releasing its source write fence.
    #[doc(hidden)]
    pub fn finish_supervised_play_retirement(
        &mut self,
    ) -> Result<(), EditorSupervisedPlayOwnerError> {
        let Some(owner) = self.supervised_play_owner.as_mut() else {
            return Ok(());
        };
        let Some(retirement) = owner.retirement.as_ref() else {
            return Ok(());
        };
        if !retirement.acknowledged {
            return Err(EditorSupervisedPlayOwnerError::Pending);
        }
        owner.retirement = None;
        Ok(())
    }

    pub(super) fn matches_supervised_play_binding(
        &self,
        binding: &EditorSupervisedPlayBinding,
    ) -> bool {
        self.supervised_play_owner_for(binding).is_ok()
    }

    pub(super) fn supervised_play_retirement_held(&self) -> bool {
        self.supervised_play_owner
            .as_ref()
            .is_some_and(|owner| owner.retirement.is_some())
    }

    fn supervised_play_owner_for(
        &self,
        binding: &EditorSupervisedPlayBinding,
    ) -> Result<&SupervisedPlayOwnerRegistration, EditorSupervisedPlayOwnerError> {
        self.supervised_play_owner
            .as_ref()
            .filter(|owner| Arc::ptr_eq(&owner.identity, &binding.registration))
            .ok_or(EditorSupervisedPlayOwnerError::ForeignOwner)
    }

    fn validate_supervised_play_retirement_operation(
        &self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorSupervisedPlayOwnerError> {
        let current = self.authored_schema_upgrade_view();
        if current.operation != Some(operation) || !current.writes_fenced {
            return Err(EditorSupervisedPlayOwnerError::Operation);
        }
        Ok(())
    }
}

#[cfg(all(test, any(windows, all(target_os = "linux", target_arch = "x86_64"))))]
mod tests;
