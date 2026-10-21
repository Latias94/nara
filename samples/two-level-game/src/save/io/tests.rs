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
