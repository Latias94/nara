//! Pure policy/geometry checks. Original GPU receipts and native event delivery belong to the
//! independent native consumers; these tests never issue or fabricate backend acceptance.

use std::sync::Arc;

use nara::{
    render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d},
    scene::{SceneAuthoringSession, SceneDocument},
    tooling::{
        EditorFrameClock, EditorWorkspace, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
    },
};
use winit::{
    event::{ElementState, Ime, MouseButton},
    keyboard::{Key, NamedKey},
};

use super::{
    Capabilities, EditorClipboard, EditorPanelCapability as C, EditorPanelImageError,
    EditorPanelImeArea, EditorPanelPlatformEvent, EditorPanelServiceError, EditorPanelServices,
    EditorPanelTextInputScope, EditorPanelWindowGeometry, ImageInputPolicy, geometry,
    services::ServiceRequests,
};

fn workspace_decision() -> (EditorFrameClock, EditorWorkspaceIntentView) {
    let mut workspace = EditorWorkspace::new();
    workspace
        .open_scene_session(
            "Source",
            SceneAuthoringSession::new(SceneDocument::default()),
        )
        .unwrap();
    let document = workspace.active_document().unwrap();
    let mut clock = EditorFrameClock::new();
    let prompt = clock.issue_close_prompt(document);
    let view = EditorWorkspaceIntentView::new(
        Some(EditorWorkspaceIntent::CloseDocument { document }),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision),
        Some(document),
        Some(prompt),
        None,
    );
    (clock, view)
}

#[test]
fn ordinary_images_stay_locked_and_empty_decisions_grant_no_exception() {
    let (_, decision) = workspace_decision();
    let ordinary = ImageInputPolicy::default();
    assert_eq!(ordinary, ImageInputPolicy::Ordinary);
    assert!(ordinary.allows_input(false, Some(decision)));
    assert!(!ordinary.allows_input(true, Some(decision)));
    assert!(!ordinary.allows_input(true, None));
    for current in [
        EditorWorkspaceIntentView::default(),
        EditorWorkspaceIntentView::new(
            Some(EditorWorkspaceIntent::Exit),
            Some(EditorWorkspaceIntentPhase::RetiringRuntime),
            None,
            None,
            None,
        ),
    ] {
        let policy = ImageInputPolicy::WorkspaceDecision(current);
        assert!(!policy.allows_input(true, Some(current)));
        assert!(!policy.allows_input(false, Some(current)));
    }
}

#[test]
fn workspace_decision_input_expires_with_the_exact_prompt_or_view() {
    let (mut clock, current) = workspace_decision();
    let policy = ImageInputPolicy::WorkspaceDecision(current);
    assert!(policy.allows_input(true, Some(current)));
    assert!(policy.allows_input(false, Some(current)));
    assert!(!policy.allows_input(true, None));
    let document = current.decision_document().unwrap();
    let replacement_prompt = clock.issue_close_prompt(document);
    let replacement = EditorWorkspaceIntentView::new(
        current.intent(),
        current.phase(),
        Some(document),
        Some(replacement_prompt),
        None,
    );
    let saving = EditorWorkspaceIntentView::new(
        current.intent(),
        Some(EditorWorkspaceIntentPhase::Saving),
        Some(document),
        current.decision_prompt(),
        None,
    );
    for successor in [replacement, saving, EditorWorkspaceIntentView::default()] {
        assert!(!policy.allows_input(true, Some(successor)));
        assert!(!policy.allows_input(false, Some(successor)));
        assert_ne!(policy, ImageInputPolicy::WorkspaceDecision(successor));
    }
}

#[test]
fn terminal_workspace_decision_input_expires_after_acknowledgement() {
    let current = EditorWorkspaceIntentView::new(
        None,
        None,
        None,
        None,
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::Exit,
        }),
    );
    let policy = ImageInputPolicy::WorkspaceDecision(current);
    assert!(policy.allows_input(true, Some(current)));
    assert!(policy.allows_input(false, Some(current)));
    assert!(!policy.allows_input(true, Some(EditorWorkspaceIntentView::default())));
    assert!(!policy.allows_input(false, None));
}

fn mapping() -> DisplayMapping2d {
    DisplayMapping2d::fit(
        Extent2d::new(200, 100).unwrap(),
        DisplayRect {
            min: [10.25, 20.5],
            max: [210.25, 170.5],
        },
        1.5,
        DisplayScale::Fit,
    )
    .unwrap()
}

#[test]
fn fractional_native_mapping_rejects_clip_and_letterbox_but_extrapolates_capture() {
    let geometry = EditorPanelWindowGeometry {
        physical_size: [600, 450],
        scale_factor: 1.5,
    };
    let mapping = mapping();
    let clip = DisplayRect {
        min: [20.25, 60.5],
        max: [150.25, 130.5],
    };
    let logical = geometry.logical_position([165.375, 143.25]).unwrap();
    assert_eq!(logical, [110.25, 95.5]);
    assert_eq!(
        geometry::target_position(mapping, logical, false),
        Some([100.0, 50.0])
    );
    assert!(geometry::hit_mapping(mapping, clip, logical));
    assert!(!geometry::hit_mapping(mapping, clip, [15.25, 95.5]));
    assert!(!geometry::hit_mapping(
        mapping,
        mapping.panel_rect(),
        [110.25, 30.5]
    ));
    let outside = [-9.75, 195.5];
    assert_eq!(geometry::target_position(mapping, outside, false), None);
    assert_eq!(
        geometry::target_position(mapping, outside, true),
        Some([-20.0, 150.0])
    );
    assert_eq!(
        geometry::target_position(mapping, logical, true),
        mapping.panel_to_target(logical)
    );
    assert_eq!(
        geometry::target_position(mapping, [f32::NAN, 0.0], true),
        None
    );
}

#[test]
fn invalid_native_geometry_never_produces_input_coordinates() {
    for geometry in [
        EditorPanelWindowGeometry {
            physical_size: [0, 10],
            scale_factor: 1.0,
        },
        EditorPanelWindowGeometry {
            physical_size: [10, 10],
            scale_factor: 0.0,
        },
        EditorPanelWindowGeometry {
            physical_size: [10, 10],
            scale_factor: f64::NAN,
        },
        EditorPanelWindowGeometry {
            physical_size: [10, 10],
            scale_factor: f64::MAX,
        },
    ] {
        assert_eq!(geometry.logical_size(), None);
        assert_eq!(geometry.logical_position([0.0, 0.0]), None);
    }
}

#[test]
fn unsupported_capabilities_and_omitted_prerequisites_refuse_admission() {
    for capability in [
        C::OutsidePanelOverlay,
        C::DirectRendering,
        C::NativeDetach,
        C::Accessibility,
    ] {
        assert!(matches!(Capabilities::admit([C::Pointer, capability]),
            Err(EditorPanelImageError::UnsupportedCapability(actual)) if actual == capability));
    }
    assert!(matches!(
        Capabilities::admit([C::TextInput]),
        Err(EditorPanelImageError::MissingCapability(C::Keyboard))
    ));
    assert!(matches!(
        Capabilities::admit([C::WindowModal, C::Keyboard]),
        Err(EditorPanelImageError::MissingCapability(C::Pointer))
    ));
    let caps =
        Capabilities::admit([C::Pointer, C::Keyboard, C::WindowModal, C::TextInput]).unwrap();
    assert!(caps.has(C::WindowModal));
    assert!(!caps.has(C::Clipboard));
}

#[test]
fn scopes_are_nominal_and_clones_retain_only_the_original_focus_lifetime() {
    let field_a = EditorPanelTextInputScope::new();
    let captured = field_a.clone();
    let field_b = EditorPanelTextInputScope::default();
    assert_eq!(field_a, captured);
    assert_ne!(field_a, field_b);
    drop(field_a);
    assert_eq!(captured, captured.clone());
    assert_ne!(captured, EditorPanelTextInputScope::new());
}

struct UnavailableClipboard;

impl super::super::EditorClipboardAccess for UnavailableClipboard {
    fn read(&mut self) -> Result<String, String> {
        Err("original provider refused read".into())
    }
    fn write(&mut self, _: &str) -> Result<(), String> {
        Err("original provider refused write".into())
    }
}

fn services(clipboard: &EditorClipboard) -> EditorPanelServices<'_> {
    EditorPanelServices {
        clipboard,
        capabilities: Capabilities::admit([
            C::Pointer,
            C::Keyboard,
            C::TextInput,
            C::Clipboard,
            C::Cursor,
            C::Refresh,
        ])
        .unwrap(),
        active: true,
        keyboard_owner: true,
        pointer_owner: true,
        capture_owner: false,
        captured_button: None,
        pressed: None,
        ime_ready: true,
        current_text_scope: None,
        delivery_text_scope: None,
        mapping: mapping(),
        clip: mapping().content_rect(),
        requests: ServiceRequests::default(),
    }
}

#[test]
fn ime_caret_rejects_invisible_points_and_preserves_zero_width_clip_edges() {
    let clipboard = EditorClipboard::with_access(UnavailableClipboard);
    let mut services = services(&clipboard);
    let content = services.mapping.content_rect();
    services.clip = DisplayRect {
        min: content.min,
        max: [content.min[0] + 100.0, content.max[1]],
    };
    for area in [
        EditorPanelImeArea {
            position: [150.0, 20.0],
            size: [0.0, 16.0],
        },
        EditorPanelImeArea {
            position: [100.25, 20.0],
            size: [0.0, 16.0],
        },
    ] {
        assert_eq!(
            services.set_ime_area(area),
            Err(EditorPanelServiceError::InvalidImeArea)
        );
        assert!(services.requests.ime_area.is_none());
    }
    assert!(
        services
            .set_ime_area(EditorPanelImeArea {
                position: [100.0, 20.0],
                size: [0.0, 16.0]
            })
            .is_ok()
    );
    assert!(
        services
            .set_ime_area(EditorPanelImeArea {
                position: [90.0, 20.0],
                size: [11.0, 16.0]
            })
            .is_ok()
    );
    assert!(
        services
            .set_ime_area(EditorPanelImeArea {
                position: [0.0, 0.0],
                size: [100.0, 100.0]
            })
            .is_ok()
    );
    services.clip.min[1] = content.min[1] + 10.0;
    assert!(
        services
            .set_ime_area(EditorPanelImeArea {
                position: [50.0, 0.0],
                size: [0.0, 16.0]
            })
            .is_ok()
    );
}

#[test]
fn changing_local_focus_removes_reported_readiness_without_requiring_a_panel_change() {
    let clipboard = EditorClipboard::with_access(UnavailableClipboard);
    let mut services = services(&clipboard);
    let original = EditorPanelTextInputScope::new();
    services.current_text_scope = Some(original.clone());
    services.delivery_text_scope = Some(original.clone());
    services.set_text_input(Some(original)).unwrap();
    assert!(services.ime_ready().unwrap());
    services
        .set_text_input(Some(EditorPanelTextInputScope::new()))
        .unwrap();
    assert!(!services.ime_ready().unwrap());
    services.set_text_input(None).unwrap();
    assert!(!services.ime_ready().unwrap());
}

#[test]
fn original_text_scope_cannot_disable_or_move_a_successor_fields_native_ime() {
    let clipboard = EditorClipboard::with_access(UnavailableClipboard);
    let mut services = services(&clipboard);
    services.current_text_scope = Some(EditorPanelTextInputScope::new());
    services.delivery_text_scope = Some(EditorPanelTextInputScope::new());
    assert_eq!(
        services.set_text_input(None),
        Err(EditorPanelServiceError::StaleTextInputScope)
    );
    assert_eq!(
        services.set_ime_area(EditorPanelImeArea {
            position: [10.0, 20.0],
            size: [0.0, 15.0]
        }),
        Err(EditorPanelServiceError::StaleTextInputScope)
    );
    assert_eq!(
        services.ime_ready(),
        Err(EditorPanelServiceError::StaleTextInputScope)
    );
}

#[test]
fn capability_and_pointer_ownership_failures_are_explicit_and_preserve_provider_errors() {
    let clipboard = EditorClipboard::with_access(UnavailableClipboard);
    let mut services = services(&clipboard);
    assert_eq!(
        services.read_clipboard(),
        Err(EditorPanelServiceError::Clipboard(
            "original provider refused read".into()
        ))
    );
    assert_eq!(
        services.request_pointer_capture(MouseButton::Left),
        Err(EditorPanelServiceError::CaptureRequiresPress)
    );
    services.pressed = Some(MouseButton::Right);
    services.captured_button = Some(MouseButton::Left);
    assert_eq!(
        services.request_pointer_capture(MouseButton::Right),
        Err(EditorPanelServiceError::CaptureButtonConflict)
    );
    services.capabilities = Capabilities::default();
    assert_eq!(
        services.write_clipboard("text"),
        Err(EditorPanelServiceError::CapabilityNotDeclared(C::Clipboard))
    );
    assert_eq!(
        services.request_redraw(),
        Err(EditorPanelServiceError::CapabilityNotDeclared(C::Refresh))
    );
    services.capabilities = Capabilities::admit([C::Keyboard, C::TextInput]).unwrap();
    services.keyboard_owner = false;
    assert_eq!(
        services.set_text_input(Some(EditorPanelTextInputScope::new())),
        Err(EditorPanelServiceError::NotKeyboardOwner)
    );
}

#[test]
fn injected_keys_keep_explicit_text_and_ime_offsets_require_utf8_boundaries() {
    let key = EditorPanelPlatformEvent::logical_key(
        Key::Named(NamedKey::Enter),
        ElementState::Pressed,
        true,
        Some("\r".into()),
    );
    match key {
        EditorPanelPlatformEvent::Keyboard { key, text } => {
            assert_eq!(key.logical_key, Key::Named(NamedKey::Enter));
            assert!(key.repeat);
            assert_eq!(key.physical_key, None);
            assert_eq!(text.as_deref(), Some("\r"));
        }
        _ => panic!("logical_key constructs normalized keyboard input"),
    }
    assert!(
        EditorPanelPlatformEvent::Ime(Ime::Preedit("中".into(), Some((0, 3))))
            .validate()
            .is_ok()
    );
    assert!(
        EditorPanelPlatformEvent::Ime(Ime::Preedit("中".into(), Some((1, 3))))
            .validate()
            .is_err()
    );
    assert!(
        EditorPanelPlatformEvent::Ime(Ime::Preedit("中".into(), Some((3, 0))))
            .validate()
            .is_err()
    );
}

#[allow(dead_code)]
fn image_clone_does_not_require_snapshot_clone(
    image: &super::EditorPanelImage<std::sync::Mutex<()>>,
) {
    let _retained: super::EditorPanelImage<std::sync::Mutex<()>> = image.clone();
    let _snapshot: Arc<std::sync::Mutex<()>> = Arc::clone(image.snapshot());
}
