use std::{fs, path::PathBuf, time::Duration};

use nara_core::{DocumentId, EntityUuid, Vec2};
use nara_project::workflow::ProjectTrust;
use nara_scene::authoring::{SceneDocument, SceneEntity};
use open_gpui::{
    AnyWindowHandle, AppContext as _, Context, Entity, Focusable as _, IntoElement, Render,
    TestAppContext, VisualTestContext, Window, WindowOptions, div,
};
use open_gpui_docking::{DockPane, DockSurface, dockspace::policy::DockPolicy};

use super::{QuitEditor, guard_editor_window};
use crate::{
    docking::{PANES, ROOT, default_session},
    inspector::InspectorPanel,
    theme,
    workspace::Workspace,
};

const SCENE: &str = "initial.scene.ron";

struct EmptyPane;

impl Render for EmptyPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Editor {
    cx: TestAppContext,
    window: AnyWindowHandle,
    workspace: Entity<Workspace>,
    inspector: Entity<InspectorPanel>,
    _dock: DockSurface,
    directory: tempfile::TempDir,
}

impl Editor {
    fn open(named: bool, invalid_layout: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let assets = root.join("assets");
        fs::create_dir(&assets).unwrap();
        fs::write(
            root.join("nara.toml"),
            "[project]\nname = 'Close Fixture'\nversion = '0.1.0'\n[paths]\nassets = 'assets'\n",
        )
        .unwrap();
        if named {
            let id = DocumentId::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
            let entity = EntityUuid::try_from_uuid(uuid::Uuid::new_v4()).unwrap();
            let mut scene = SceneDocument::new(id);
            scene.entities.push(SceneEntity::new(entity, "Original"));
            scene.write(assets.join(SCENE)).unwrap();
        }
        let workspace = Workspace::open(root, ProjectTrust::Untrusted).unwrap();
        let layout = invalid_layout.then(|| {
            let path = root.join(".nara");
            fs::write(&path, "not a directory").unwrap();
            path.join("editor-layout.json")
        });
        let cx = TestAppContext::single();
        let (window, workspace, inspector, dock) = cx.update(|cx| {
            let workspace = cx.new(|_| workspace);
            workspace.update(cx, |workspace, cx| {
                if named {
                    let entity = workspace.session.document().scene().entities[0].id;
                    workspace.select([entity], cx);
                } else {
                    workspace.create(Vec2::ZERO, None, None, cx);
                }
            });
            let inspector = cx.new(|cx| InspectorPanel::new(workspace.clone(), cx));
            let mut policy = DockPolicy::default();
            policy.set_allow_native_surfaces(true);
            let session = default_session(policy).unwrap();
            let panes = PANES.map(|key| {
                let item = session.item_id_for_external_key(key).unwrap();
                if key == "inspector" {
                    DockPane::new(item, "Inspector", inspector.clone())
                } else {
                    DockPane::new(item, key, cx.new(|_| EmptyPane))
                }
            });
            let dock = DockSurface::new_managed(session, panes, theme::docking(), cx).unwrap();
            let window = dock
                .open_native_root(ROOT, true, WindowOptions::default(), cx)
                .unwrap();
            guard_editor_window(
                &dock,
                window,
                workspace.downgrade(),
                inspector.downgrade(),
                layout,
                cx,
            )
            .unwrap();
            (window, workspace, inspector, dock)
        });
        cx.run_until_parked();
        Self {
            cx,
            window,
            workspace,
            inspector,
            _dock: dock,
            directory,
        }
    }

    fn edit_name(&self, name: &str) -> VisualTestContext {
        let mut visual = VisualTestContext::from_window(self.window, &self.cx);
        let input = self.inspector.read_with(&self.cx, |inspector, _| {
            inspector.name_input().unwrap().clone()
        });
        visual.update(|window, cx| {
            window.focus(&input.read(cx).focus_handle(cx), cx);
            input.update(cx, |input, cx| input.select_all(window, cx));
        });
        self.cx.run_until_parked();
        visual.simulate_input(name);
        visual
    }

    fn has_drafts(&self) -> bool {
        self.inspector
            .read_with(&self.cx, |inspector, cx| !inspector.drafts(cx).is_empty())
    }

    fn request_close(&self) {
        VisualTestContext::from_window(self.window, &self.cx).simulate_close();
        self.cx.run_until_parked();
    }

    fn answer(&self, button: &str) {
        self.cx.simulate_prompt_answer(button);
        self.cx.run_until_parked();
    }

    fn choose_path(&self, path: Option<PathBuf>) {
        assert!(self.cx.did_prompt_for_new_path());
        self.cx.simulate_new_path_selection(|_| path);
        self.cx.run_until_parked();
    }

    fn is_open(&self) -> bool {
        self.cx.windows().contains(&self.window)
    }

    fn scene_path(&self, name: &str) -> PathBuf {
        self.directory.path().join("assets").join(name)
    }
}

#[test]
fn cancel_and_review_keep_the_draft_open_then_allow_applying_and_saving_it() {
    let editor = Editor::open(true, false);
    let mut visual = editor.edit_name("Edited by the artist");
    assert!(editor.has_drafts());
    editor.request_close();
    editor.answer("Cancel");
    assert!(editor.is_open() && editor.has_drafts());
    editor.request_close();
    editor.answer("Review in Inspector");
    assert!(editor.is_open() && editor.has_drafts());
    visual.simulate_keystrokes("enter");
    assert!(!editor.has_drafts());
    editor.request_close();
    editor.answer("Save and Close");
    assert!(!editor.is_open());
    let scene = SceneDocument::read(editor.scene_path(SCENE)).unwrap();
    assert_eq!(scene.entities[0].name, "Edited by the artist");
}

#[test]
fn first_save_close_keeps_existing_files_and_retries_with_an_exact_new_name() {
    let editor = Editor::open(false, false);
    let occupied = editor.scene_path("taken.scene.ron");
    for path in [
        None,
        Some(editor.scene_path("taken")),
        Some(occupied.clone()),
    ] {
        editor.request_close();
        editor.answer("Save and Close");
        fs::write(&occupied, b"another writer").unwrap();
        editor.choose_path(path);
        assert_eq!(fs::read(&occupied).unwrap(), b"another writer");
        assert!(editor.is_open());
        assert!(editor.workspace.read_with(&editor.cx, |workspace, _| {
            workspace.session.is_dirty() && workspace.session.document().path().is_none()
        }));
    }
    editor.request_close();
    editor.answer("Save and Close");
    let path = editor.scene_path("confirmed.scene.ron");
    editor.choose_path(Some(path.clone()));
    assert!(!editor.is_open());
    assert_eq!(SceneDocument::read(path).unwrap().entities.len(), 1);
}

#[test]
fn a_draft_created_during_save_as_requires_a_new_close_decision() {
    let editor = Editor::open(false, false);
    editor.request_close();
    editor.answer("Save and Close");
    editor.edit_name("Late edit");
    let path = editor.scene_path("new.scene.ron");
    editor.choose_path(Some(path.clone()));
    assert!(editor.is_open() && editor.has_drafts());
    assert!(!path.exists());

    editor.request_close();
    editor.answer("Discard Drafts and Continue");
    editor.answer("Cancel");
    assert!(editor.is_open() && editor.has_drafts());
    editor.request_close();
    editor.answer("Discard Drafts and Continue");
    editor.answer("Discard");
    assert!(!editor.is_open());
    assert!(!path.exists());
}

#[test]
fn a_suspended_draft_requires_a_close_decision_with_a_clean_scene() {
    let mut editor = Editor::open(true, false);
    editor.edit_name("Suspended");
    editor
        .workspace
        .update(&mut editor.cx, |workspace, cx| workspace.select([], cx));
    assert!(editor.has_drafts());
    editor.request_close();
    editor.answer("Discard Drafts and Continue");
    assert!(!editor.is_open());
    let scene = SceneDocument::read(editor.scene_path(SCENE)).unwrap();
    assert_eq!(scene.entities[0].name, "Original");
}

#[test]
fn a_failed_layout_save_can_keep_the_editor_open_or_discard_the_layout() {
    let editor = Editor::open(true, true);
    editor.request_close();
    editor.answer("Keep Editor Open");
    assert!(editor.is_open());
    editor.request_close();
    editor.answer("Close Without Saving Layout");
    assert!(!editor.is_open());
    let layout = editor.directory.path().join(".nara");
    assert_eq!(fs::read(layout).unwrap(), b"not a directory");
}

#[test]
fn deleting_an_entity_or_opening_another_scene_evicts_unreachable_drafts() {
    let mut editor = Editor::open(true, false);
    let second = nara_core::vfs::AssetPath::new("second.scene.ron").unwrap();
    let second_path = editor.scene_path(second.as_str());
    let second_scene = SceneDocument::new(DocumentId::try_from_uuid(uuid::Uuid::new_v4()).unwrap());
    second_scene.write(&second_path).unwrap();

    editor.edit_name("Draft before delete");
    assert!(editor.has_drafts());
    editor.workspace.update(&mut editor.cx, |workspace, cx| {
        workspace.delete_selection(cx)
    });
    editor.cx.run_until_parked();
    assert!(!editor.has_drafts());

    editor
        .workspace
        .update(&mut editor.cx, |workspace, cx| workspace.undo(cx));
    editor.cx.run_until_parked();
    let entity = editor.workspace.read_with(&editor.cx, |workspace, _| {
        workspace.session.document().scene().entities[0].id
    });
    editor.workspace.update(&mut editor.cx, |workspace, cx| {
        workspace.select([entity], cx)
    });
    editor.cx.run_until_parked();

    editor.edit_name("Draft before switching scene");
    assert!(editor.has_drafts());
    editor.workspace.update(&mut editor.cx, |workspace, cx| {
        workspace.refresh_assets(cx);
        workspace.open_asset(&second, cx);
    });
    editor.cx.run_until_parked();
    assert!(!editor.has_drafts());
    editor.request_close();
    assert!(!editor.is_open());
}

#[test]
fn quit_action_routes_through_the_editor_close_guard() {
    let editor = Editor::open(true, false);
    editor.edit_name("Unsaved quit draft");
    assert!(editor.has_drafts());
    editor.cx.update(|cx| cx.dispatch_action(&QuitEditor));
    editor.cx.run_until_parked();
    editor.answer("Cancel");
    assert!(editor.is_open() && editor.has_drafts());
    editor.cx.update(|cx| cx.dispatch_action(&QuitEditor));
    editor.cx.run_until_parked();
    editor.answer("Discard Drafts and Continue");
    assert!(!editor.is_open());
}

#[test]
fn canceling_name_draft_with_escape_resets_text_and_clears_draft() {
    let editor = Editor::open(true, false);
    let mut visual = editor.edit_name("Temporary");
    assert!(editor.has_drafts());
    visual.simulate_keystrokes("escape");
    editor.cx.run_until_parked();
    assert!(!editor.has_drafts());
    let text = editor.inspector.read_with(&editor.cx, |inspector, cx| {
        inspector.name_input().unwrap().read(cx).value().to_owned()
    });
    assert_eq!(text, "Original");
    editor.request_close();
    assert!(!editor.is_open());
}

#[test]
fn source_refresh_waits_for_an_inspector_draft_before_publishing() {
    let mut editor = Editor::open(true, false);
    let mut visual = editor.edit_name("Inspector draft");
    let native = editor.scene_path(SCENE);
    let mut changed = SceneDocument::read(&native).unwrap();
    changed.entities[0].name = "External write".into();
    changed.write(&native).unwrap();

    let inspector = editor.inspector.downgrade();
    editor.workspace.update(&mut editor.cx, |workspace, cx| {
        workspace.watch_assets(inspector, cx);
        workspace.refresh_assets(cx);
    });
    let settle = || {
        for _ in 0..4 {
            editor.cx.run_until_parked();
            editor
                .cx
                .executor()
                .advance_clock(Duration::from_millis(100));
        }
        editor.cx.run_until_parked();
    };
    let name = || {
        editor.workspace.read_with(&editor.cx, |workspace, _| {
            workspace.session.document().scene().entities[0]
                .name
                .clone()
        })
    };
    settle();
    assert_eq!(name(), "Original");
    assert!(editor.has_drafts());

    visual.simulate_keystrokes("escape");
    settle();
    assert_eq!(name(), "External write");
    assert!(!editor.has_drafts());
    editor.workspace.read_with(&editor.cx, |workspace, _| {
        assert!(workspace.session.can_undo());
        assert!(!workspace.session.is_dirty());
    });
}
