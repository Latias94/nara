//! Finite, cooperative native attachment for image-backed semantic Editor panels.
//!
//! The caller renders the exact displays, mappings and clips exposed by a staged frame through
//! its existing WGPU Host. This module only adopts input routes after that Host reports the
//! complete original native presentation. It creates no GPU owner, event loop or UI journal.

use std::{collections::VecDeque, sync::Arc, time::Instant};

use nara::{
    render::{DisplayMapping2d, DisplayRect},
    tooling::EditorWorkspaceIntentView,
    window::WindowId,
};
use nara_render_wgpu::{WgpuDisplayOccurrence, WgpuImageDisplay, WgpuSubmittedImageReceipt};
use winit::{event::MouseButton, keyboard::ModifiersState, window::Window};

use crate::{
    panels::{EditorPanelInstanceToken, EditorPanelSubmission},
    workspace::EditorPanelInstanceId,
};

use super::EditorClipboard;

mod events;
mod geometry;
mod input;
mod observation;
mod presentation;
mod services;

#[cfg(test)]
mod tests;

pub use events::{
    EditorPanelCancelReason, EditorPanelInputKind, EditorPanelKeyEvent, EditorPanelPlatformEvent,
    EditorPanelWheelDelta,
};
pub use observation::EditorPanelImageObservation;
pub use services::{EditorPanelImeArea, EditorPanelServiceError, EditorPanelServices};

/// The finite per-window attachment and complete-candidate image limit.
pub const EDITOR_PANEL_IMAGE_LIMIT: usize = 64;

/// Nominal provider-local text-focus lifetime, independent of widget IDs or field storage.
///
/// Keep one scope for a local field's current text-focus lifetime and clone it when requesting
/// input. A fresh scope ends the previous lifetime even inside the same semantic panel. A scope
/// grants no capability or keyboard ownership. Providers compare the delivered scope against
/// their original field token rather than redirect text to their latest local focus.
#[derive(Clone)]
pub struct EditorPanelTextInputScope(Arc<()>);

impl EditorPanelTextInputScope {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl Default for EditorPanelTextInputScope {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for EditorPanelTextInputScope {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EditorPanelTextInputScope {}

impl std::fmt::Debug for EditorPanelTextInputScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPanelTextInputScope")
            .finish_non_exhaustive()
    }
}

/// Native services explicitly requested by an attached panel.
///
/// These are source-level, cooperative in-process permissions, not a sandbox or a stable ABI.
/// Unsupported capabilities fail at attachment, before any image can be staged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorPanelCapability {
    Pointer,
    Wheel,
    Keyboard,
    TextInput,
    Clipboard,
    Cursor,
    Refresh,
    WindowModal,
    /// Cooperative provider-defined semantic operation through an exact accepted observation.
    /// This grants no native accessibility integration or control identity vocabulary.
    ObservedOperations,
    OutsidePanelOverlay,
    DirectRendering,
    NativeDetach,
    Accessibility,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Capabilities(u16);

impl Capabilities {
    fn bit(capability: EditorPanelCapability) -> Option<u16> {
        use EditorPanelCapability as C;
        Some(match capability {
            C::Pointer => 1,
            C::Wheel => 1 << 1,
            C::Keyboard => 1 << 2,
            C::TextInput => 1 << 3,
            C::Clipboard => 1 << 4,
            C::Cursor => 1 << 5,
            C::Refresh => 1 << 6,
            C::WindowModal => 1 << 7,
            C::ObservedOperations => 1 << 8,
            C::OutsidePanelOverlay | C::DirectRendering | C::NativeDetach | C::Accessibility => {
                return None;
            }
        })
    }

    fn admit(
        capabilities: impl IntoIterator<Item = EditorPanelCapability>,
    ) -> Result<Self, EditorPanelImageError> {
        let mut admitted = Self::default();
        for capability in capabilities {
            admitted.0 |= Self::bit(capability)
                .ok_or(EditorPanelImageError::UnsupportedCapability(capability))?;
        }
        use EditorPanelCapability as C;
        for (capability, prerequisite) in [
            (C::TextInput, C::Keyboard),
            (C::WindowModal, C::Keyboard),
            (C::WindowModal, C::Pointer),
            (C::ObservedOperations, C::Keyboard),
        ] {
            if admitted.has(capability) && !admitted.has(prerequisite) {
                return Err(EditorPanelImageError::MissingCapability(prerequisite));
            }
        }
        Ok(admitted)
    }

    fn has(self, capability: EditorPanelCapability) -> bool {
        Self::bit(capability).is_some_and(|bit| self.0 & bit != 0)
    }
}

/// Exact semantic incarnation and original native image occurrence receiving an input delivery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorPanelImageEndpoint {
    token: EditorPanelInstanceToken,
    image: WgpuSubmittedImageReceipt,
    occurrence: WgpuDisplayOccurrence,
}

impl EditorPanelImageEndpoint {
    #[must_use]
    pub fn token(&self) -> &EditorPanelInstanceToken {
        &self.token
    }

    #[must_use]
    pub fn instance(&self) -> EditorPanelInstanceId {
        self.token.instance()
    }

    #[must_use]
    pub const fn image(&self) -> WgpuSubmittedImageReceipt {
        self.image
    }

    #[must_use]
    pub const fn occurrence(&self) -> WgpuDisplayOccurrence {
        self.occurrence
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ImageInputPolicy {
    #[default]
    Ordinary,
    WorkspaceDecision(EditorWorkspaceIntentView),
}

impl ImageInputPolicy {
    fn is_current_workspace_decision(self, current: Option<EditorWorkspaceIntentView>) -> bool {
        matches!(self, Self::WorkspaceDecision(view)
            if Some(view) == current
                && (view.decision_prompt().is_some() || view.result().is_some()))
    }

    fn allows_input(self, locked: bool, current: Option<EditorWorkspaceIntentView>) -> bool {
        match self {
            Self::Ordinary => !locked,
            Self::WorkspaceDecision(_) => self.is_current_workspace_decision(current),
        }
    }
}

/// One whole-image output and its immutable frontend snapshot in logical window coordinates.
///
/// A complete window candidate admits at most one image per semantic panel instance. Distinct
/// panels may sample the same immutable output with distinct original display occurrences.
/// The clip is the exact logical-window clip to use when rendering; letterbox space is not input.
pub struct EditorPanelImage<S> {
    token: EditorPanelInstanceToken,
    display: WgpuImageDisplay,
    snapshot: Arc<S>,
    mapping: DisplayMapping2d,
    clip: DisplayRect,
    window_modal: bool,
    input_policy: ImageInputPolicy,
}

impl<S> Clone for EditorPanelImage<S> {
    fn clone(&self) -> Self {
        Self {
            token: self.token.clone(),
            display: self.display.clone(),
            snapshot: Arc::clone(&self.snapshot),
            mapping: self.mapping,
            clip: self.clip,
            window_modal: self.window_modal,
            input_policy: self.input_policy,
        }
    }
}

impl<S> std::fmt::Debug for EditorPanelImage<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPanelImage")
            .field("endpoint", &self.endpoint())
            .field("mapping", &self.mapping)
            .field("clip", &self.clip)
            .field("window_modal", &self.window_modal)
            .field("input_policy", &self.input_policy)
            .finish_non_exhaustive()
    }
}

impl<S> EditorPanelImage<S> {
    /// Captures placement facts without granting input. [`EditorPanelImageHost::stage`] validates
    /// them against the attached token, original image extent and current native window geometry.
    #[must_use]
    pub fn new(
        token: EditorPanelInstanceToken,
        display: WgpuImageDisplay,
        snapshot: Arc<S>,
        mapping: DisplayMapping2d,
        clip: DisplayRect,
    ) -> Self {
        Self {
            token,
            display,
            snapshot,
            mapping,
            clip,
            window_modal: false,
            input_policy: ImageInputPolicy::Ordinary,
        }
    }

    /// Makes this image the window-modal provider. The last modal in paint order owns the window;
    /// pointer input outside its visible content is consumed, and Escape goes to that provider.
    #[must_use]
    pub fn with_window_modal(mut self, window_modal: bool) -> Self {
        self.window_modal = window_modal;
        self
    }

    /// Captures the first-party shell's exact workspace decision without granting action authority.
    /// Views without a decision prompt or result leave the image's input policy unchanged.
    /// Captured decision input expires when that exact view changes.
    #[must_use]
    pub(crate) fn with_workspace_decision(mut self, decision: &EditorWorkspaceIntentView) -> Self {
        if decision.decision_prompt().is_some() || decision.result().is_some() {
            self.input_policy = ImageInputPolicy::WorkspaceDecision(*decision);
        }
        self
    }

    #[must_use]
    pub fn token(&self) -> &EditorPanelInstanceToken {
        &self.token
    }

    #[must_use]
    pub fn endpoint(&self) -> EditorPanelImageEndpoint {
        EditorPanelImageEndpoint {
            token: self.token.clone(),
            image: self.display.receipt(),
            occurrence: self.display.occurrence(),
        }
    }

    #[must_use]
    pub fn display(&self) -> &WgpuImageDisplay {
        &self.display
    }

    #[must_use]
    pub fn snapshot(&self) -> &Arc<S> {
        &self.snapshot
    }

    #[must_use]
    pub const fn mapping(&self) -> DisplayMapping2d {
        self.mapping
    }

    #[must_use]
    pub const fn clip(&self) -> DisplayRect {
        self.clip
    }

    #[must_use]
    pub const fn is_window_modal(&self) -> bool {
        self.window_modal
    }

    fn compatible(&self, successor: &Self) -> bool {
        self.token == successor.token
            && self.mapping == successor.mapping
            && self.clip == successor.clip
            && self.window_modal == successor.window_modal
            && self.input_policy == successor.input_policy
    }
}

/// Native geometry captured at staging and checked again before accepting the native outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorPanelWindowGeometry {
    pub physical_size: [u32; 2],
    pub scale_factor: f64,
}

struct ImageCandidate<S> {
    owner: Arc<()>,
    window: WindowId,
    display_frame_index: u64,
    geometry: EditorPanelWindowGeometry,
    images: Vec<EditorPanelImage<S>>,
}

/// A single complete native-window candidate; it grants no input until accepted.
///
/// Pass the same displays to the original WGPU Host, sample them using the exposed mapping and
/// clip, and give its original outcome back to [`EditorPanelImageHost::presented`]. Dropping a
/// frame does not submit, retire or accept any GPU work. A newer stage supersedes this candidate.
#[must_use]
pub struct EditorPanelImageFrame<S>(Arc<ImageCandidate<S>>);

impl<S> Clone for EditorPanelImageFrame<S> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<S> std::fmt::Debug for EditorPanelImageFrame<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPanelImageFrame")
            .field("window", &self.0.window)
            .field("display_frame_index", &self.0.display_frame_index)
            .field("geometry", &self.0.geometry)
            .field("images", &self.0.images)
            .finish_non_exhaustive()
    }
}

impl<S> EditorPanelImageFrame<S> {
    #[must_use]
    pub fn images(&self) -> &[EditorPanelImage<S>] {
        &self.0.images
    }

    #[must_use]
    pub fn window_id(&self) -> WindowId {
        self.0.window
    }

    #[must_use]
    pub fn display_frame_index(&self) -> u64 {
        self.0.display_frame_index
    }

    #[must_use]
    pub fn geometry(&self) -> EditorPanelWindowGeometry {
        self.0.geometry
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorPanelImageStatus {
    Presented,
    /// No native window was presented. No new input routes were published; stage a retry.
    Deferred,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditorPanelImageError {
    #[error("the panel or dispatch receiver belongs to another semantic panel Host")]
    ForeignSemanticHost,
    #[error("the original native window must already be focused")]
    WindowNotFocused,
    #[error("ordinary panel input is locked by the Application lifecycle")]
    InputLocked,
    #[error("the original panel has no currently accepted image")]
    NotPresented,
    #[error("the current window-modal panel prevents this keyboard focus request")]
    ModalBlocked,
    #[error("the native image panel Host is closed")]
    Closed,
    #[error("native image panels do not support {0:?}")]
    UnsupportedCapability(EditorPanelCapability),
    #[error("the attachment must explicitly declare {0:?}")]
    MissingCapability(EditorPanelCapability),
    #[error("the original panel instance has retired")]
    RetiredInstance,
    #[error("the original panel instance is not attached to this Host")]
    NotAttached,
    #[error("changing an existing attachment's capabilities requires detaching it first")]
    AttachmentConflict,
    #[error("a native window admits at most {EDITOR_PANEL_IMAGE_LIMIT} panel attachments/images")]
    LimitExceeded,
    #[error("a window candidate contains the same panel instance more than once")]
    DuplicatePanel,
    #[error("a window candidate contains the same native display occurrence more than once")]
    DuplicateOccurrence,
    #[error("native window size, DPI, mapping or clip is invalid or no longer current")]
    InvalidGeometry,
    #[error("the image mapping must use the complete original output extent")]
    ExtentMismatch,
    #[error("cropped image UV mappings are unsupported")]
    CroppedImage,
    #[error("the image no longer belongs to a live native device epoch")]
    StaleImage,
    #[error("the candidate belongs to another native image panel Host")]
    ForeignFrame,
    #[error("the candidate has been superseded or already consumed")]
    SupersededFrame,
    #[error("the display frame index must be newer than the last accepted native frame")]
    StaleDisplayFrame,
    #[error("the observation belongs to another native image panel Host")]
    ForeignObservation,
    #[error("the observed output, snapshot or native geometry is no longer accepted")]
    StaleObservation,
    #[error("the observed keyboard endpoint or text-focus lifetime has changed")]
    ChangedInputScope,
    #[error("the observed panel does not own this pointer event")]
    PointerBlocked,
    #[error("window-wide lifecycle events cannot target a panel observation")]
    UnsupportedObservedEvent,
    #[error("the native outcome does not cover the exact window, display frame and all images")]
    IncompletePresentation,
    #[error("native pointer, wheel or IME data is invalid")]
    InvalidEvent,
}

/// Input uses the accepted immutable snapshot, never a pending or newly produced frontend state.
pub struct EditorPanelInput<S> {
    pub endpoint: EditorPanelImageEndpoint,
    pub snapshot: Arc<S>,
    pub kind: EditorPanelInputKind,
    pub modifiers: ModifiersState,
    /// Image target pixels. Captured input may extrapolate beyond the image without clamping.
    pub target_position: Option<[f32; 2]>,
    /// Original provider-local focus lifetime captured with keyboard/text/IME input. It cannot
    /// be substituted by a later scope requested from an earlier callback in this dispatch.
    pub text_scope: Option<EditorPanelTextInputScope>,
}

/// The caller submits this exact original semantic candidate with `submit_and_drive`.
#[must_use]
pub struct EditorPanelInputOutcome {
    pub consumed: bool,
    pub submission: Option<EditorPanelSubmission>,
}

struct Attachment {
    token: EditorPanelInstanceToken,
    capabilities: Capabilities,
    deadline: Option<Instant>,
    observation_lifetime: Option<Arc<()>>,
}

struct PointerCapture<S> {
    image: EditorPanelImage<S>,
    button: MouseButton,
}

struct Delivery<S> {
    image: EditorPanelImage<S>,
    kind: EditorPanelInputKind,
    modifiers: ModifiersState,
    position: Option<[f32; 2]>,
    text_scope: Option<EditorPanelTextInputScope>,
}

#[derive(Default)]
struct NativeIme {
    scope: Option<EditorPanelTextInputScope>,
    scope_owner: Option<EditorPanelInstanceToken>,
    allowed: bool,
    ready: bool,
    waiting_disabled: bool,
    retiring_scope: Option<(EditorPanelInstanceToken, EditorPanelTextInputScope)>,
    area: Option<services::LogicalImeArea>,
    published_area: Option<services::LogicalImeArea>,
}

/// One original native window's bounded image attachment and input/service arbitration.
///
/// This is a source-level cooperative Host. The caller owns the window's event loop, all GPU
/// production/submission/presentation and semantic Application driving. Local fields and IME
/// buffers remain frontend state. Call `dispatch(None)` to flush presentation-time cancellation.
pub struct EditorPanelImageHost<S> {
    owner: Arc<()>,
    semantic: Option<Arc<()>>,
    window: Arc<Window>,
    window_id: WindowId,
    clipboard: EditorClipboard,
    attachments: Vec<Attachment>,
    pending: Option<Arc<ImageCandidate<S>>>,
    accepted: Vec<EditorPanelImage<S>>,
    geometry: EditorPanelWindowGeometry,
    last_display_frame: Option<u64>,
    capture: Option<PointerCapture<S>>,
    keyboard: Option<EditorPanelImage<S>>,
    hovered: Option<EditorPanelImage<S>>,
    cancellations: VecDeque<Delivery<S>>,
    pointer: Option<[f64; 2]>,
    pointer_inside: bool,
    modifiers: ModifiersState,
    focused: bool,
    ime: NativeIme,
    cursor: winit::window::CursorIcon,
    redraw_requested: bool,
    closed: bool,
}
