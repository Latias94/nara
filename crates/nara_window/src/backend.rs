//! Backend window handle ownership and target retirement.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use nara_app::RuntimeDriverScope;
use nara_ecs::Resource;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use thiserror::Error;

use crate::WindowId;

trait WindowHandleSource: HasWindowHandle + HasDisplayHandle + Send + Sync {}

impl<T> WindowHandleSource for T where T: HasWindowHandle + HasDisplayHandle + Send + Sync {}

/// An owned source for native window and display handles.
///
/// The provider is consumed by [`BackendWindowHandles::insert`]. Backends obtain
/// a tracked, non-cloneable owner through [`BackendWindowHandles::acquire_surface`].
pub struct WindowHandleProvider {
    source: Arc<dyn WindowHandleSource>,
}

impl WindowHandleProvider {
    #[must_use]
    pub fn new<T>(source: Arc<T>) -> Self
    where
        T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static,
    {
        Self { source }
    }
}

impl fmt::Debug for WindowHandleProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowHandleProvider")
            .finish_non_exhaustive()
    }
}

/// A non-cloneable native handle owner consumed by a backend surface.
///
/// Dropping this value acknowledges that the concrete backend no longer owns a
/// surface for the target. The registry keeps the platform provider separately
/// until the platform adapter completes controlled retirement.
pub struct WindowSurfaceHandleSource {
    source: Arc<dyn WindowHandleSource>,
    authority: BackendWindowHandles,
    window_id: WindowId,
    generation: u64,
}

impl fmt::Debug for WindowSurfaceHandleSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowSurfaceHandleSource")
            .field("window_id", &self.window_id)
            .finish_non_exhaustive()
    }
}

impl HasWindowHandle for WindowSurfaceHandleSource {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.source.window_handle()
    }
}

impl HasDisplayHandle for WindowSurfaceHandleSource {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.source.display_handle()
    }
}

impl Drop for WindowSurfaceHandleSource {
    fn drop(&mut self) {
        let _ = self
            .authority
            .acknowledge_surface_dropped(self.window_id, self.generation);
    }
}

/// A renderer-owned lease bound to the target authority that activated a surface.
///
/// The lease remains tied to that authority even if an ECS resource containing
/// another [`BackendWindowHandles`] value later replaces it.
#[derive(Debug)]
#[must_use = "the lease must be held until its backend surface is dropped"]
pub struct WindowSurfaceLease {
    authority: BackendWindowHandles,
    window_id: WindowId,
    generation: u64,
}

impl WindowSurfaceLease {
    #[must_use]
    pub fn can_acquire_frame(&self) -> bool {
        self.authority
            .is_surface_binding_active(self.window_id, self.generation)
    }

    pub fn retirement_requested(&self) -> Result<bool, WindowTargetError> {
        self.authority
            .surface_retirement_requested(self.window_id, self.generation)
    }

    pub fn request_retirement(&self) -> Result<WindowTargetTransition, WindowTargetError> {
        self.authority
            .request_surface_retirement(self.window_id, self.generation)
    }

    pub fn confirm_owner_dropped(self) -> Result<(), WindowTargetError> {
        self.authority
            .confirm_surface_owner_dropped(self.window_id, self.generation)
    }
}

/// The two capabilities produced by one atomic surface acquisition.
#[derive(Debug)]
#[must_use = "the handle source and lease must be handed to the backend surface owner"]
pub struct WindowSurfaceBinding {
    handle_source: WindowSurfaceHandleSource,
    lease: WindowSurfaceLease,
}

impl WindowSurfaceBinding {
    pub fn into_parts(self) -> (WindowSurfaceHandleSource, WindowSurfaceLease) {
        (self.handle_source, self.lease)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTargetPhase {
    Active,
    RetireRequested,
    SurfaceRetired,
    ProviderReleased,
    NativeDestroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTargetFault {
    ExternallyDestroyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTargetTransition {
    Applied,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowTargetSnapshot {
    pub phase: WindowTargetPhase,
    pub fault: Option<WindowTargetFault>,
    pub surface_active: bool,
    pub provider_present: bool,
    pub native_destroyed: bool,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum WindowTargetError {
    #[error("window target {window_id:?} is not registered")]
    UnknownWindow { window_id: WindowId },
    #[error("window target {window_id:?} is already registered")]
    DuplicateWindow { window_id: WindowId },
    #[error("window target {window_id:?} cannot activate a surface after retirement started")]
    SurfaceActivationAfterRetirement { window_id: WindowId },
    #[error("window target {window_id:?} already has an active surface lease")]
    SurfaceAlreadyActive { window_id: WindowId },
    #[error("window target {window_id:?} still has an active native surface owner")]
    SurfaceOwnerStillActive { window_id: WindowId },
    #[error("window target {window_id:?} surface binding is stale")]
    StaleSurfaceBinding { window_id: WindowId },
    #[error("window target {window_id:?} exhausted its surface binding generations")]
    SurfaceGenerationExhausted { window_id: WindowId },
    #[error("window target {window_id:?} cannot release its provider before surface retirement")]
    ProviderReleaseBeforeSurfaceRetirement { window_id: WindowId },
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum WindowSurfaceRetirementError {
    #[error("window surface retirement driver {driver} failed")]
    DriverFailed { driver: &'static str },
}

/// Backend-neutral bridge used by a platform adapter to retire selected surfaces.
#[derive(Clone, Copy, Resource)]
pub struct WindowSurfaceRetirementDriver {
    driver: &'static str,
    retire:
        fn(&mut RuntimeDriverScope<'_>, &[WindowId]) -> Result<(), WindowSurfaceRetirementError>,
}

impl WindowSurfaceRetirementDriver {
    #[must_use]
    pub const fn new(
        driver: &'static str,
        retire: fn(
            &mut RuntimeDriverScope<'_>,
            &[WindowId],
        ) -> Result<(), WindowSurfaceRetirementError>,
    ) -> Self {
        Self { driver, retire }
    }

    #[must_use]
    pub const fn driver(self) -> &'static str {
        self.driver
    }

    pub fn retire_targets(
        self,
        scope: &mut RuntimeDriverScope<'_>,
        window_ids: &[WindowId],
    ) -> Result<(), WindowSurfaceRetirementError> {
        (self.retire)(scope, window_ids)
    }
}

impl fmt::Debug for WindowSurfaceRetirementDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowSurfaceRetirementDriver")
            .field("driver", &self.driver)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct BackendWindowEntry {
    provider: Option<WindowHandleProvider>,
    phase: WindowTargetPhase,
    fault: Option<WindowTargetFault>,
    last_surface_generation: u64,
    active_surface_generation: Option<u64>,
}

impl BackendWindowEntry {
    fn new(provider: WindowHandleProvider) -> Self {
        Self {
            provider: Some(provider),
            phase: WindowTargetPhase::Active,
            fault: None,
            last_surface_generation: 0,
            active_surface_generation: None,
        }
    }

    fn native_destroyed(&self) -> bool {
        self.phase == WindowTargetPhase::NativeDestroyed
            || self.fault == Some(WindowTargetFault::ExternallyDestroyed)
    }

    fn snapshot(&self) -> WindowTargetSnapshot {
        WindowTargetSnapshot {
            phase: self.phase,
            fault: self.fault,
            surface_active: self.active_surface_generation.is_some(),
            provider_present: self.provider.is_some(),
            native_destroyed: self.native_destroyed(),
        }
    }

    fn request_retirement(&mut self) -> WindowTargetTransition {
        if self.phase != WindowTargetPhase::Active {
            return WindowTargetTransition::Unchanged;
        }
        self.phase = if self.active_surface_generation.is_some() {
            WindowTargetPhase::RetireRequested
        } else {
            WindowTargetPhase::SurfaceRetired
        };
        WindowTargetTransition::Applied
    }
}

#[derive(Debug, Default)]
struct BackendWindowRegistry {
    entries: BTreeMap<WindowId, BackendWindowEntry>,
}

/// Shared authority for provider ownership and renderer-acknowledged retirement.
///
/// The cloned authority may outlive ordinary `App` mutation so the platform
/// runner can record native destruction after plugin cleanup has completed.
/// Surface exclusivity is scoped to one authority and `WindowId`; platform
/// hosts must register each native target once in the authority shared with its
/// renderer rather than constructing independent registries for the same target.
#[derive(Clone, Default, Resource)]
pub struct BackendWindowHandles {
    registry: Arc<Mutex<BackendWindowRegistry>>,
}

impl fmt::Debug for BackendWindowHandles {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackendWindowHandles")
            .field("targets", &self.lock().entries.len())
            .finish()
    }
}

impl BackendWindowHandles {
    fn lock(&self) -> MutexGuard<'_, BackendWindowRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn insert(
        &self,
        window_id: WindowId,
        provider: WindowHandleProvider,
    ) -> Result<(), WindowTargetError> {
        let mut registry = self.lock();
        match registry.entries.entry(window_id) {
            Entry::Vacant(entry) => {
                entry.insert(BackendWindowEntry::new(provider));
                Ok(())
            }
            Entry::Occupied(_) => Err(WindowTargetError::DuplicateWindow { window_id }),
        }
    }

    #[must_use]
    pub fn is_registered(&self, window_id: WindowId) -> bool {
        self.lock().entries.contains_key(&window_id)
    }

    #[must_use]
    pub fn is_surface_target_active(&self, window_id: WindowId) -> bool {
        self.lock().entries.get(&window_id).is_some_and(|entry| {
            entry.phase == WindowTargetPhase::Active
                && entry.fault.is_none()
                && entry.provider.is_some()
        })
    }

    fn is_surface_binding_active(&self, window_id: WindowId, generation: u64) -> bool {
        self.lock().entries.get(&window_id).is_some_and(|entry| {
            entry.phase == WindowTargetPhase::Active
                && entry.fault.is_none()
                && entry.provider.is_some()
                && entry.active_surface_generation == Some(generation)
        })
    }

    pub fn snapshot(&self, window_id: WindowId) -> Result<WindowTargetSnapshot, WindowTargetError> {
        self.lock()
            .entries
            .get(&window_id)
            .map(BackendWindowEntry::snapshot)
            .ok_or(WindowTargetError::UnknownWindow { window_id })
    }

    pub fn acquire_surface(
        &self,
        window_id: WindowId,
    ) -> Result<WindowSurfaceBinding, WindowTargetError> {
        let mut registry = self.lock();
        let entry = registry
            .entries
            .get_mut(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        if entry.phase != WindowTargetPhase::Active || entry.fault.is_some() {
            return Err(WindowTargetError::SurfaceActivationAfterRetirement { window_id });
        }
        if entry.active_surface_generation.is_some() {
            return Err(WindowTargetError::SurfaceAlreadyActive { window_id });
        }
        let generation = entry
            .last_surface_generation
            .checked_add(1)
            .ok_or(WindowTargetError::SurfaceGenerationExhausted { window_id })?;
        let source = Arc::clone(
            &entry
                .provider
                .as_ref()
                .ok_or(WindowTargetError::SurfaceActivationAfterRetirement { window_id })?
                .source,
        );
        entry.last_surface_generation = generation;
        entry.active_surface_generation = Some(generation);
        Ok(WindowSurfaceBinding {
            handle_source: WindowSurfaceHandleSource {
                source,
                authority: self.clone(),
                window_id,
                generation,
            },
            lease: WindowSurfaceLease {
                authority: self.clone(),
                window_id,
                generation,
            },
        })
    }

    pub fn request_retirement(
        &self,
        window_id: WindowId,
    ) -> Result<WindowTargetTransition, WindowTargetError> {
        let mut registry = self.lock();
        let entry = registry
            .entries
            .get_mut(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        Ok(entry.request_retirement())
    }

    fn request_surface_retirement(
        &self,
        window_id: WindowId,
        generation: u64,
    ) -> Result<WindowTargetTransition, WindowTargetError> {
        let mut registry = self.lock();
        let entry = registry
            .entries
            .get_mut(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        Self::validate_surface_generation(entry, window_id, generation)?;
        Ok(entry.request_retirement())
    }

    fn surface_retirement_requested(
        &self,
        window_id: WindowId,
        generation: u64,
    ) -> Result<bool, WindowTargetError> {
        let registry = self.lock();
        let entry = registry
            .entries
            .get(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        Self::validate_surface_generation(entry, window_id, generation)?;
        Ok(entry.phase == WindowTargetPhase::RetireRequested
            && entry.active_surface_generation == Some(generation))
    }

    pub fn request_retirements(&self, window_ids: &[WindowId]) -> Result<(), WindowTargetError> {
        let mut registry = self.lock();
        if let Some(window_id) = window_ids
            .iter()
            .find(|window_id| !registry.entries.contains_key(window_id))
        {
            return Err(WindowTargetError::UnknownWindow {
                window_id: *window_id,
            });
        }
        for window_id in window_ids {
            registry
                .entries
                .get_mut(window_id)
                .ok_or(WindowTargetError::UnknownWindow {
                    window_id: *window_id,
                })?
                .request_retirement();
        }
        Ok(())
    }

    fn acknowledge_surface_dropped(
        &self,
        window_id: WindowId,
        generation: u64,
    ) -> Result<WindowTargetTransition, WindowTargetError> {
        let mut registry = self.lock();
        let entry = registry
            .entries
            .get_mut(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        Self::validate_surface_generation(entry, window_id, generation)?;
        if entry.active_surface_generation.is_none() {
            return Ok(WindowTargetTransition::Unchanged);
        }
        entry.active_surface_generation = None;
        if entry.phase == WindowTargetPhase::RetireRequested {
            entry.phase = WindowTargetPhase::SurfaceRetired;
        }
        Ok(WindowTargetTransition::Applied)
    }

    fn confirm_surface_owner_dropped(
        &self,
        window_id: WindowId,
        generation: u64,
    ) -> Result<(), WindowTargetError> {
        let registry = self.lock();
        let entry = registry
            .entries
            .get(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        Self::validate_surface_generation(entry, window_id, generation)?;
        if entry.active_surface_generation == Some(generation) {
            Err(WindowTargetError::SurfaceOwnerStillActive { window_id })
        } else {
            Ok(())
        }
    }

    fn validate_surface_generation(
        entry: &BackendWindowEntry,
        window_id: WindowId,
        generation: u64,
    ) -> Result<(), WindowTargetError> {
        if entry.last_surface_generation == generation {
            Ok(())
        } else {
            Err(WindowTargetError::StaleSurfaceBinding { window_id })
        }
    }

    pub fn release_provider(
        &self,
        window_id: WindowId,
    ) -> Result<WindowTargetTransition, WindowTargetError> {
        let provider = {
            let mut registry = self.lock();
            let entry = registry
                .entries
                .get_mut(&window_id)
                .ok_or(WindowTargetError::UnknownWindow { window_id })?;
            match entry.phase {
                WindowTargetPhase::SurfaceRetired => {
                    let provider = entry.provider.take();
                    entry.phase = if entry.native_destroyed() {
                        WindowTargetPhase::NativeDestroyed
                    } else {
                        WindowTargetPhase::ProviderReleased
                    };
                    provider
                }
                WindowTargetPhase::ProviderReleased | WindowTargetPhase::NativeDestroyed => {
                    return Ok(WindowTargetTransition::Unchanged);
                }
                WindowTargetPhase::Active | WindowTargetPhase::RetireRequested => {
                    return Err(WindowTargetError::ProviderReleaseBeforeSurfaceRetirement {
                        window_id,
                    });
                }
            }
        };
        drop(provider);
        Ok(WindowTargetTransition::Applied)
    }

    pub fn release_retired_providers(
        &self,
        window_ids: impl IntoIterator<Item = WindowId>,
    ) -> Result<(), WindowTargetError> {
        let mut first_error = None;
        for window_id in window_ids {
            if let Err(error) = self.release_provider(window_id) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn all_native_destroyed(
        &self,
        window_ids: impl IntoIterator<Item = WindowId>,
    ) -> Result<bool, WindowTargetError> {
        let registry = self.lock();
        for window_id in window_ids {
            let entry = registry
                .entries
                .get(&window_id)
                .ok_or(WindowTargetError::UnknownWindow { window_id })?;
            if !entry.native_destroyed() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn mark_native_destroyed(
        &self,
        window_id: WindowId,
    ) -> Result<WindowTargetTransition, WindowTargetError> {
        let mut registry = self.lock();
        let entry = registry
            .entries
            .get_mut(&window_id)
            .ok_or(WindowTargetError::UnknownWindow { window_id })?;
        if entry.native_destroyed() {
            return Ok(WindowTargetTransition::Unchanged);
        }

        if entry.phase == WindowTargetPhase::ProviderReleased {
            entry.phase = WindowTargetPhase::NativeDestroyed;
        } else {
            entry.fault = Some(WindowTargetFault::ExternallyDestroyed);
            if entry.phase == WindowTargetPhase::Active {
                entry.phase = if entry.active_surface_generation.is_some() {
                    WindowTargetPhase::RetireRequested
                } else {
                    WindowTargetPhase::SurfaceRetired
                };
            }
        }
        Ok(WindowTargetTransition::Applied)
    }
}
