//! Runtime-owned file Hosts retain cleanup authority until explicit retirement.

use std::sync::{Arc, Mutex, TryLockError};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_ecs::Resource;

use crate::project_host::persistence::{
    FilePublicationHost, PrefabPersistenceHost, ScenePersistenceHost,
};

trait RetirableHost {
    fn retry_cleanup(&mut self) -> bool;
}

impl RetirableHost for ScenePersistenceHost {
    fn retry_cleanup(&mut self) -> bool {
        ScenePersistenceHost::retry_cleanup(self)
    }
}

impl RetirableHost for PrefabPersistenceHost {
    fn retry_cleanup(&mut self) -> bool {
        PrefabPersistenceHost::retry_cleanup(self)
    }
}

impl RetirableHost for FilePublicationHost {
    fn retry_cleanup(&mut self) -> bool {
        FilePublicationHost::retry_cleanup(self)
    }
}

/// Shares one concrete Host without duplicating its file or cleanup authority.
pub(in crate::project_host::runtime::editor) struct RetainedHost<H> {
    host: Arc<Mutex<Option<H>>>,
}

impl<H> Clone for RetainedHost<H> {
    fn clone(&self) -> Self {
        Self {
            host: Arc::clone(&self.host),
        }
    }
}

impl<H> RetainedHost<H> {
    fn new(host: H) -> Self {
        Self {
            host: Arc::new(Mutex::new(Some(host))),
        }
    }

    pub(in crate::project_host::runtime::editor) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.host, &other.host)
    }

    pub(in crate::project_host::runtime::editor) fn try_with<R>(
        &self,
        apply: impl FnOnce(&mut H) -> R,
    ) -> Option<R> {
        let mut host = match self.host.try_lock() {
            Ok(host) => host,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        host.as_mut().map(apply)
    }

    /// Workers may wait for the Host, but must never acquire custody while borrowing it.
    pub(in crate::project_host::runtime::editor) fn with_worker<R>(
        &self,
        apply: impl FnOnce(&mut H) -> R,
    ) -> Option<R> {
        self.host
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
            .map(apply)
    }

    fn is_retired(&self) -> bool {
        match self.host.try_lock() {
            Ok(host) => host.is_none(),
            Err(TryLockError::WouldBlock) => false,
            Err(TryLockError::Poisoned(error)) => error.into_inner().is_none(),
        }
    }
}

#[allow(private_bounds)]
impl<H: RetirableHost> RetainedHost<H> {
    fn retry_cleanup(&self) -> bool {
        let mut host = match self.host.try_lock() {
            Ok(host) => host,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        host.as_mut().is_none_or(RetirableHost::retry_cleanup)
    }

    /// Releases the Host only after its current borrower and retained cleanup finish.
    pub(in crate::project_host::runtime::editor) fn retire(&self) -> bool {
        let mut host = match self.host.try_lock() {
            Ok(host) => host,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        if host.as_mut().is_some_and(|host| !host.retry_cleanup()) {
            return false;
        }
        *host = None;
        true
    }
}

enum HostRegistration {
    Scene(RetainedHost<ScenePersistenceHost>),
    Prefab(RetainedHost<PrefabPersistenceHost>),
    Publication(RetainedHost<FilePublicationHost>),
}

impl HostRegistration {
    fn retry_cleanup(&self) -> bool {
        match self {
            Self::Scene(host) => host.retry_cleanup(),
            Self::Prefab(host) => host.retry_cleanup(),
            Self::Publication(host) => host.retry_cleanup(),
        }
    }

    fn is_retired(&self) -> bool {
        match self {
            Self::Scene(host) => host.is_retired(),
            Self::Prefab(host) => host.is_retired(),
            Self::Publication(host) => host.is_retired(),
        }
    }

    fn retire(&self) -> bool {
        match self {
            Self::Scene(host) => host.retire(),
            Self::Prefab(host) => host.retire(),
            Self::Publication(host) => host.retire(),
        }
    }
}

#[derive(Default)]
struct HostRegistry {
    closing: bool,
    hosts: Vec<HostRegistration>,
}

impl HostRegistry {
    fn prune(&mut self) {
        self.hosts.retain(|host| !host.is_retired());
    }
}

/// Holds every admitted file Host independently of any one persistence operation.
#[derive(Clone, Default, Resource)]
pub(in crate::project_host::runtime::editor) struct HostCustody {
    registry: Arc<Mutex<HostRegistry>>,
}

impl HostCustody {
    pub(in crate::project_host::runtime::editor) fn retain_scene(
        &self,
        host: ScenePersistenceHost,
    ) -> Option<RetainedHost<ScenePersistenceHost>> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if registry.closing {
            return None;
        }
        registry.prune();
        let host = RetainedHost::new(host);
        registry.hosts.push(HostRegistration::Scene(host.clone()));
        Some(host)
    }

    pub(in crate::project_host::runtime::editor) fn retain_prefab(
        &self,
        host: PrefabPersistenceHost,
    ) -> Option<RetainedHost<PrefabPersistenceHost>> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if registry.closing {
            return None;
        }
        registry.prune();
        let host = RetainedHost::new(host);
        registry.hosts.push(HostRegistration::Prefab(host.clone()));
        Some(host)
    }

    pub(in crate::project_host::runtime::editor) fn retain_publication(
        &self,
        host: FilePublicationHost,
    ) -> Option<RetainedHost<FilePublicationHost>> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if registry.closing {
            return None;
        }
        registry.prune();
        let host = RetainedHost::new(host);
        registry
            .hosts
            .push(HostRegistration::Publication(host.clone()));
        Some(host)
    }

    pub(in crate::project_host::runtime::editor) fn is_accepting(&self) -> bool {
        let registry = match self.registry.try_lock() {
            Ok(registry) => registry,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        !registry.closing
    }

    pub(in crate::project_host::runtime::editor) fn close_admission(&self) {
        self.registry
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closing = true;
    }

    /// Retries every registered Host without retiring a lane that the session still owns.
    pub(in crate::project_host::runtime::editor) fn retry_cleanup(&self) -> bool {
        let registry = match self.registry.try_lock() {
            Ok(registry) => registry,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let mut complete = true;
        for host in &registry.hosts {
            complete &= host.retry_cleanup();
        }
        complete
    }

    /// Transfers live registrations at the final, infallible runtime replacement boundary.
    ///
    /// The old participant becomes closed and empty; only the successor may retire the Hosts.
    pub(in crate::project_host::runtime::editor) fn transfer_to(&self, next: &Self) -> bool {
        if Arc::ptr_eq(&self.registry, &next.registry) {
            return false;
        }
        let mut current = match self.registry.try_lock() {
            Ok(registry) => registry,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let mut successor = match next.registry.try_lock() {
            Ok(registry) => registry,
            Err(TryLockError::WouldBlock) => return false,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        successor.prune();
        if current.closing || successor.closing || !successor.hosts.is_empty() {
            return false;
        }
        current.prune();
        successor.hosts = std::mem::take(&mut current.hosts);
        current.closing = true;
        true
    }

    fn poll_retirement(&self) -> RuntimeCloseProgress {
        let mut registry = match self.registry.try_lock() {
            Ok(registry) => registry,
            Err(TryLockError::WouldBlock) => return RuntimeCloseProgress::Pending,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        registry.closing = true;
        registry.hosts.retain(|host| !host.retire());
        if registry.hosts.is_empty() {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }
}

const HOST_CUSTODY_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.editor.persistence.host-custody");
const HOST_CUSTODY_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.editor.persistence.host-custody");
const HOST_CUSTODY_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("nara.editor.persistence.host-custody"),
    PluginCategory::Service,
)
.requires_plugins(&[nara_tasks::TASK_PLUGIN_ID])
.shutdown_obligations(&[HOST_CUSTODY_OBLIGATION]);

pub(in crate::project_host::runtime::editor) struct HostCustodyPlugin;

impl Plugin for HostCustodyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &HOST_CUSTODY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let custody = HostCustody::default();
        app.register_plugin_runtime_close_participant(
            HOST_CUSTODY_OBLIGATION,
            HOST_CUSTODY_PARTICIPANT,
            HostCustodyCloseParticipant(custody.clone()),
        )?;
        app.insert_resource(custody)?;
        Ok(())
    }
}

struct HostCustodyCloseParticipant(HostCustody);

impl RuntimeCloseParticipant for HostCustodyCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.close_admission();
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.0.poll_retirement())
    }
}
