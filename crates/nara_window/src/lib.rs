//! Backend-independent window data and events.

use std::collections::{BTreeMap, btree_map::Values};

use nara_app::{App, AppExitRequests, CoreStage, Plugin, PluginError};
use nara_ecs::{Component, ResMut, Resource, World, schedule::IntoScheduleConfigs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WindowId(u64);

impl Default for WindowId {
    fn default() -> Self {
        Self::PRIMARY
    }
}

impl WindowId {
    pub const PRIMARY: Self = Self(1);

    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WindowResolution {
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
}

impl Default for WindowResolution {
    fn default() -> Self {
        Self {
            physical_width: 1280,
            physical_height: 720,
            scale_factor: 1.0,
        }
    }
}

impl WindowResolution {
    #[must_use]
    pub fn new(physical_width: u32, physical_height: u32) -> Self {
        Self {
            physical_width,
            physical_height,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    #[must_use]
    pub const fn is_zero_sized(&self) -> bool {
        self.physical_width == 0 || self.physical_height == 0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindowMode {
    #[default]
    Windowed,
    BorderlessFullscreen,
    Fullscreen,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PresentMode {
    #[default]
    AutoVsync,
    AutoNoVsync,
    Fifo,
    Immediate,
    Mailbox,
}

#[derive(Debug, Clone, PartialEq, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub resolution: WindowResolution,
    pub mode: WindowMode,
    pub present_mode: PresentMode,
    pub resizable: bool,
    pub focused: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            id: WindowId::PRIMARY,
            title: "nara".to_owned(),
            resolution: WindowResolution::default(),
            mode: WindowMode::default(),
            present_mode: PresentMode::default(),
            resizable: true,
            focused: true,
        }
    }
}

impl Window {
    #[must_use]
    pub fn new(title: impl Into<String>, resolution: WindowResolution) -> Self {
        Self {
            title: title.into(),
            resolution,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_id(mut self, id: WindowId) -> Self {
        self.id = id;
        self
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
pub struct PrimaryWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct PrimaryWindowId(pub WindowId);

impl Default for PrimaryWindowId {
    fn default() -> Self {
        Self(WindowId::PRIMARY)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindowEvent {
    Created {
        window_id: WindowId,
    },
    CloseRequested {
        window_id: WindowId,
    },
    Closed {
        window_id: WindowId,
    },
    Resized {
        window_id: WindowId,
        resolution: WindowResolution,
    },
    Focused {
        window_id: WindowId,
        focused: bool,
    },
    ScaleFactorChanged {
        window_id: WindowId,
        scale_factor: f64,
    },
    RedrawRequested {
        window_id: WindowId,
    },
}

impl WindowEvent {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        match self {
            Self::Created { window_id }
            | Self::CloseRequested { window_id }
            | Self::Closed { window_id }
            | Self::Resized { window_id, .. }
            | Self::Focused { window_id, .. }
            | Self::ScaleFactorChanged { window_id, .. }
            | Self::RedrawRequested { window_id } => *window_id,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct WindowEvents {
    events: Vec<WindowEvent>,
}

impl WindowEvents {
    pub fn push(&mut self, event: WindowEvent) {
        self.events.push(event);
    }

    fn clear(&mut self) {
        self.events.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[WindowEvent] {
        &self.events
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCloseRequest {
    window_id: WindowId,
    canceled: bool,
}

impl WindowCloseRequest {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn is_canceled(&self) -> bool {
        self.canceled
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct WindowCloseRequests {
    requests: BTreeMap<WindowId, WindowCloseRequest>,
}

impl WindowCloseRequests {
    pub fn request(&mut self, window_id: WindowId) {
        self.requests.insert(
            window_id,
            WindowCloseRequest {
                window_id,
                canceled: false,
            },
        );
    }

    pub fn cancel(&mut self, window_id: WindowId) {
        if let Some(request) = self.requests.get_mut(&window_id) {
            request.canceled = true;
        }
    }

    #[must_use]
    pub fn is_requested(&self, window_id: WindowId) -> bool {
        self.requests.contains_key(&window_id)
    }

    #[must_use]
    pub fn is_canceled(&self, window_id: WindowId) -> bool {
        self.requests
            .get(&window_id)
            .is_some_and(WindowCloseRequest::is_canceled)
    }

    #[must_use]
    pub fn has_uncancelled(&self) -> bool {
        self.requests.values().any(|request| !request.is_canceled())
    }

    pub fn iter(&self) -> Values<'_, WindowId, WindowCloseRequest> {
        self.requests.values()
    }

    fn clear(&mut self) {
        self.requests.clear();
    }
}

pub fn apply_window_event(world: &mut World, event: &WindowEvent) {
    let window_id = event.window_id();
    if matches!(event, WindowEvent::CloseRequested { .. }) {
        if !world.contains_resource::<WindowCloseRequests>() {
            world.insert_resource(WindowCloseRequests::default());
        }
        world
            .resource_mut::<WindowCloseRequests>()
            .request(window_id);
    }

    let mut query = world.query::<&mut Window>();

    for mut window in query.iter_mut(world) {
        if window.id != window_id {
            continue;
        }

        match event {
            WindowEvent::Resized { resolution, .. } => {
                window.resolution = *resolution;
            }
            WindowEvent::Focused { focused, .. } => {
                window.focused = *focused;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                window.resolution.scale_factor = *scale_factor;
            }
            WindowEvent::Created { .. }
            | WindowEvent::CloseRequested { .. }
            | WindowEvent::Closed { .. }
            | WindowEvent::RedrawRequested { .. } => {}
        }
    }
}

pub fn push_window_event(world: &mut World, event: WindowEvent) {
    if !world.contains_resource::<WindowEvents>() {
        world.insert_resource(WindowEvents::default());
    }
    apply_window_event(world, &event);
    world.resource_mut::<WindowEvents>().push(event);
}

#[derive(Debug, Clone)]
pub struct WindowPlugin {
    pub primary_window: Option<Window>,
}

pub const WINDOW_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.window");
const WINDOW_PLUGIN_DEFINITION_ID: nara_app::PluginDefinitionId =
    nara_app::PluginDefinitionId::new("nara.window.configured", 1);
const WINDOW_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("desktop-winit")];
pub const WINDOW_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(WINDOW_PLUGIN_ID, nara_app::PluginCategory::Platform)
        .requires_product_capabilities(WINDOW_PRODUCT_REQUIREMENTS);

/// Creates a repeatable window plugin definition from primary-window data.
#[must_use]
pub fn plugin(primary_window: Option<Window>) -> nara_app::PluginDefinition {
    let canonical_configuration = window_plugin_configuration(primary_window.as_ref());
    nara_app::PluginDefinition::infallible::<WindowPlugin, _>(
        WINDOW_PLUGIN_DEFINITION_ID,
        canonical_configuration,
        move || WindowPlugin {
            primary_window: primary_window.clone(),
        },
    )
}

fn window_plugin_configuration(primary_window: Option<&Window>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(
        primary_window.map_or(32, |window| 96_usize.saturating_add(window.title.len())),
    );
    bytes.extend_from_slice(b"nara.window.plugin-config.v1\0");
    let Some(window) = primary_window else {
        bytes.push(0);
        return bytes;
    };
    bytes.push(1);
    bytes.extend_from_slice(&window.id.get().to_le_bytes());
    bytes.extend_from_slice(&(window.title.len() as u64).to_le_bytes());
    bytes.extend_from_slice(window.title.as_bytes());
    bytes.extend_from_slice(&window.resolution.physical_width.to_le_bytes());
    bytes.extend_from_slice(&window.resolution.physical_height.to_le_bytes());
    bytes.extend_from_slice(&window.resolution.scale_factor.to_bits().to_le_bytes());
    bytes.push(match window.mode {
        WindowMode::Windowed => 0,
        WindowMode::BorderlessFullscreen => 1,
        WindowMode::Fullscreen => 2,
    });
    bytes.push(match window.present_mode {
        PresentMode::AutoVsync => 0,
        PresentMode::AutoNoVsync => 1,
        PresentMode::Fifo => 2,
        PresentMode::Immediate => 3,
        PresentMode::Mailbox => 4,
    });
    bytes.push(u8::from(window.resizable));
    bytes.push(u8::from(window.focused));
    bytes
}

impl Default for WindowPlugin {
    fn default() -> Self {
        Self {
            primary_window: Some(Window::default()),
        }
    }
}

impl Plugin for WindowPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WINDOW_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<WindowEvents>()?;
        app.init_resource::<WindowCloseRequests>()?;
        app.init_resource::<backend::BackendWindowHandles>()?;
        app.insert_resource(PrimaryWindowId::default())?;

        if let Some(window) = &self.primary_window {
            let primary_id = window.id;
            app.insert_resource(PrimaryWindowId(primary_id))?;
            app.world_mut()?.spawn((window.clone(), PrimaryWindow));
        }
        app.add_systems(
            CoreStage::Last,
            (
                request_exit_for_uncancelled_close,
                clear_window_frame_events,
            )
                .chain(),
        )?;
        Ok(())
    }
}

fn request_exit_for_uncancelled_close(
    close_requests: ResMut<WindowCloseRequests>,
    mut exit_requests: ResMut<AppExitRequests>,
) {
    if close_requests.has_uncancelled() {
        exit_requests.request_exit();
    }
}

fn clear_window_frame_events(
    mut events: ResMut<WindowEvents>,
    mut close_requests: ResMut<WindowCloseRequests>,
) {
    events.clear();
    close_requests.clear();
}

pub mod backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_has_non_zero_resolution() {
        let window = Window::default();

        assert_eq!(window.id, WindowId::PRIMARY);
        assert_eq!(window.title, "nara");
        assert!(!window.resolution.is_zero_sized());
    }

    #[test]
    fn zero_sized_resolution_is_detected() {
        assert!(WindowResolution::new(0, 720).is_zero_sized());
        assert!(WindowResolution::new(1280, 0).is_zero_sized());
        assert!(!WindowResolution::new(1280, 720).is_zero_sized());
    }

    #[test]
    fn window_plugin_spawns_primary_window_data() {
        let mut app = App::new();
        app.add_plugin(WindowPlugin::default()).unwrap();

        assert_eq!(
            app.world().resource::<PrimaryWindowId>().0,
            WindowId::PRIMARY
        );
        assert!(
            !app.world()
                .resource::<backend::BackendWindowHandles>()
                .is_registered(WindowId::PRIMARY)
        );

        let world = app.world_mut().expect("app should allow world mutation");
        let mut query = world.query::<(&Window, &PrimaryWindow)>();
        let windows = query.iter(world).collect::<Vec<_>>();

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].0.id, WindowId::PRIMARY);
    }

    #[test]
    fn window_plugin_definition_binds_canonical_window_configuration() {
        let window = Window::new("configured", WindowResolution::new(960, 540));
        let first = plugin(Some(window.clone()));
        let repeated = plugin(Some(window.clone()));
        let changed = plugin(Some(Window::new(
            "changed",
            WindowResolution::new(960, 540),
        )));

        assert_eq!(first.key(), repeated.key());
        assert_ne!(first.key(), changed.key());

        let plan = nara_app::PluginPlan::resolve(first).unwrap();
        let app = plan.instantiate().unwrap();
        let mut query = app
            .world()
            .iter_entities()
            .filter_map(|entity| entity.get::<Window>());

        assert_eq!(query.next(), Some(&window));
        assert_eq!(query.next(), None);
    }

    #[test]
    fn resize_event_updates_matching_window_and_records_event() {
        let mut world = World::new();
        world.insert_resource(WindowEvents::default());
        world.spawn((Window::default(), PrimaryWindow));

        let resolution = WindowResolution::new(640, 480);
        push_window_event(
            &mut world,
            WindowEvent::Resized {
                window_id: WindowId::PRIMARY,
                resolution,
            },
        );

        let mut query = world.query::<&Window>();
        let window = query.single(&world).unwrap();

        assert_eq!(window.resolution, resolution);
        assert_eq!(world.resource::<WindowEvents>().as_slice().len(), 1);
    }

    #[test]
    fn focus_event_updates_window_and_close_event_records_frame_request() {
        let mut world = World::new();
        world.spawn((Window::default(), PrimaryWindow));

        push_window_event(
            &mut world,
            WindowEvent::Focused {
                window_id: WindowId::PRIMARY,
                focused: false,
            },
        );
        push_window_event(
            &mut world,
            WindowEvent::CloseRequested {
                window_id: WindowId::PRIMARY,
            },
        );

        let mut query = world.query::<&Window>();
        let window = query.single(&world).unwrap();

        assert!(!window.focused);
        assert!(
            world
                .resource::<WindowCloseRequests>()
                .is_requested(WindowId::PRIMARY)
        );
    }

    fn cancel_primary_close(mut close_requests: ResMut<WindowCloseRequests>) {
        close_requests.cancel(WindowId::PRIMARY);
    }

    #[test]
    fn close_request_can_be_cancelled_before_last_stage_exit_request() {
        let mut app = App::new();
        app.add_plugin(WindowPlugin::default()).unwrap();
        app.add_systems(CoreStage::Update, cancel_primary_close)
            .expect("app should accept systems");
        push_window_event(
            app.world_mut().expect("app should allow world mutation"),
            WindowEvent::CloseRequested {
                window_id: WindowId::PRIMARY,
            },
        );

        let outcome = app.run_once(std::time::Duration::ZERO).unwrap();

        assert_eq!(outcome.exit, None);
        assert!(
            !app.world()
                .resource::<WindowCloseRequests>()
                .is_requested(WindowId::PRIMARY)
        );
        assert!(app.world().resource::<WindowEvents>().as_slice().is_empty());
    }

    #[test]
    fn uncancelled_close_request_becomes_app_exit_request() {
        let mut app = App::new();
        app.add_plugin(WindowPlugin::default()).unwrap();
        push_window_event(
            app.world_mut().expect("app should allow world mutation"),
            WindowEvent::CloseRequested {
                window_id: WindowId::PRIMARY,
            },
        );

        let outcome = app.run_once(std::time::Duration::ZERO).unwrap();

        assert_eq!(outcome.exit, Some(nara_app::AppExit::Requested));
        assert!(app.world().resource::<WindowEvents>().as_slice().is_empty());
    }

    #[test]
    fn backend_handle_registry_starts_empty() {
        let handles = backend::BackendWindowHandles::default();

        assert!(!handles.is_registered(WindowId::PRIMARY));
    }
}
