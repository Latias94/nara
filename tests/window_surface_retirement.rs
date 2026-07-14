use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara::window::{
    WindowId,
    backend::{
        BackendWindowHandles, WindowHandleProvider, WindowSurfaceHandleSource, WindowSurfaceLease,
        WindowTargetError, WindowTargetFault, WindowTargetPhase, WindowTargetTransition,
    },
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

#[derive(Debug)]
struct TestWindowSource {
    drops: Arc<AtomicUsize>,
}

impl Drop for TestWindowSource {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl HasWindowHandle for TestWindowSource {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

impl HasDisplayHandle for TestWindowSource {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

fn provider(drops: &Arc<AtomicUsize>) -> WindowHandleProvider {
    WindowHandleProvider::new(Arc::new(TestWindowSource {
        drops: Arc::clone(drops),
    }))
}

fn acquire_surface(
    handles: &BackendWindowHandles,
    window_id: WindowId,
) -> (WindowSurfaceHandleSource, WindowSurfaceLease) {
    handles
        .acquire_surface(window_id)
        .expect("surface should activate")
        .into_parts()
}

#[test]
fn controlled_retirement_requires_surface_before_provider_and_native_target() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles
        .insert(WindowId::PRIMARY, provider(&drops))
        .expect("test provider should register");
    let (owner, lease) = acquire_surface(&handles, WindowId::PRIMARY);

    assert!(handles.release_provider(WindowId::PRIMARY).is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(
        handles
            .request_retirement(WindowId::PRIMARY)
            .expect("retirement should start"),
        WindowTargetTransition::Applied
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::RetireRequested
    );
    assert!(!handles.is_surface_target_active(WindowId::PRIMARY));

    drop(owner);
    lease
        .confirm_owner_dropped()
        .expect("renderer should observe the owner drop");
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::SurfaceRetired
    );
    handles
        .release_provider(WindowId::PRIMARY)
        .expect("provider should release after the surface");
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    handles
        .mark_native_destroyed(WindowId::PRIMARY)
        .expect("native target should finish last");

    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(snapshot.fault, None);
}

#[test]
fn repeated_controlled_transitions_are_idempotent() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();

    assert_eq!(
        handles.request_retirement(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Applied
    );
    assert_eq!(
        handles.request_retirement(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Unchanged
    );
    assert_eq!(
        handles.release_provider(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Applied
    );
    assert_eq!(
        handles.release_provider(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Unchanged
    );
    assert_eq!(
        handles.mark_native_destroyed(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Applied
    );
    assert_eq!(
        handles.mark_native_destroyed(WindowId::PRIMARY).unwrap(),
        WindowTargetTransition::Unchanged
    );
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn scoped_retirement_preflights_every_target_before_mutation() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();

    assert!(matches!(
        handles.request_retirements(&[WindowId::PRIMARY, WindowId::new(99)]),
        Err(WindowTargetError::UnknownWindow { window_id })
            if window_id == WindowId::new(99)
    ));
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::Active
    );
}

#[test]
fn activation_after_provider_only_retirement_fails_without_losing_the_provider() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    handles.request_retirement(WindowId::PRIMARY).unwrap();

    assert!(matches!(
        handles.acquire_surface(WindowId::PRIMARY),
        Err(
            nara::window::backend::WindowTargetError::SurfaceActivationAfterRetirement {
                window_id: WindowId::PRIMARY
            }
        )
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(
        handles
            .snapshot(WindowId::PRIMARY)
            .unwrap()
            .provider_present
    );

    handles.release_provider(WindowId::PRIMARY).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn a_window_target_issues_only_one_live_surface_lease() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    let (owner, lease) = acquire_surface(&handles, WindowId::PRIMARY);

    assert!(matches!(
        handles.acquire_surface(WindowId::PRIMARY),
        Err(WindowTargetError::SurfaceAlreadyActive {
            window_id: WindowId::PRIMARY
        })
    ));
    assert!(handles.snapshot(WindowId::PRIMARY).unwrap().surface_active);

    handles.request_retirement(WindowId::PRIMARY).unwrap();
    drop(owner);
    lease.confirm_owner_dropped().unwrap();
    handles.release_provider(WindowId::PRIMARY).unwrap();
}

#[test]
fn premature_native_destruction_is_sticky_and_disables_acquisition() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    let (owner, lease) = acquire_surface(&handles, WindowId::PRIMARY);

    handles
        .mark_native_destroyed(WindowId::PRIMARY)
        .expect("external destruction should be recorded");
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::RetireRequested);
    assert_eq!(snapshot.fault, Some(WindowTargetFault::ExternallyDestroyed));
    assert!(!handles.is_surface_target_active(WindowId::PRIMARY));
    assert!(handles.release_provider(WindowId::PRIMARY).is_err());

    drop(owner);
    lease.confirm_owner_dropped().unwrap();
    handles.release_provider(WindowId::PRIMARY).unwrap();
    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(snapshot.fault, Some(WindowTargetFault::ExternallyDestroyed));
}

#[test]
fn retiring_one_window_does_not_change_another_window() {
    let handles = BackendWindowHandles::default();
    let first = WindowId::PRIMARY;
    let second = WindowId::new(2);
    let first_drops = Arc::new(AtomicUsize::new(0));
    let second_drops = Arc::new(AtomicUsize::new(0));
    handles.insert(first, provider(&first_drops)).unwrap();
    handles.insert(second, provider(&second_drops)).unwrap();
    let (first_owner, first_lease) = acquire_surface(&handles, first);
    let _second_binding = handles.acquire_surface(second).unwrap();

    handles.request_retirement(first).unwrap();
    drop(first_owner);
    first_lease.confirm_owner_dropped().unwrap();
    handles.release_provider(first).unwrap();
    handles.mark_native_destroyed(first).unwrap();

    assert_eq!(
        handles.snapshot(first).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert_eq!(
        handles.snapshot(second).unwrap().phase,
        WindowTargetPhase::Active
    );
    assert!(handles.is_surface_target_active(second));
    assert_eq!(first_drops.load(Ordering::SeqCst), 1);
    assert_eq!(second_drops.load(Ordering::SeqCst), 0);
}

#[test]
fn surface_loss_keeps_a_live_provider_available_for_recreation() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    let (owner, lease) = acquire_surface(&handles, WindowId::PRIMARY);
    drop(owner);
    lease.confirm_owner_dropped().unwrap();

    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::Active);
    assert!(!snapshot.surface_active);
    assert!(snapshot.provider_present);
    assert!(handles.is_surface_target_active(WindowId::PRIMARY));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
}

#[test]
fn stale_lease_cannot_retire_a_replacement_surface() {
    let handles = BackendWindowHandles::default();
    let drops = Arc::new(AtomicUsize::new(0));
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    let (first_owner, first_lease) = acquire_surface(&handles, WindowId::PRIMARY);
    drop(first_owner);

    let replacement = handles.acquire_surface(WindowId::PRIMARY).unwrap();

    assert_eq!(
        first_lease.request_retirement(),
        Err(WindowTargetError::StaleSurfaceBinding {
            window_id: WindowId::PRIMARY,
        })
    );
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::Active);
    assert!(snapshot.surface_active);

    drop(replacement);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
}

#[test]
fn surface_owner_drop_is_tracked_without_releasing_the_registered_provider() {
    let drops = Arc::new(AtomicUsize::new(0));
    let handles = BackendWindowHandles::default();
    handles.insert(WindowId::PRIMARY, provider(&drops)).unwrap();
    let binding = handles.acquire_surface(WindowId::PRIMARY).unwrap();

    drop(binding);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::Active);
    assert!(!snapshot.surface_active);
    assert!(snapshot.provider_present);

    handles.request_retirement(WindowId::PRIMARY).unwrap();
    handles.release_provider(WindowId::PRIMARY).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn surface_lease_acknowledges_the_authority_that_created_it() {
    let original = BackendWindowHandles::default();
    let replacement = BackendWindowHandles::default();
    let original_drops = Arc::new(AtomicUsize::new(0));
    let replacement_drops = Arc::new(AtomicUsize::new(0));
    original
        .insert(WindowId::PRIMARY, provider(&original_drops))
        .unwrap();
    replacement
        .insert(WindowId::PRIMARY, provider(&replacement_drops))
        .unwrap();

    let (owner, lease) = acquire_surface(&original, WindowId::PRIMARY);
    original.request_retirement(WindowId::PRIMARY).unwrap();
    drop(owner);
    lease.confirm_owner_dropped().unwrap();

    assert_eq!(
        original.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::SurfaceRetired
    );
    assert_eq!(
        replacement.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::Active
    );
    assert!(
        !replacement
            .snapshot(WindowId::PRIMARY)
            .unwrap()
            .surface_active
    );
}
