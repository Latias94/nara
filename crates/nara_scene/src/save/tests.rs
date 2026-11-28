use std::fs;

use bevy_ecs::component::Component;
use nara_reflect::{Bytes, Schema};

use super::{SaveError, SaveSchema, SaveStore, wire};

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(crate = "nara_reflect", id = "game.Progress", version = 1)]
struct Progress {
    checkpoint: u16,
}

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(crate = "nara_reflect", id = "game.Progress", version = 2)]
struct ProgressV2 {
    checkpoint: u16,
    finished: bool,
}

#[derive(Component, Schema, Debug)]
#[schema(crate = "nara_reflect", id = "game.OtherSnapshot", version = 1)]
struct OtherSnapshot {
    checkpoint: u16,
}

#[derive(Component, Schema)]
#[schema(crate = "nara_reflect", id = "game.Blob", version = 1)]
struct Blob {
    data: Bytes,
}

#[test]
fn writes_the_nsav_v1_header_and_round_trips() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("portable").join("saves");
    let store = SaveStore::in_dir(&directory);
    let slot = "Best-01_A";
    let value = Progress { checkpoint: 263 };
    assert!(matches!(
        store.load::<Progress>(slot),
        Err(SaveError::NotFound(_))
    ));
    assert!(!directory.exists());

    store.save(slot, &value).unwrap();
    assert_eq!(store.load::<Progress>(slot).unwrap(), value);
    let loaded = store.load_with_backup::<Progress>(slot).unwrap();
    assert_eq!(loaded.value, value);
    assert!(!loaded.recovered);
    assert!(!directory.join("Best-01_A.nsav.bak").exists());

    let bytes = fs::read(directory.join("Best-01_A.nsav")).unwrap();
    let payload = value.encode_postcard().unwrap();
    let hash = Progress::type_schema().unwrap().postcard_schema_hash();
    assert_eq!(&bytes[..4], b"NSAV");
    assert_eq!(&bytes[4..8], &[1, 0, 1, 0]);
    assert_eq!(&bytes[8..12], &[0; 4]);
    assert_eq!(&bytes[12..16], &(payload.len() as u32).to_le_bytes());
    assert_eq!(&bytes[16..32], &hash.as_bytes()[..16]);
    assert_eq!(&bytes[64..], payload.as_slice());
    let digest = blake3::Hasher::new()
        .update(&bytes[..32])
        .update(&payload)
        .finalize();
    assert_eq!(&bytes[32..64], digest.as_bytes());
}

#[test]
fn replacement_keeps_the_previous_save_and_recovery_leaves_it_intact() {
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let primary = temp.path().join("slot.nsav");
    let backup = temp.path().join("slot.nsav.bak");
    let first = Progress { checkpoint: 1 };
    let second = Progress { checkpoint: 2 };
    let third = Progress { checkpoint: 3 };

    store.save("slot", &first).unwrap();
    let first_bytes = fs::read(&primary).unwrap();
    store.save("slot", &second).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), first_bytes);
    assert_eq!(store.load::<Progress>("slot").unwrap(), second);

    let mut damaged = fs::read(&primary).unwrap();
    damaged[64] ^= 1;
    fs::write(&primary, &damaged).unwrap();
    assert!(matches!(
        store.load::<Progress>("slot"),
        Err(SaveError::Corrupt { .. })
    ));
    let loaded = store.load_with_backup::<Progress>("slot").unwrap();
    assert!(loaded.recovered);
    assert_eq!(loaded.value, first);
    assert_eq!(fs::read(&primary).unwrap(), damaged);

    store.save("slot", &third).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), first_bytes);
    assert_eq!(store.load::<Progress>("slot").unwrap(), third);

    fs::remove_file(primary).unwrap();
    assert!(matches!(
        store.load::<Progress>("slot"),
        Err(SaveError::NotFound(_))
    ));
    let loaded = store.load_with_backup::<Progress>("slot").unwrap();
    assert!(loaded.recovered);
    assert_eq!(loaded.value, first);
}

#[test]
fn an_undecodable_snapshot_does_not_replace_the_backup() {
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let first = Progress { checkpoint: 1 };
    store.save("slot", &first).unwrap();
    store.save("slot", &Progress { checkpoint: 2 }).unwrap();

    let malformed = wire::encode(SaveSchema::of::<Progress>().unwrap(), &[]).unwrap();
    fs::write(temp.path().join("slot.nsav"), malformed).unwrap();
    assert!(matches!(
        store.load::<Progress>("slot"),
        Err(SaveError::Decode { .. })
    ));
    let loaded = store.load_with_backup::<Progress>("slot").unwrap();
    assert!(loaded.recovered);
    assert_eq!(loaded.value, first);

    store.save("slot", &Progress { checkpoint: 3 }).unwrap();
    let backup = fs::read(temp.path().join("slot.nsav.bak")).unwrap();
    assert_eq!(&backup[64..], first.encode_postcard().unwrap().as_slice());
}

#[test]
fn migration_receives_the_intact_old_payload() {
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let primary = temp.path().join("slot.nsav");
    let old = Progress { checkpoint: 3 };
    store.save("slot", &old).unwrap();
    let old_bytes = fs::read(&primary).unwrap();
    assert!(matches!(
        store.load::<ProgressV2>("slot"),
        Err(SaveError::SchemaVersionMismatch {
            expected: 2,
            found: 1
        })
    ));
    assert!(matches!(
        store.load_with_migration::<OtherSnapshot>("slot", |_, _| panic!("same version")),
        Err(SaveError::SchemaHashMismatch)
    ));

    let migrated = store
        .load_with_migration("slot", |version, payload| {
            assert_eq!(version, 1);
            assert_eq!(payload, old.encode_postcard()?.as_slice());
            let old = Progress::decode_postcard(payload)?;
            Ok(ProgressV2 {
                checkpoint: old.checkpoint,
                finished: false,
            })
        })
        .unwrap();
    assert_eq!(
        migrated,
        ProgressV2 {
            checkpoint: 3,
            finished: false
        }
    );
    assert_eq!(fs::read(&primary).unwrap(), old_bytes);

    store.save("slot", &migrated).unwrap();
    assert_eq!(
        fs::read(temp.path().join("slot.nsav.bak")).unwrap(),
        old_bytes
    );
    assert!(matches!(
        store.load_with_backup::<Progress>("slot"),
        Err(SaveError::SchemaVersionMismatch {
            expected: 1,
            found: 2
        })
    ));
    assert_eq!(
        store
            .load_with_migration::<ProgressV2>("slot", |_, _| panic!("current version"))
            .unwrap(),
        migrated
    );
}

#[test]
fn validates_header_integrity_before_migration() {
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let primary = temp.path().join("slot.nsav");
    store.save("slot", &Progress { checkpoint: 1 }).unwrap();
    let mut bytes = fs::read(&primary).unwrap();
    bytes[6] = 3;
    fs::write(&primary, &bytes).unwrap();
    assert!(matches!(
        store.load_with_migration::<ProgressV2>("slot", |_, _| panic!("invalid digest")),
        Err(SaveError::Corrupt { .. })
    ));

    bytes[4] = 2;
    let digest = blake3::Hasher::new()
        .update(&bytes[..32])
        .update(&bytes[64..])
        .finalize();
    bytes[32..64].copy_from_slice(digest.as_bytes());
    fs::write(&primary, &bytes).unwrap();
    assert!(matches!(
        store.load::<Progress>("slot"),
        Err(SaveError::UnsupportedFormatVersion(2))
    ));
}

#[test]
fn enforces_payload_size_on_write_and_read() {
    const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let value = Blob {
        data: vec![0; MAX_PAYLOAD].into(),
    };
    assert!(
        matches!(store.save("slot", &value), Err(SaveError::PayloadTooLarge(size)) if size > MAX_PAYLOAD)
    );
    let primary = temp.path().join("slot.nsav");
    assert!(!primary.exists());

    fs::File::create(primary)
        .unwrap()
        .set_len((64 + MAX_PAYLOAD + 1) as u64)
        .unwrap();
    assert!(matches!(
        store.load::<Blob>("slot"),
        Err(SaveError::Corrupt { .. })
    ));
}

#[test]
fn rejects_nonportable_slot_and_app_ids_before_accessing_paths() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("saves");
    let store = SaveStore::in_dir(&directory);
    for id in [
        "", ".", "..", "../slot", r"..\slot", "C:slot", "slot ", "slot.", "槽", "AuX", "cOm9",
        "LPT1", "NUL.txt",
    ] {
        assert!(matches!(
            store.save(id, &Progress { checkpoint: 1 }),
            Err(SaveError::InvalidSlot(_))
        ));
        assert!(matches!(
            store.load::<Progress>(id),
            Err(SaveError::InvalidSlot(_))
        ));
        assert!(matches!(
            SaveStore::for_app(id),
            Err(SaveError::InvalidAppId(_))
        ));
    }
    assert!(!directory.exists());
}

#[test]
fn rejects_a_nonfile_entry_for_a_slot() {
    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let primary = temp.path().join("slot.nsav");
    fs::create_dir(&primary).unwrap();
    assert!(
        matches!(store.load::<Progress>("slot"), Err(SaveError::UnsafePath(path)) if path == primary)
    );
    assert!(
        matches!(store.save("slot", &Progress { checkpoint: 1 }), Err(SaveError::UnsafePath(path)) if path == primary)
    );
}

#[test]
fn rejects_links_at_slot_backup_and_managed_directories() {
    #[cfg(unix)]
    use std::os::unix::fs::{symlink as link_dir, symlink as link_file};
    #[cfg(windows)]
    use std::os::windows::fs::{symlink_dir as link_dir, symlink_file as link_file};

    let temp = tempfile::tempdir().unwrap();
    let store = SaveStore::in_dir(temp.path());
    let value = Progress { checkpoint: 1 };
    store.save("target", &value).unwrap();
    let target = temp.path().join("target.nsav");
    let primary = temp.path().join("slot.nsav");
    if let Err(error) = link_file(&target, &primary) {
        #[cfg(windows)]
        if error.raw_os_error() == Some(1314) {
            // Creating symlinks requires developer mode or the symlink privilege on Windows.
            return;
        }
        panic!("could not create test symlink: {error}");
    }
    assert!(
        matches!(store.load::<Progress>("slot"), Err(SaveError::UnsafePath(path)) if path == primary)
    );
    assert!(
        matches!(store.save("slot", &value), Err(SaveError::UnsafePath(path)) if path == primary)
    );
    assert_eq!(store.load::<Progress>("target").unwrap(), value);

    fs::remove_file(&primary).unwrap();
    let backup = temp.path().join("slot.nsav.bak");
    link_file(temp.path().join("missing.nsav"), &backup).unwrap();
    assert!(
        matches!(store.load_with_backup::<Progress>("slot"), Err(SaveError::UnsafePath(path)) if path == backup)
    );
    assert!(
        matches!(store.save("slot", &value), Err(SaveError::UnsafePath(path)) if path == backup)
    );

    let real_dir = temp.path().join("real-dir");
    fs::create_dir(&real_dir).unwrap();
    let link = temp.path().join("linked-dir");
    link_dir(&real_dir, &link).unwrap();
    let direct_store = SaveStore::in_dir(link.join(""));
    assert!(
        matches!(direct_store.save("slot", &value), Err(SaveError::UnsafePath(path)) if path == link)
    );

    let mut app_store = SaveStore::in_dir(link.join("saves"));
    app_store.app_directory = Some(link.clone());
    assert!(
        matches!(app_store.save("slot", &value), Err(SaveError::UnsafePath(path)) if path == link)
    );
    assert!(!real_dir.join("saves").exists());
}
