use std::{fs, os::windows::fs::OpenOptionsExt, thread, time::Instant};

use nara_core::{DocumentId, EntityUuid, vfs::AssetPath};
use nara_project::workflow::ProjectTrust;
use nara_scene::authoring::{SceneDocument, SceneEntity};
use open_gpui::{AppContext as _, Entity, TestAppContext};

use super::*;

fn poll_worker(cx: &TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(5));
    thread::sleep(Duration::from_millis(3));
}

fn wait_until(cx: &TestAppContext, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "the editor did not finish the reload"
        );
        poll_worker(cx);
    }
}

fn watch(workspace: Workspace, cx: &TestAppContext) -> (Entity<Workspace>, Entity<InspectorPanel>) {
    cx.update(|cx| {
        let workspace = cx.new(|_| workspace);
        let inspector = cx.new(|cx| InspectorPanel::new(workspace.clone(), cx));
        workspace.update(cx, |workspace, cx| {
            workspace.watch_assets(inspector.downgrade(), cx);
        });
        (workspace, inspector)
    })
}

#[test]
fn a_split_rename_preserves_the_open_asset_and_malformed_edits_are_terminal() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    fs::write(
        fixture.path().join("nara.toml"),
        "[project]\nname = 'Hot Reload'\nversion = '0.1.0'\n",
    )?;
    let assets = fixture.path().join("assets");
    fs::create_dir(&assets)?;
    let old = assets.join("first.scene.ron");
    let moved = assets.join("moved.scene.ron");
    let mut scene = SceneDocument::new(DocumentId::try_from_uuid(uuid::Uuid::new_v4())?);
    scene.entities.push(SceneEntity::new(
        EntityUuid::try_from_uuid(uuid::Uuid::new_v4())?,
        "Usable",
    ));
    scene.write(&old)?;

    let workspace = Workspace::open(fixture.path(), ProjectTrust::Untrusted)?;
    let asset = workspace.opened_asset.context("no open scene asset")?;
    let cx = TestAppContext::single();
    let (workspace, _inspector) = watch(workspace, &cx);
    wait_until(&cx, || {
        workspace.read_with(&cx, |workspace, _| workspace.preview_assets_revision != 0)
    });
    let prepare_error = || {
        workspace
            .read_with(&cx, |workspace, _| workspace.reload_input().unwrap())
            .prepare(EXTERNAL)
            .err()
            .expect("the reload should fail")
    };

    fs::rename(&old, &moved)?;
    assert!(is_transient_reload_error(prepare_error().as_ref()));
    let prior_revision = workspace.read_with(&cx, |workspace, _| workspace.revision);
    for _ in 0..12 {
        poll_worker(&cx);
    }
    workspace.read_with(&cx, |workspace, _| {
        assert!(workspace.hot_reload.last_error.is_none());
        assert_eq!(workspace.revision, prior_revision);
    });

    scene.entities[0].name = "Latest edit".into();
    scene.write(&moved)?;
    fs::rename(
        assets.join("first.scene.ron.meta"),
        assets.join("moved.scene.ron.meta"),
    )?;
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&moved)?;
    let error = prepare_error();
    assert!(is_transient_reload_error(error.as_ref()), "{error:#}");
    for _ in 0..12 {
        poll_worker(&cx);
    }
    workspace.read_with(&cx, |workspace, _| {
        assert!(workspace.hot_reload.last_error.is_none());
        assert_eq!(workspace.revision, prior_revision);
    });
    drop(guard);

    wait_until(&cx, || {
        workspace.read_with(&cx, |workspace, _| workspace.revision != prior_revision)
    });
    let usable_revision = workspace.read_with(&cx, |workspace, _| {
        assert_eq!(workspace.opened_asset, Some(asset));
        assert_eq!(
            workspace.project.asset_hint(asset),
            Some(AssetPath::new("moved.scene.ron").unwrap())
        );
        assert_eq!(
            workspace.session.document().scene().entities[0].name,
            "Latest edit"
        );
        assert!(workspace.hot_reload.last_error.is_none());
        workspace.revision
    });

    fs::write(&moved, "invalid RON")?;
    assert!(!is_transient_reload_error(prepare_error().as_ref()));
    wait_until(&cx, || {
        workspace.read_with(&cx, |workspace, _| {
            workspace.hot_reload.last_error.is_some()
        })
    });
    workspace.read_with(&cx, |workspace, _| {
        assert_eq!(workspace.revision, usable_revision);
        assert_eq!(
            workspace.session.document().scene().entities[0].name,
            "Latest edit"
        );
        assert!(
            workspace
                .hot_reload
                .last_error
                .as_ref()
                .unwrap()
                .contains("invalid source scene RON")
        );
    });
    Ok(())
}

#[test]
fn a_new_asset_root_is_indexed_even_if_it_appears_before_watch_setup() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    fs::write(
        fixture.path().join("nara.toml"),
        "[project]\nname = 'Hot Reload'\nversion = '0.1.0'\n[paths]\nassets = 'nested/assets'\n",
    )?;
    let workspace = Workspace::open(fixture.path(), ProjectTrust::Untrusted)?;
    assert!(workspace.assets.entries().is_empty());
    let assets = fixture.path().join("nested/assets");
    fs::create_dir_all(&assets)?;
    fs::write(assets.join("manifest.txt"), "created after project open")?;
    fs::write(
        assets.join("manifest.txt.meta"),
        "AssetMeta(id: \"0193a100-1111-7000-8000-000000000001\", importer: \"nara.raw\", settings: ())",
    )?;
    let cx = TestAppContext::single();
    let (workspace, _inspector) = watch(workspace, &cx);
    wait_until(&cx, || {
        workspace.read_with(&cx, |workspace, _| !workspace.assets.entries().is_empty())
    });
    Ok(())
}
