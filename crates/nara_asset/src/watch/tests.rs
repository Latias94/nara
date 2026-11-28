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
fn a_continuous_burst_cannot_postpone_a_batch_indefinitely() {
    let start = Instant::now();
    let step = QUIET_INTERVAL / 2;
    let mut changes = PendingChange::default();
    for elapsed in (0..10).map(|index| step * index) {
        changes.record(start + elapsed, false);
        assert!(!changes.ready(start + elapsed));
    }
    changes.record(start + step * 10, true);
    assert_eq!(changes.take(start + step * 10), Some(true));
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
    for hidden in [
        ".nara-write-file.tmp",
        "nested/.nara-meta-file",
        ".stage/ship.png",
    ] {
        assert_eq!(event(rename, &root.join(hidden)), None);
    }
    assert_eq!(
        relevant_change(
            Ok(Event::new(rename)
                .add_path(root.join(".nara-write-file.tmp"))
                .add_path(root.join("ship.png"))),
            &root,
        ),
        Some(false),
    );
}

fn wait_for_change(watcher: &AssetWatcher) -> notify::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watcher.take_changed()? {
        assert!(Instant::now() < deadline, "missing asset notification");
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[test]
fn a_missing_asset_tree_is_watched_when_created_and_replaced() -> notify::Result<()> {
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
fn a_sharing_violation_retries_mounting_without_another_notification() -> notify::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
    let project = tempfile::tempdir()?;
    let assets = project.path().join("assets");
    let watcher = AssetWatcher::under(project.path(), &assets)?;
    fs::create_dir(&assets)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&assets)?;
    assert!(wait_for_change(&watcher).is_err());
    assert!(watcher.watches.lock()?.mounted.is_empty());
    assert!(!watcher.change_is_ready());
    drop(lock);

    wait_for_change(&watcher)?;
    assert_eq!(watcher.watches.lock()?.mounted.last(), Some(&watcher.root));
    fs::write(assets.join("ship.png"), b"edit after recovering the watch")?;
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
