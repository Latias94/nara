use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::panels::{
    EditorNativeWindowId, EditorPanelInstance, EditorPanelInstanceId, EditorPanelMultiplicity,
    EditorPanelPlacement, EditorPanelRegistrationError, EditorPanelRegistry, EditorPanelTarget,
    EditorRenderViewId,
};

const LAYOUT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorToolkitLayout {
    toolkit: String,
    version: u32,
    data: String,
}

impl EditorToolkitLayout {
    #[must_use]
    pub fn new(toolkit: impl Into<String>, version: u32, data: impl Into<String>) -> Self {
        Self {
            toolkit: toolkit.into(),
            version,
            data: data.into(),
        }
    }
    #[must_use]
    pub fn data(&self) -> &str {
        &self.data
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EditorWindowBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl EditorWindowBounds {
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    fn intersects(self, other: Self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// User preferences only. This object has no source persistence or document mutation capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorWorkspaceLayout {
    version: u32,
    next_instance: u64,
    panels: Vec<EditorPanelInstance>,
    toolkit: Option<EditorToolkitLayout>,
    window_bounds: Option<EditorWindowBounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorWorkspaceRestoreReason {
    MissingContribution(String),
    IncompatibleToolkitLayout,
    UnsupportedLayoutVersion,
    InvalidInstances,
    OffscreenWindow,
    SecondaryWindowUnavailable,
    InvalidPreferences(String),
}

impl std::fmt::Display for EditorWorkspaceRestoreReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingContribution(kind) => {
                write!(f, "Layout reset: panel contribution {kind} is unavailable")
            }
            Self::IncompatibleToolkitLayout => {
                f.write_str("Toolkit layout reset: frontend or layout version changed")
            }
            Self::UnsupportedLayoutVersion => {
                f.write_str("Layout reset: preference version is unsupported")
            }
            Self::InvalidInstances => {
                f.write_str("Layout reset: panel instance identities are invalid")
            }
            Self::OffscreenWindow => {
                f.write_str("Window placement reset: saved bounds are outside available displays")
            }
            Self::SecondaryWindowUnavailable => {
                f.write_str("Panel returned to primary window: detached windows are unavailable")
            }
            Self::InvalidPreferences(reason) => write!(f, "Layout reset: {reason}"),
        }
    }
}

#[derive(Debug)]
pub struct EditorWorkspaceRestore {
    layout: EditorWorkspaceLayout,
    reasons: Vec<EditorWorkspaceRestoreReason>,
}

impl EditorWorkspaceRestore {
    #[must_use]
    pub const fn layout(&self) -> &EditorWorkspaceLayout {
        &self.layout
    }
    #[must_use]
    pub fn reasons(&self) -> &[EditorWorkspaceRestoreReason] {
        &self.reasons
    }
    #[must_use]
    pub fn into_layout(self) -> EditorWorkspaceLayout {
        self.layout
    }
    pub(crate) fn into_parts(self) -> (EditorWorkspaceLayout, Vec<EditorWorkspaceRestoreReason>) {
        (self.layout, self.reasons)
    }
}

impl EditorWorkspaceLayout {
    #[must_use]
    pub fn default_for(registry: &EditorPanelRegistry) -> Self {
        let mut layout = Self {
            version: LAYOUT_VERSION,
            next_instance: 1,
            panels: Vec::new(),
            toolkit: None,
            window_bounds: None,
        };
        for contribution in registry
            .contributions()
            .filter(|contribution| contribution.initially_open())
        {
            // A finite source registry cannot exhaust the u64 instance space.
            let _ = layout.open(
                registry,
                contribution.kind(),
                EditorPanelTarget::ActiveDocument,
            );
        }
        layout
    }
    #[must_use]
    pub fn instances(&self) -> &[EditorPanelInstance] {
        &self.panels
    }
    #[must_use]
    pub fn instance(&self, id: EditorPanelInstanceId) -> Option<&EditorPanelInstance> {
        self.panels.iter().find(|panel| panel.id == id)
    }
    pub fn open(
        &mut self,
        registry: &EditorPanelRegistry,
        kind: &str,
        target: EditorPanelTarget,
    ) -> Result<EditorPanelInstanceId, EditorPanelRegistrationError> {
        let contribution = registry
            .descriptor(kind)
            .ok_or_else(|| EditorPanelRegistrationError::Missing(kind.to_owned()))?;
        if contribution.multiplicity() == EditorPanelMultiplicity::Single
            && let Some(panel) = self.panels.iter_mut().find(|panel| panel.kind == kind)
        {
            panel.open = true;
            panel.visible = true;
            panel.target = target;
            return Ok(panel.id);
        }
        let next = self
            .next_instance
            .checked_add(1)
            .ok_or(EditorPanelRegistrationError::InstanceCapacity)?;
        let id = EditorPanelInstanceId(self.next_instance);
        self.next_instance = next;
        self.panels.push(EditorPanelInstance {
            id,
            kind: kind.to_owned(),
            target,
            placement: contribution.default_placement(),
            open: true,
            visible: true,
            window: EditorNativeWindowId(1),
            render_view: None,
        });
        Ok(id)
    }
    pub fn close(&mut self, id: EditorPanelInstanceId) -> bool {
        self.change(id, |panel| {
            panel.open = false;
            panel.visible = false;
        })
    }
    pub fn reopen(&mut self, id: EditorPanelInstanceId) -> bool {
        self.change(id, |panel| {
            panel.open = true;
            panel.visible = true;
        })
    }
    pub fn set_visible(&mut self, id: EditorPanelInstanceId, visible: bool) -> bool {
        self.change(id, |panel| panel.visible = visible)
    }
    pub fn set_target(&mut self, id: EditorPanelInstanceId, target: EditorPanelTarget) -> bool {
        self.change(id, |panel| panel.target = target)
    }
    pub fn set_render_view(
        &mut self,
        id: EditorPanelInstanceId,
        view: Option<EditorRenderViewId>,
    ) -> bool {
        self.change(id, |panel| panel.render_view = view)
    }
    pub fn place(&mut self, id: EditorPanelInstanceId, placement: EditorPanelPlacement) -> bool {
        self.change(id, |panel| panel.placement = placement)
    }
    fn change(
        &mut self,
        id: EditorPanelInstanceId,
        change: impl FnOnce(&mut EditorPanelInstance),
    ) -> bool {
        let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) else {
            return false;
        };
        change(panel);
        true
    }
    pub fn reset(&mut self, registry: &EditorPanelRegistry) {
        *self = Self::default_for(registry);
    }
    pub fn set_toolkit_layout(&mut self, toolkit: Option<EditorToolkitLayout>) {
        self.toolkit = toolkit;
    }
    #[must_use]
    pub const fn toolkit_layout(&self) -> Option<&EditorToolkitLayout> {
        self.toolkit.as_ref()
    }
    pub fn set_window_bounds(&mut self, bounds: EditorWindowBounds) {
        self.window_bounds = Some(bounds);
    }
    #[must_use]
    pub const fn window_bounds(&self) -> Option<EditorWindowBounds> {
        self.window_bounds
    }
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    #[must_use]
    pub fn restore(
        mut saved: Self,
        registry: &EditorPanelRegistry,
        toolkit: &str,
        toolkit_version: u32,
        displays: &[EditorWindowBounds],
    ) -> EditorWorkspaceRestore {
        let mut reasons = Vec::new();
        if saved.version != LAYOUT_VERSION {
            reasons.push(EditorWorkspaceRestoreReason::UnsupportedLayoutVersion);
        }
        let mut identities = std::collections::BTreeSet::new();
        let mut single_kinds = std::collections::BTreeSet::new();
        for panel in &saved.panels {
            match registry.descriptor(&panel.kind) {
                None => reasons.push(EditorWorkspaceRestoreReason::MissingContribution(
                    panel.kind.clone(),
                )),
                Some(descriptor)
                    if descriptor.multiplicity() == EditorPanelMultiplicity::Single
                        && !single_kinds.insert(&panel.kind) =>
                {
                    reasons.push(EditorWorkspaceRestoreReason::InvalidInstances)
                }
                Some(_) => {}
            }
            if panel.id.0 == 0 || panel.id.0 >= saved.next_instance || !identities.insert(panel.id)
            {
                reasons.push(EditorWorkspaceRestoreReason::InvalidInstances);
            }
        }
        if !reasons.is_empty() {
            saved = Self::default_for(registry);
        }
        if saved
            .toolkit
            .as_ref()
            .is_some_and(|blob| blob.toolkit != toolkit || blob.version != toolkit_version)
        {
            saved.toolkit = None;
            for panel in &mut saved.panels {
                if let Some(descriptor) = registry.descriptor(panel.kind()) {
                    panel.placement = descriptor.default_placement();
                    if descriptor.initially_open() {
                        panel.open = true;
                        panel.visible = true;
                    }
                }
            }
            reasons.push(EditorWorkspaceRestoreReason::IncompatibleToolkitLayout);
        }
        if saved.window_bounds.is_some_and(|bounds| {
            !displays.is_empty() && !displays.iter().any(|display| bounds.intersects(*display))
        }) {
            saved.window_bounds = None;
            reasons.push(EditorWorkspaceRestoreReason::OffscreenWindow);
        }
        if saved
            .panels
            .iter()
            .any(|panel| panel.window != EditorNativeWindowId(1))
        {
            for panel in &mut saved.panels {
                panel.window = EditorNativeWindowId(1);
            }
            reasons.push(EditorWorkspaceRestoreReason::SecondaryWindowUnavailable);
        }
        EditorWorkspaceRestore {
            layout: saved,
            reasons,
        }
    }
}

/// One explicit user-settings path, independent of whichever project is open.
#[derive(Debug, Clone)]
pub struct EditorWorkspacePreferences {
    path: PathBuf,
}

impl EditorWorkspacePreferences {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    #[must_use]
    pub fn user_default() -> Option<Self> {
        let directory = if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })
        }?;
        directory
            .is_absolute()
            .then(|| Self::new(directory.join("nara/editor-workspace.json")))
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> io::Result<Option<EditorWorkspaceLayout>> {
        match fs::read(&self.path) {
            Ok(bytes) => EditorWorkspaceLayout::from_json(&bytes)
                .map(Some)
                .map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn save(&self, layout: &EditorWorkspaceLayout) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, layout.to_json().map_err(io::Error::other)?)
    }
}
