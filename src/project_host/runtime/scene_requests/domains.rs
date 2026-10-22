//! Synchronous native-domain participation for managed Project Scene travel.
//!
//! This module owns orchestration state. Domain implementations retain partial native progress in
//! the attempt returned from preparation. The Scene request owner advances unfinished participants
//! in registration order on the runtime owner thread;
//! it does not infer dependencies, provide rollback, or expose a shared event bus.

use std::collections::HashSet;

use nara_diagnostic::DiagnosticCode;
use nara_ecs::{Resource, World};
use nara_identity::SpawnedSceneInstance;
use nara_scene::advanced::SceneNativeRetirementClaims;

use super::PreparedProjectSceneView;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManagedSceneDomainId(&'static str);

impl ManagedSceneDomainId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedSceneDomainPhase {
    Retiring,
    Retired,
    Reconciling,
    Reconciled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("managed Scene domain operation failed: {}", .code.as_str())]
pub struct ManagedSceneDomainError {
    code: DiagnosticCode,
}

impl ManagedSceneDomainError {
    pub const fn new(code: DiagnosticCode) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        self.code
    }

    fn engine(code: &'static str) -> Self {
        Self::new(DiagnosticCode::new(code).expect("engine-owned domain diagnostic code is valid"))
    }
}

/// Claims and owned progress captured together for one managed travel.
#[must_use]
pub struct ManagedSceneDomainPreparation {
    claims: SceneNativeRetirementClaims,
    attempt: Box<dyn ManagedSceneDomainAttempt>,
}

impl ManagedSceneDomainPreparation {
    pub fn new(
        claims: SceneNativeRetirementClaims,
        attempt: impl ManagedSceneDomainAttempt + 'static,
    ) -> Self {
        Self {
            claims,
            attempt: Box::new(attempt),
        }
    }
}

/// One domain's owned native attempt. Implementations must retain any partially completed native
/// work and make repeated calls safe; the engine retries only the phase that has not completed.
///
/// Calls run synchronously on the runtime owner thread and must not block on I/O or wait for a
/// future frame. This protocol does not support pending asynchronous native work. `Send + Sync`
/// allows storage in the App's ECS resources; it does not authorize off-thread execution.
/// Domain plugin shutdown or its runtime close participant must clean up native state during Stop.
/// The attempt remains alive until World retirement, so shutdown must not depend on taking it
/// back from the private request owner.
pub trait ManagedSceneDomainAttempt: Send + Sync {
    fn retire(
        &mut self,
        world: &mut World,
        current: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError>;

    fn reconcile(
        &mut self,
        world: &mut World,
        published: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError>;
}

/// A registered native domain. Domain preparation observes the immutable target and World only;
/// all irreversible effects belong to the returned owned attempt.
/// Preparation must be nonblocking and leave current native state unchanged, including on error.
/// Return claims and the matching attempt together so both describe the same captured baseline.
pub trait ManagedSceneDomain: Send + Sync + 'static {
    fn id(&self) -> ManagedSceneDomainId;

    fn prepare(
        &self,
        world: &World,
        target: &PreparedProjectSceneView<'_>,
    ) -> Result<ManagedSceneDomainPreparation, ManagedSceneDomainError>;
}

struct PreparedDomainAttempt {
    id: ManagedSceneDomainId,
    claims: SceneNativeRetirementClaims,
    attempt: Box<dyn ManagedSceneDomainAttempt>,
    phase: ManagedSceneDomainPhase,
}

/// Owned attempt for one managed Scene travel. It is valid after native retirement begins and must
/// remain in the request slot until every phase completes or the runtime Stop path takes custody.
pub(crate) struct ManagedSceneTravelAttempt {
    domains: Vec<PreparedDomainAttempt>,
}

impl ManagedSceneTravelAttempt {
    pub fn prepare(
        registry: &ManagedSceneDomains,
        world: &World,
        target: &PreparedProjectSceneView<'_>,
    ) -> Result<Self, ManagedSceneDomainError> {
        let mut claimed = HashSet::new();
        let mut domains = Vec::with_capacity(registry.domains.len());
        for domain in &registry.domains {
            let prepared = domain.prepare(world, target)?;
            let ManagedSceneDomainPreparation { claims, attempt } = prepared;
            if claims
                .component_ids()
                .iter()
                .any(|component_id| !claimed.insert(*component_id))
            {
                return Err(ManagedSceneDomainError::engine(
                    "project.scene.domain-claim-conflict",
                ));
            }
            domains.push(PreparedDomainAttempt {
                id: domain.id(),
                claims,
                attempt,
                phase: ManagedSceneDomainPhase::Retiring,
            });
        }
        Ok(Self { domains })
    }

    /// Combines all participant claims for the Scene owner's one admission proof.
    pub fn combined_claims(&self) -> SceneNativeRetirementClaims {
        let mut claims = SceneNativeRetirementClaims::default();
        for domain in &self.domains {
            for component_id in domain.claims.component_ids().iter().copied() {
                claims.insert(component_id);
            }
        }
        claims
    }

    pub fn retire(
        &mut self,
        world: &mut World,
        current: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError> {
        for domain in &mut self.domains {
            if domain.phase != ManagedSceneDomainPhase::Retiring {
                continue;
            }
            domain.attempt.retire(world, current)?;
            domain.phase = ManagedSceneDomainPhase::Retired;
        }
        Ok(())
    }

    pub fn reconcile(
        &mut self,
        world: &mut World,
        published: &SpawnedSceneInstance,
    ) -> Result<(), ManagedSceneDomainError> {
        for domain in &mut self.domains {
            if domain.phase == ManagedSceneDomainPhase::Reconciled {
                continue;
            }
            if domain.phase == ManagedSceneDomainPhase::Retiring {
                return Err(ManagedSceneDomainError::engine(
                    "project.scene.domain-retirement-incomplete",
                ));
            }
            domain.phase = ManagedSceneDomainPhase::Reconciling;
            domain.attempt.reconcile(world, published)?;
            domain.phase = ManagedSceneDomainPhase::Reconciled;
        }
        Ok(())
    }

    pub fn domain_phases(
        &self,
    ) -> impl Iterator<Item = (ManagedSceneDomainId, ManagedSceneDomainPhase)> + '_ {
        self.domains.iter().map(|domain| (domain.id, domain.phase))
    }
}

/// App-owned registry of independent domain participants. Plugins install their participant during
/// build; each travel captures only the resulting owned native attempts.
#[derive(Resource, Default)]
pub struct ManagedSceneDomains {
    domains: Vec<Box<dyn ManagedSceneDomain>>,
}

impl ManagedSceneDomains {
    pub fn register(
        &mut self,
        domain: Box<dyn ManagedSceneDomain>,
    ) -> Result<(), ManagedSceneDomainError> {
        if self
            .domains
            .iter()
            .any(|existing| existing.id() == domain.id())
        {
            return Err(ManagedSceneDomainError::engine(
                "project.scene.domain-id-conflict",
            ));
        }
        self.domains.push(domain);
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Domain(ManagedSceneDomainId);

    impl ManagedSceneDomain for Domain {
        fn id(&self) -> ManagedSceneDomainId {
            self.0
        }

        fn prepare(
            &self,
            _world: &World,
            _target: &PreparedProjectSceneView<'_>,
        ) -> Result<ManagedSceneDomainPreparation, ManagedSceneDomainError> {
            Err(ManagedSceneDomainError::engine("test-unreachable"))
        }
    }

    #[test]
    fn registration_rejects_duplicate_domain_ids() {
        let mut domains = ManagedSceneDomains::default();
        domains
            .register(Box::new(Domain(ManagedSceneDomainId::new("a"))))
            .unwrap();
        assert_eq!(
            domains
                .register(Box::new(Domain(ManagedSceneDomainId::new("a"))))
                .unwrap_err()
                .code()
                .as_str(),
            "project.scene.domain-id-conflict"
        );
    }

    #[test]
    fn domain_errors_accept_only_validated_diagnostic_codes() {
        assert!(DiagnosticCode::new("domain error with spaces").is_err());
        let code = DiagnosticCode::new("consumer.domain.failed").unwrap();
        let error = ManagedSceneDomainError::new(code);
        assert_eq!(error.code(), code);
        let report = super::super::rejected_with_code(error.code(), "Native domain failed");
        assert_eq!(*report.diagnostics.iter().next().unwrap().code(), code);
    }
}
