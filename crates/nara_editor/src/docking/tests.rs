use std::{fs, io};

use open_gpui_docking::dockspace::{
    close::CloseDecision,
    geometry::LogicalRect,
    model::{
        DockPlacement, DockspaceActionRejection, DockspaceContainedLayout, FloatingPresentationId,
    },
    runtime::{DockspaceHostFrame, SurfaceUnavailableReason},
};

use super::*;

const PEER: SurfaceId = SurfaceId::new(2);
const CONTAINED_ROOT: RootId = RootId::new(3);

#[test]
fn first_scene_save_keeps_the_existing_file_and_preserves_undo_on_retry() {
    use nara_core::{DocumentId, EntityUuid};
    use nara_editor_core::{EditorCommand, EditorDocument, EditorSession};
    use nara_scene::authoring::{SceneDocument, SceneEntity};

    let directory = tempfile::tempdir().unwrap();
    let occupied = directory.path().join("existing.scene.ron");
    let available = directory.path().join("new.scene.ron");
    let original = b"another writer already owns this file";
    fs::write(&occupied, original).unwrap();

    let id = DocumentId::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
    let document = EditorDocument::new(SceneDocument::new(id)).unwrap();
    let mut session = EditorSession::new(document, Default::default());
    let entity = EntityUuid::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
    assert!(
        session
            .apply(EditorCommand::Create(SceneEntity::new(
                entity,
                "New entity"
            )))
            .unwrap()
    );

    assert!(session.save_new(&occupied).is_err());
    assert_eq!(fs::read(&occupied).unwrap(), original);
    assert!(session.document().path().is_none());
    assert!(session.is_dirty());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);

    session.save_new(&available).unwrap();
    assert_eq!(session.document().path(), Some(available.as_path()));
    assert_eq!(
        SceneDocument::read(&available).unwrap(),
        *session.document().scene()
    );
    assert!(!session.is_dirty());
    assert!(session.undo().unwrap());
    assert!(session.is_dirty());
    assert!(session.redo().unwrap());
    assert!(!session.is_dirty());
}

#[test]
fn required_tab_and_contained_layout_close_requests_leave_panes_open() {
    let (mut session, _) = layout_fixture(false);
    let scene = session.item_id_for_external_key("scene").unwrap();
    let console = session.item_id_for_external_key("console").unwrap();

    for target in [None, Some(CONTAINED_ROOT)] {
        let mut frame = session.begin_host_frame().unwrap();
        match target {
            Some(root) => frame.request_close_root_current(root).unwrap(),
            None => frame.request_close_item_current(scene).unwrap(),
        };
        commit(frame);
        let plan = session.pending_content_close_plans().next().unwrap();
        let mut frame = session.begin_host_frame().unwrap();
        assert!(close::veto_content_close(&mut frame, plan.request()).unwrap());
        commit(frame);
        assert!(session.pending_content_close_plans().next().is_none());
        assert_eq!(session.view().item(scene).unwrap().surface(), ROOT);
        assert_eq!(session.view().item(console).unwrap().root(), CONTAINED_ROOT);
    }
}

#[test]
fn layout_round_trip_preserves_all_panes_and_peer_placement() {
    let root = tempfile::tempdir().unwrap();
    let path = layout_path(root.path());
    let (mut session, host) = layout_fixture(false);
    let document = DockWorkspaceDocument::capture(&mut session, host.clone()).unwrap();
    layout_file::write(&path, &document.to_json().unwrap()).unwrap();

    let restored = restore(&path, session.policy().clone()).unwrap().unwrap();
    assert_eq!(restored.host, host);
    for key in PANES {
        let item = session.item_id_for_external_key(key).unwrap();
        assert_eq!(restored.session.item_id_for_external_key(key), Some(item));
        let original = session.view().item(item).unwrap();
        let restored = restored.session.view().item(item).unwrap();
        assert_eq!(restored.surface(), original.surface());
        assert_eq!(restored.contained(), original.contained());
    }
}

#[test]
fn restored_peer_pane_can_redock_from_an_independent_root() {
    let (mut session, host) = layout_fixture(false);
    let document = DockWorkspaceDocument::capture(&mut session, host).unwrap();
    let mut session = document
        .restore(session.policy().clone(), |_, key| PANES.contains(&key))
        .unwrap()
        .session;
    let peer_root = RootId::new(2);
    assert_eq!(
        session
            .presentation_commands(peer_root)
            .dock_back()
            .unwrap()
            .unavailable_reason(),
        Some(DockspaceActionRejection::DockBackUnavailable { root: peer_root }),
    );
    let scene = session.item_id_for_external_key("scene").unwrap();
    let inspector = session.item_id_for_external_key("inspector").unwrap();
    let mut frame = session.begin_host_frame().unwrap();
    frame
        .dock_item_current(inspector, DockPlacement::After(scene))
        .unwrap();
    commit(frame);
    assert_eq!(session.view().item(inspector).unwrap().surface(), ROOT);
    require_full_layout(&session.view(), |key| session.item_id_for_external_key(key)).unwrap();
}

#[test]
fn layout_restore_rejects_closed_or_hidden_required_panes() {
    let root = tempfile::tempdir().unwrap();
    let path = layout_path(root.path());
    for hidden in [false, true] {
        let (mut session, host) = layout_fixture(false);
        let assets = session.item_id_for_external_key("assets").unwrap();
        let mut frame = session.begin_host_frame().unwrap();
        if hidden {
            frame.set_item_visibility_current(assets, false).unwrap();
            commit(frame);
        } else {
            frame.request_close_item_current(assets).unwrap();
            commit(frame);
            let plan = session.pending_content_close_plans().next().unwrap();
            let mut frame = session.begin_host_frame().unwrap();
            let decision = frame
                .pending_close_decisions(plan.request())
                .next()
                .unwrap();
            frame
                .resolve_close(plan.request(), decision.token(), CloseDecision::Allow)
                .unwrap();
            commit(frame);
        }
        let document = DockWorkspaceDocument::capture(&mut session, host).unwrap();
        layout_file::write(&path, &document.to_json().unwrap()).unwrap();
        let error = restore(&path, session.policy().clone()).unwrap_err();
        assert!(format!("{error:#}").contains("pane \"assets\" is missing or hidden"));
    }
}

#[test]
fn layout_restore_rejects_a_peer_without_an_editor_pane() {
    let root = tempfile::tempdir().unwrap();
    let path = layout_path(root.path());
    let (mut session, host) = layout_fixture(true);
    let document = DockWorkspaceDocument::capture(&mut session, host).unwrap();
    layout_file::write(&path, &document.to_json().unwrap()).unwrap();

    let error = restore(&path, session.policy().clone()).unwrap_err();
    assert!(format!("{error:#}").contains("has no required pane"));
}

#[test]
fn layout_file_is_bounded_and_replaces_only_regular_files() {
    let root = tempfile::tempdir().unwrap();
    let path = layout_path(root.path());
    assert!(layout_file::read(&path).unwrap().is_none());
    assert!(!path.parent().unwrap().exists());

    let limit = layout_file::MAX_LAYOUT_BYTES;
    let exact = vec![b'x'; limit];
    layout_file::write(&path, &exact).unwrap();
    assert_eq!(layout_file::read(&path).unwrap().unwrap(), exact);
    assert!(layout_file::write(&path, &vec![b'x'; limit + 1]).is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), limit as u64);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(limit as u64 + 1)
        .unwrap();
    let error = layout_file::read(&path).unwrap_err();
    assert!(format!("{error:#}").contains(&format!("{limit}-byte limit")));

    layout_file::write(&path, b"replacement").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(layout_file::read(&path).is_err());
    assert!(layout_file::write(&path, b"rejected").is_err());
    assert!(path.is_dir());
}

#[test]
fn layout_file_rejects_linked_directory_and_target_on_read_and_write() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let path = layout_path(root.path());
    let directory = path.parent().unwrap();
    let external_file = external.path().join(path.file_name().unwrap());
    fs::write(&external_file, b"external").unwrap();
    if !can_create_link(link_dir(external.path(), directory)) {
        return;
    }
    assert_rejected_link(&path, "layout directory");
    assert_eq!(fs::read(&external_file).unwrap(), b"external");
    remove_link_dir(directory).unwrap();

    fs::create_dir(directory).unwrap();
    assert!(can_create_link(link_file(&external_file, &path)));
    assert_rejected_link(&path, "regular file");
    assert_eq!(fs::read(&external_file).unwrap(), b"external");
    fs::remove_file(&external_file).unwrap();
    assert_rejected_link(&path, "regular file");
    assert!(!external_file.exists());
}

fn assert_rejected_link(path: &Path, reason: &str) {
    for error in [
        layout_file::read(path).unwrap_err(),
        layout_file::write(path, b"rejected").unwrap_err(),
    ] {
        let error = format!("{error:#}");
        assert!(
            error.contains(reason) && error.contains("editor-layout.json"),
            "{error}"
        );
    }
    assert!(fs::symlink_metadata(path.parent().unwrap()).is_ok());
}

fn layout_path(project: &Path) -> PathBuf {
    project.join(".nara").join("editor-layout.json")
}

fn layout_fixture(add_empty_peer: bool) -> (DockspaceSession, DockHostDocument) {
    let mut keys = DockspaceDocumentBootstrap::new(DockspaceDocumentId::from_bytes([1; 16]));
    let [hierarchy, scene, inspector, assets, console] =
        PANES.map(|key| keys.ensure_item(key).unwrap());
    let contained = DockspaceContainedLayout::new(
        FloatingPresentationId::new(1),
        DockspaceRootLayout::new(CONTAINED_ROOT, DockspaceNode::tabs([console])),
        LogicalRect::new(40.0, 50.0, 240.0, 190.0).unwrap(),
    );
    let mut surfaces = vec![
        DockspaceSurfaceLayout::new(
            ROOT,
            DockspaceRootLayout::new(
                RootId::new(1),
                DockspaceNode::tabs([hierarchy, scene, assets]),
            ),
        )
        .with_contained(contained),
        DockspaceSurfaceLayout::new(
            PEER,
            DockspaceRootLayout::new(RootId::new(2), DockspaceNode::tabs([inspector])),
        ),
    ];
    if add_empty_peer {
        surfaces.push(DockspaceSurfaceLayout::new(
            SurfaceId::new(3),
            DockspaceRootLayout::new(RootId::new(4), DockspaceNode::central_tabs([])),
        ));
    }
    let layout = DockspaceLayout::new(surfaces).unwrap();
    let mut policy = DockPolicy::default();
    policy.set_allow_native_surfaces(true);
    policy.set_allow_contained_floating(true);
    let session = DockspaceSession::from_persistent_layout(layout, policy, keys).unwrap();
    let host = DockHostDocument {
        surfaces: session
            .view()
            .surfaces()
            .map(|surface| surface.id())
            .map(|surface| DockSurfaceKeyBinding {
                space_key: format!("editor-{}", surface.get()),
                surface,
                logical_placement: (surface == PEER).then_some(DockLogicalViewportPlacement {
                    display_id: Some(7),
                    window_bounds: Some(DockLogicalWindowPlacement {
                        state: DockWindowPlacementState::Windowed,
                        bounds: DockLogicalPlacementRect {
                            x: 690.0,
                            y: 210.0,
                            width: 420.0,
                            height: 500.0,
                        },
                    }),
                    host_bounds: None,
                }),
                central_policy: None,
            })
            .collect(),
        migration: None,
    };
    (session, host)
}

fn commit(mut frame: DockspaceHostFrame<'_>) {
    frame
        .complete_unpainted_surfaces(SurfaceUnavailableReason::Deferred)
        .unwrap();
    frame.commit().unwrap();
}

fn can_create_link(result: io::Result<()>) -> bool {
    #[cfg(windows)]
    if result
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(1314))
    {
        return false;
    }
    result.unwrap();
    true
}

#[cfg(unix)]
use {
    fs::remove_file as remove_link_dir, std::os::unix::fs::symlink as link_dir,
    std::os::unix::fs::symlink as link_file,
};

#[cfg(windows)]
use {
    fs::remove_dir as remove_link_dir, std::os::windows::fs::symlink_dir as link_dir,
    std::os::windows::fs::symlink_file as link_file,
};
