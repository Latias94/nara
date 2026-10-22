use std::collections::BTreeMap;

use nara::tooling::{
    EditorDocumentId, EditorDocumentKind, EditorDocumentTabModel, EditorExtensionDocumentView,
    EditorFrameModel, EditorSelectionSet,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable contribution identity. Instances, documents and render views have separate lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPanelDescriptor {
    kind: String,
    title: String,
    multiplicity: EditorPanelMultiplicity,
    placement: EditorPanelPlacement,
    initially_open: bool,
}

impl EditorPanelDescriptor {
    #[must_use]
    pub fn new(
        kind: impl Into<String>,
        title: impl Into<String>,
        multiplicity: EditorPanelMultiplicity,
        placement: EditorPanelPlacement,
        initially_open: bool,
    ) -> Self {
        Self {
            kind: kind.into(),
            title: title.into(),
            multiplicity,
            placement,
            initially_open,
        }
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub const fn multiplicity(&self) -> EditorPanelMultiplicity {
        self.multiplicity
    }
    #[must_use]
    pub const fn default_placement(&self) -> EditorPanelPlacement {
        self.placement
    }
    #[must_use]
    pub const fn initially_open(&self) -> bool {
        self.initially_open
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPanelMultiplicity {
    Single,
    Multiple,
}

/// A finite arrangement intent, not a cross-toolkit dock graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EditorPanelPlacement {
    Left,
    Center,
    Right,
    Bottom,
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EditorPanelInstanceId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorRenderViewId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EditorNativeWindowId(pub u64);

/// Source locators survive presentation replacement; runtime document IDs are never saved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorPanelTarget {
    #[default]
    ActiveDocument,
    Source {
        kind: EditorPanelSourceKind,
        locator: String,
    },
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorPanelSourceKind {
    Scene,
    Prefab,
    Extension,
}

impl EditorPanelTarget {
    #[must_use]
    pub fn document(&self, frame: &EditorFrameModel) -> Option<EditorDocumentId> {
        match self {
            Self::ActiveDocument => frame.workspace().active_document,
            Self::Source { kind, locator } => frame
                .workspace()
                .documents
                .iter()
                .find(|document| {
                    (matches!(
                        (kind, document.kind),
                        (EditorPanelSourceKind::Scene, EditorDocumentKind::Scene)
                            | (EditorPanelSourceKind::Prefab, EditorDocumentKind::Prefab)
                    ) || matches!(kind, EditorPanelSourceKind::Extension)
                        && matches!(document.kind, EditorDocumentKind::Extension(_)))
                        && document
                            .source_binding
                            .as_ref()
                            .is_some_and(|source| source.locator() == locator)
                })
                .map(|document| document.document),
            Self::Workspace => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPanelInstance {
    pub(crate) id: EditorPanelInstanceId,
    pub(crate) kind: String,
    pub(crate) target: EditorPanelTarget,
    pub(crate) placement: EditorPanelPlacement,
    pub(crate) open: bool,
    pub(crate) visible: bool,
    pub(crate) window: EditorNativeWindowId,
    #[serde(skip)]
    pub(crate) render_view: Option<EditorRenderViewId>,
}

impl EditorPanelInstance {
    #[must_use]
    pub const fn id(&self) -> EditorPanelInstanceId {
        self.id
    }
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    #[must_use]
    pub const fn target(&self) -> &EditorPanelTarget {
        &self.target
    }
    #[must_use]
    pub const fn placement(&self) -> EditorPanelPlacement {
        self.placement
    }
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.open && self.visible
    }
    #[must_use]
    pub const fn native_window(&self) -> EditorNativeWindowId {
        self.window
    }
    #[must_use]
    pub const fn render_view(&self) -> Option<EditorRenderViewId> {
        self.render_view
    }
}

/// Trusted source composition only. Ordinary project Open never modifies this registry.
#[derive(Debug, Clone, Default)]
pub struct EditorPanelRegistry {
    contributions: BTreeMap<String, EditorPanelDescriptor>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditorPanelRegistrationError {
    #[error("panel contribution identity must not be empty")]
    EmptyIdentity,
    #[error("panel contribution already registered: {0}")]
    Duplicate(String),
    #[error("panel contribution is not registered: {0}")]
    Missing(String),
    #[error("panel instance identity capacity exhausted")]
    InstanceCapacity,
}

impl EditorPanelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(
        &mut self,
        descriptor: EditorPanelDescriptor,
    ) -> Result<(), EditorPanelRegistrationError> {
        if descriptor.kind.is_empty() {
            return Err(EditorPanelRegistrationError::EmptyIdentity);
        }
        if self.contributions.contains_key(descriptor.kind()) {
            return Err(EditorPanelRegistrationError::Duplicate(
                descriptor.kind.clone(),
            ));
        }
        self.contributions
            .insert(descriptor.kind.clone(), descriptor);
        Ok(())
    }
    #[must_use]
    pub fn descriptor(&self, kind: &str) -> Option<&EditorPanelDescriptor> {
        self.contributions.get(kind)
    }
    pub fn contributions(&self) -> impl Iterator<Item = &EditorPanelDescriptor> {
        self.contributions.values()
    }
}

/// Read-only model shared by trusted semantic contributions and frontend companions.
#[derive(Debug, Clone, Copy)]
pub struct EditorPanelModel<'a> {
    frame: &'a EditorFrameModel,
    instance: &'a EditorPanelInstance,
}

impl<'a> EditorPanelModel<'a> {
    #[must_use]
    pub const fn new(frame: &'a EditorFrameModel, instance: &'a EditorPanelInstance) -> Self {
        Self { frame, instance }
    }
    #[must_use]
    pub const fn frame(&self) -> &'a EditorFrameModel {
        self.frame
    }
    #[must_use]
    pub const fn instance(&self) -> &'a EditorPanelInstance {
        self.instance
    }
    #[must_use]
    pub fn document(&self) -> Option<&'a EditorDocumentTabModel> {
        self.instance
            .target
            .document(self.frame)
            .and_then(|document| self.frame.workspace().document(document))
    }
    #[must_use]
    pub fn selection(&self) -> Option<&'a EditorSelectionSet> {
        let document = self.document()?.document;
        self.frame
            .workspace()
            .active_scene
            .as_ref()
            .filter(|scene| scene.document == document)
            .map(|scene| &scene.selection)
    }

    #[must_use]
    pub fn extension_document(&self) -> Option<&'a EditorExtensionDocumentView> {
        self.document()
            .and_then(|document| self.frame.workspace().extension_document(document.document))
    }
}
