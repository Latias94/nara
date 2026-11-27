use std::thread;

use notify::{Event, event::RenameMode};

use super::*;

#[test]
fn a_burst_settles_after_its_last_event_and_leaves_the_next_batch_pending() {
    let start = Instant::now();
    let half = QUIET_INTERVAL / 2;
    let mut changes = PendingChange::default();
    changes.record(start, true);
    changes.record(start + half, false);
    assert_eq!(changes.take(start + QUIET_INTERVAL), None);
    let settled = start + QUIET_INTERVAL + half;
    assert_eq!(changes.take(settled), Some(true));
    changes.record(settled, false);
    assert_eq!(changes.take(settled), None);
    assert_eq!(changes.take(settled + QUIET_INTERVAL), Some(false));
    assert_eq!(changes.take(settled + QUIET_INTERVAL), None);
}

#[test]
fn only_the_asset_tree_and_its_mount_path_are_relevant() {
    let root = Path::new("project").join("content").join("assets");
    let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
    let event =
        |kind, path: &Path| relevant_change(Ok(Event::new(kind).add_path(path.into())), &root);
    assert_eq!(event(rename, Path::new("project/target")), None);
    assert_eq!(event(rename, Path::new("project/content/unrelated")), None);
    assert_eq!(
        event(
            EventKind::Modify(ModifyKind::Any),
            Path::new("project/content")
        ),
        None
    );
    assert_eq!(event(rename, Path::new("project/content")), Some(true));
    assert_eq!(event(rename, &root), Some(true));
    assert_eq!(event(rename, &root.join("ship.png")), Some(false));
    assert_eq!(
        relevant_change(
            Ok(Event::new(rename)
                .add_path(PathBuf::from("project/staged.png"))
                .add_path(root.join("ship.png"))),
            &root,
        ),
        Some(false),
    );
}

#[test]
fn a_missing_asset_tree_is_watched_when_created_and_replaced() -> notify::Result<()> {
    fn wait_for_change(watcher: &AssetWatcher) -> notify::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !watcher.take_changed()? {
            assert!(Instant::now() < deadline, "missing asset notification");
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    let project = tempfile::tempdir()?;
    let folder = project.path().join("content");
    let assets = folder.join("assets");
    let watcher = AssetWatcher::under(project.path(), &assets)?;
    assert!(watcher.watches.lock()?.mounted.is_empty());
    fs::create_dir_all(&assets)?;
    wait_for_change(&watcher)?;
    assert_eq!(watcher.watches.lock()?.mounted.last(), Some(&watcher.root));

    fs::rename(&assets, folder.join("retired"))?;
    fs::create_dir_all(&assets)?;
    wait_for_change(&watcher)?;
    let nested = assets.join("nested");
    fs::create_dir(&nested)?;
    fs::write(nested.join("ship.png.meta"), b"replacement")?;
    wait_for_change(&watcher)?;
    fs::write(nested.join("ship.png.meta"), b"edit in the new tree")?;
    wait_for_change(&watcher)
}

#[cfg(windows)]
#[test]
fn sharing_and_lock_violations_are_retryable_but_access_denial_is_terminal() {
    for code in [32, 33] {
        assert!(is_transient_reload_error(&io::Error::from_raw_os_error(
            code
        )));
    }
    for code in [2, 3, 5] {
        assert!(!is_transient_reload_error(&io::Error::from_raw_os_error(
            code
        )));
    }
    assert!(!is_transient_reload_error(&io::Error::other(
        "sharing violation"
    )));
}
