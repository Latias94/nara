use std::{fs, path::PathBuf};

use super::*;
use crate::{
    levels::{LevelId, LevelKey},
    save::CheckpointLocation,
};

fn checkpoint(score: u32) -> Checkpoint {
    let mut value = Checkpoint::new(CheckpointLocation {
        level: LevelKey::main(LevelId::Cavern),
        anchor: nara::identity::SceneEntityId::new("checkpoint").unwrap(),
    });
    value.score = score;
    value
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "two-level-save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> PathBuf {
        self.0.join("checkpoint.json")
    }

    fn marker_path(&self) -> PathBuf {
        self.0.join(".two-level-lock-checkpoint.json")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn published_save_is_bounded_readback_and_reopens_in_an_independent_owner() {
    let directory = Directory::new();
    let mut slot = SaveSlot::open(&directory.path()).unwrap();
    let first = checkpoint(10);
    let receipt = slot.save(&first).unwrap();
    let bytes = fs::read(directory.path()).unwrap();
    assert_eq!(receipt.digest, ContentDigest::of_bytes(&bytes));
    drop(slot);
    let mut reopened = SaveSlot::open(&directory.path()).unwrap();
    assert_eq!(reopened.load().unwrap().checkpoint, first);
    reopened.save(&checkpoint(20)).unwrap();
    assert_eq!(reopened.load().unwrap().checkpoint.score, 20);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn competing_owner_and_same_identity_external_bytes_are_not_overwritten() {
    let directory = Directory::new();
    SaveSlot::open(&directory.path())
        .unwrap()
        .save(&checkpoint(1))
        .unwrap();
    let mut stale = SaveSlot::open(&directory.path()).unwrap();
    let mut other = SaveSlot::open(&directory.path()).unwrap();
    other.save(&checkpoint(2)).unwrap();
    assert!(matches!(
        stale.save(&checkpoint(3)),
        Err(SaveIoError::Conflict)
    ));
    assert_eq!(
        decode_checkpoint(&fs::read(directory.path()).unwrap())
            .unwrap()
            .checkpoint
            .score,
        2
    );
    let mut stale = SaveSlot::open(&directory.path()).unwrap();
    let identity = stale
        .parent
        .open_child_file(&stale.target)
        .unwrap()
        .identity();
    fs::write(directory.path(), encode_checkpoint(&checkpoint(4)).unwrap()).unwrap();
    assert_eq!(
        stale
            .parent
            .open_child_file(&stale.target)
            .unwrap()
            .identity(),
        identity
    );
    assert!(matches!(
        stale.save(&checkpoint(5)),
        Err(SaveIoError::Conflict)
    ));
    assert_eq!(stale.load().unwrap().checkpoint.score, 4);
}

#[test]
fn a_committed_read_does_not_authorize_a_later_file_replacement() {
    let directory = Directory::new();
    let path = directory.path();
    let mut slot = SaveSlot::open(&path).unwrap();
    slot.save(&checkpoint(1)).unwrap();
    let mut other = SaveSlot::open(&path).unwrap();
    other.save(&checkpoint(2)).unwrap();
    let (loaded, binding) = slot.prepare_load().unwrap();
    assert_eq!(loaded.checkpoint.score, 2);
    assert!(matches!(
        slot.save(&checkpoint(3)),
        Err(SaveIoError::Conflict)
    ));

    let latest = other.save(&checkpoint(4)).unwrap();
    slot.accept_load(binding);
    assert!(matches!(
        slot.save(&checkpoint(5)),
        Err(SaveIoError::Conflict)
    ));
    let bytes = fs::read(&path).unwrap();
    assert_eq!(ContentDigest::of_bytes(&bytes), latest.digest);
    assert_eq!(decode_checkpoint(&bytes).unwrap().checkpoint.score, 4);
}

#[test]
fn post_publication_change_is_uncertain_until_a_validated_readback_reconciles() {
    let directory = Directory::new();
    let mut slot = SaveSlot::open(&directory.path()).unwrap();
    let path = directory.path();
    slot.after_replace = Some(Box::new(move || {
        fs::write(path, encode_checkpoint(&checkpoint(2)).unwrap()).unwrap()
    }));
    assert!(matches!(
        slot.save(&checkpoint(1)),
        Err(SaveIoError::Uncertain)
    ));
    assert!(matches!(
        slot.save(&checkpoint(3)),
        Err(SaveIoError::Uncertain)
    ));
    assert_eq!(
        decode_checkpoint(&fs::read(directory.path()).unwrap())
            .unwrap()
            .checkpoint
            .score,
        2
    );
    assert_eq!(slot.load().unwrap().checkpoint.score, 2);
    slot.save(&checkpoint(3)).unwrap();
    assert_eq!(slot.load().unwrap().checkpoint.score, 3);
}

#[test]
fn a_missing_slot_does_not_replace_a_target_created_by_another_owner() {
    let directory = Directory::new();
    let mut missing = SaveSlot::open(&directory.path()).unwrap();
    SaveSlot::open(&directory.path())
        .unwrap()
        .save(&checkpoint(1))
        .unwrap();
    assert!(matches!(
        missing.save(&checkpoint(2)),
        Err(SaveIoError::Conflict)
    ));
    #[cfg(windows)]
    assert!(missing.retry_cleanup());
    #[cfg(target_os = "linux")]
    {
        assert!(!missing.retry_cleanup());
        assert!(missing.cleanup_pending());
    }
    assert_eq!(
        decode_checkpoint(&fs::read(directory.path()).unwrap())
            .unwrap()
            .checkpoint
            .score,
        1
    );
}

#[test]
fn interrupted_candidate_keeps_cleanup_authority_and_preserves_old_target() {
    let directory = Directory::new();
    let mut slot = SaveSlot::open(&directory.path()).unwrap();
    slot.save(&checkpoint(1)).unwrap();
    let before = fs::read(directory.path()).unwrap();
    slot.interrupt_after_write = true;
    assert!(matches!(
        slot.save(&checkpoint(2)),
        Err(SaveIoError::Interrupted)
    ));
    assert!(slot.cleanup_pending());
    assert_eq!(fs::read(directory.path()).unwrap(), before);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    #[cfg(windows)]
    {
        assert!(slot.retry_cleanup());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
        slot.save(&checkpoint(3)).unwrap();
        assert_eq!(slot.load().unwrap().checkpoint.score, 3);
    }
    #[cfg(target_os = "linux")]
    {
        // The current Linux adapter refuses exact opened-object unlink. Preserve that authority
        // and report pending; never replace it with an unchecked pathname removal fallback.
        assert!(!slot.retry_cleanup());
        assert!(slot.cleanup_pending());
        assert!(matches!(
            slot.save(&checkpoint(3)),
            Err(SaveIoError::CleanupPending)
        ));
        assert_eq!(fs::read(directory.path()).unwrap(), before);
    }
}

#[test]
fn malformed_external_checkpoint_does_not_replace_the_observed_baseline() {
    let directory = Directory::new();
    let mut slot = SaveSlot::open(&directory.path()).unwrap();
    slot.save(&checkpoint(1)).unwrap();
    fs::write(directory.path(), b"{ broken").unwrap();
    assert!(matches!(slot.load(), Err(SaveIoError::Data(_))));
    assert!(matches!(
        slot.save(&checkpoint(2)),
        Err(SaveIoError::Conflict)
    ));
    assert_eq!(fs::read(directory.path()).unwrap(), b"{ broken");
}

#[test]
fn legacy_file_is_really_read_migrated_and_republished_as_the_current_format() {
    let directory = Directory::new();
    fs::write(directory.path(), br#"{"kind":"two_level_checkpoint","format_version":1,"campaign_revision":1,"payload":{"location":{"level":"cavern","anchor":"checkpoint"},"score":10,"collected":[{"level":"meadow","entity":"gem-near"}],"markers":[{"id":1,"level":"meadow","position":[-80,0]}],"next_marker_id":2}}"#).unwrap();
    let mut slot = SaveSlot::open(&directory.path()).unwrap();
    let decoded = slot.load().unwrap();
    assert_eq!(decoded.migrated_from, Some(1));
    assert_eq!(
        decoded.checkpoint.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    slot.save(&decoded.checkpoint).unwrap();
    let bytes = fs::read(directory.path()).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["format_version"],
        crate::save::CURRENT_FORMAT_VERSION
    );
    assert_eq!(slot.load().unwrap().migrated_from, None);
}

#[test]
fn concurrent_cold_owners_cannot_both_acquire_the_same_server_slot() {
    let directory = Directory::new();
    let path = directory.path();
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            start.wait();
            SaveSlot::open_server(&path)
        });
        start.wait();
        let second = SaveSlot::open_server(&path);
        let first = first.join().unwrap();
        assert!(matches!(
            (first, second),
            (Ok(_), Err(SaveIoError::File(FsError::LockContended)))
                | (Err(SaveIoError::File(FsError::LockContended)), Ok(_))
        ));
    });
    assert!(fs::read(directory.marker_path()).unwrap().is_empty());
    assert!(SaveSlot::open_server(&path).is_ok());
}

#[test]
fn staged_handoff_requires_release_and_transfers_normal_save_access() {
    let directory = Directory::new();
    let path = directory.path();
    let mut previous = SaveSlot::open_server(&path).unwrap();
    previous.save(&checkpoint(1)).unwrap();
    let original = fs::read(&path).unwrap();
    let stamp = previous.handoff_stamp().unwrap();
    let mut next = SaveSlot::prepare_server_handoff(&path, stamp).unwrap();
    assert_eq!(next.handoff_stamp().unwrap(), stamp);
    assert!(matches!(
        next.save(&checkpoint(2)),
        Err(SaveIoError::HandoffUnavailable)
    ));
    assert!(matches!(
        next.prepare_load(),
        Err(SaveIoError::HandoffUnavailable)
    ));
    assert!(matches!(
        next.claim_server_handoff(),
        Err(SaveIoError::File(FsError::LockContended))
    ));
    assert!(matches!(
        previous.claim_server_handoff(),
        Err(SaveIoError::HandoffUnavailable)
    ));

    assert!(previous.retry_close());
    assert!(matches!(
        previous.handoff_stamp(),
        Err(SaveIoError::HandoffUnavailable)
    ));
    next.claim_server_handoff().unwrap();
    assert_eq!(next.handoff_stamp().unwrap(), stamp);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::metadata(directory.marker_path()).unwrap().len(), 0);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    assert_eq!(next.load().unwrap().checkpoint.score, 1);
    next.save(&checkpoint(2)).unwrap();
    assert_ne!(next.handoff_stamp().unwrap(), stamp);
    assert!(next.retry_close());
    assert!(fs::read(directory.marker_path()).unwrap().is_empty());
    assert_eq!(
        SaveSlot::open(&path)
            .unwrap()
            .load()
            .unwrap()
            .checkpoint
            .score,
        2
    );
}

#[test]
fn missing_checkpoint_handoff_requires_the_original_marker() {
    let directory = Directory::new();
    let path = directory.path();
    let mut plain = SaveSlot::open(&path).unwrap();
    assert!(matches!(
        plain.handoff_stamp(),
        Err(SaveIoError::HandoffUnavailable)
    ));
    assert!(matches!(
        plain.claim_server_handoff(),
        Err(SaveIoError::HandoffUnavailable)
    ));
    assert!(matches!(
        SaveSlot::prepare_server_handoff(&path, [0; 32]),
        Err(SaveIoError::Conflict)
    ));
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);

    let mut previous = SaveSlot::open_server(&path).unwrap();
    let stamp = previous.handoff_stamp().unwrap();
    let other_path = directory.0.join("other.json");
    let _other = SaveSlot::open_server(&other_path).unwrap();
    assert!(matches!(
        SaveSlot::prepare_server_handoff(&other_path, stamp),
        Err(SaveIoError::Conflict)
    ));
    let mut next = SaveSlot::prepare_server_handoff(&path, stamp).unwrap();
    assert!(previous.retry_close());
    fs::write(&path, encode_checkpoint(&checkpoint(1)).unwrap()).unwrap();
    assert!(matches!(
        next.claim_server_handoff(),
        Err(SaveIoError::Conflict)
    ));
    fs::remove_file(&path).unwrap();

    next.claim_server_handoff().unwrap();
    assert!(!path.exists());
    assert_eq!(next.handoff_stamp().unwrap(), stamp);
    next.save(&checkpoint(1)).unwrap();
    assert_eq!(next.load().unwrap().checkpoint.score, 1);
}

#[test]
fn claim_rechecks_retained_checkpoint_identity_and_bytes_and_can_retry() {
    let directory = Directory::new();
    let path = directory.path();
    let mut previous = SaveSlot::open_server(&path).unwrap();
    previous.save(&checkpoint(1)).unwrap();
    let bytes = fs::read(&path).unwrap();
    let stamp = previous.handoff_stamp().unwrap();
    let mut next = SaveSlot::prepare_server_handoff(&path, stamp).unwrap();

    fs::write(&path, encode_checkpoint(&checkpoint(2)).unwrap()).unwrap();
    assert!(matches!(
        previous.handoff_stamp(),
        Err(SaveIoError::Conflict)
    ));
    assert!(previous.retry_close());
    assert!(matches!(
        next.claim_server_handoff(),
        Err(SaveIoError::Conflict)
    ));
    fs::write(&path, &bytes).unwrap();
    assert_eq!(next.handoff_stamp().unwrap(), stamp);

    let displaced = directory.0.join("displaced.json");
    fs::rename(&path, &displaced).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        SaveSlot::prepare_server_handoff(&path, stamp),
        Err(SaveIoError::Conflict)
    ));
    assert!(matches!(next.handoff_stamp(), Err(SaveIoError::Conflict)));
    assert!(matches!(
        next.claim_server_handoff(),
        Err(SaveIoError::Conflict)
    ));
    let mut independent = SaveSlot::open_server(&path).unwrap();
    assert!(independent.retry_close());
    drop(independent);

    fs::remove_file(&path).unwrap();
    fs::rename(&displaced, &path).unwrap();
    assert_eq!(next.handoff_stamp().unwrap(), stamp);
    next.claim_server_handoff().unwrap();
}

#[test]
fn a_replaced_marker_blocks_active_io_and_a_prepared_claim() {
    let directory = Directory::new();
    let path = directory.path();
    let marker = directory.marker_path();
    let displaced = directory.0.join("displaced.lock");
    let mut previous = SaveSlot::open_server(&path).unwrap();
    let stamp = previous.handoff_stamp().unwrap();
    let mut next = SaveSlot::prepare_server_handoff(&path, stamp).unwrap();

    fs::rename(&marker, &displaced).unwrap();
    fs::write(&marker, b"").unwrap();
    assert!(matches!(
        previous.save(&checkpoint(1)),
        Err(SaveIoError::Conflict)
    ));
    assert!(matches!(
        previous.prepare_load(),
        Err(SaveIoError::Conflict)
    ));
    assert!(matches!(
        SaveSlot::prepare_server_handoff(&path, stamp),
        Err(SaveIoError::Conflict)
    ));
    assert!(matches!(
        next.claim_server_handoff(),
        Err(SaveIoError::Conflict)
    ));
    assert!(previous.retry_close());
    assert!(!path.exists());

    fs::remove_file(&marker).unwrap();
    fs::rename(&displaced, &marker).unwrap();
    assert_eq!(next.handoff_stamp().unwrap(), stamp);
    next.claim_server_handoff().unwrap();
}

#[test]
fn coordinated_slots_require_exact_spelling_and_a_single_link() {
    let directory = Directory::new();
    let path = directory.path();
    let marker = directory.marker_path();
    let upper_leaf = directory.0.join("CHECKPOINT.JSON");
    let reserved_alias = directory.0.join(".TWO-LEVEL-LOCK-checkpoint.json");
    assert!(matches!(
        SaveSlot::open_server(&marker),
        Err(SaveIoError::InvalidPath)
    ));
    assert!(matches!(
        SaveSlot::open_server(&reserved_alias),
        Err(SaveIoError::InvalidPath)
    ));

    let mut slot = SaveSlot::open_server(&path).unwrap();
    slot.save(&checkpoint(1)).unwrap();
    let stamp = slot.handoff_stamp().unwrap();
    if upper_leaf.exists() {
        assert!(matches!(
            SaveSlot::open_server(&upper_leaf),
            Err(SaveIoError::Conflict)
        ));
        assert!(matches!(
            SaveSlot::prepare_server_handoff(&upper_leaf, stamp),
            Err(SaveIoError::Conflict)
        ));
    } else {
        let independent = SaveSlot::open_server(&upper_leaf).unwrap();
        assert_ne!(independent.handoff_stamp().unwrap(), stamp);
    }

    let alias = directory.0.join("additional-link.json");
    fs::hard_link(&path, &alias).unwrap();
    assert!(matches!(slot.handoff_stamp(), Err(SaveIoError::Conflict)));
    fs::remove_file(&alias).unwrap();
    fs::hard_link(&marker, &alias).unwrap();
    assert!(matches!(slot.handoff_stamp(), Err(SaveIoError::Conflict)));
}

#[test]
fn pending_cleanup_blocks_handoff_and_close_retains_custody() {
    let directory = Directory::new();
    let path = directory.path();
    let mut slot = SaveSlot::open_server(&path).unwrap();
    slot.interrupt_after_write = true;
    assert!(matches!(
        slot.save(&checkpoint(1)),
        Err(SaveIoError::Interrupted)
    ));
    assert!(matches!(
        slot.handoff_stamp(),
        Err(SaveIoError::CleanupPending)
    ));
    #[cfg(windows)]
    {
        assert!(slot.retry_close());
        assert!(SaveSlot::open_server(&path).is_ok());
    }
    #[cfg(target_os = "linux")]
    {
        assert!(!slot.retry_close());
        assert!(slot.cleanup_pending());
        assert!(matches!(
            SaveSlot::open_server(&path),
            Err(SaveIoError::File(FsError::LockContended))
        ));
    }
}

#[test]
fn uncertain_publication_blocks_handoff_until_committed_load_rebinds_the_slot() {
    let directory = Directory::new();
    let path = directory.path();
    let mut slot = SaveSlot::open_server(&path).unwrap();
    let before = slot.handoff_stamp().unwrap();
    slot.after_replace = Some(Box::new(move || {
        fs::write(path, encode_checkpoint(&checkpoint(2)).unwrap()).unwrap();
    }));
    assert!(matches!(
        slot.save(&checkpoint(1)),
        Err(SaveIoError::Uncertain)
    ));
    assert!(matches!(slot.handoff_stamp(), Err(SaveIoError::Uncertain)));
    let (decoded, binding) = slot.prepare_load().unwrap();
    assert_eq!(decoded.checkpoint.score, 2);
    assert!(matches!(slot.handoff_stamp(), Err(SaveIoError::Uncertain)));
    slot.accept_load(binding);
    assert_ne!(slot.handoff_stamp().unwrap(), before);
}
