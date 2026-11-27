#![cfg(all(feature = "cook", any(unix, windows)))]

use std::{
    convert::Infallible,
    fs, io,
    sync::{Arc, Barrier},
    thread,
};

use nara_asset::cook::{ImportCache, ImportCacheError, ImportCacheKey};

#[test]
fn cache_frames_each_input_and_does_not_reinvoke_a_hit() {
    let root = tempfile::tempdir().unwrap();
    let cache = ImportCache::new(root.path()).unwrap();
    let key = ImportCacheKey::new(b"abc", b"def", "image.v1", "pc");
    for other in [
        ImportCacheKey::new(b"ab", b"cdef", "image.v1", "pc"),
        ImportCacheKey::new(b"xyz", b"def", "image.v1", "pc"),
        ImportCacheKey::new(b"abc", b"xyz", "image.v1", "pc"),
        ImportCacheKey::new(b"abc", b"def", "image.v2", "pc"),
        ImportCacheKey::new(b"abc", b"def", "image.v1", "mobile"),
    ] {
        assert_ne!(key, other);
    }
    assert_eq!(cache.read(key).unwrap(), None);
    assert!(matches!(
        cache.get_or_insert_with(key, || Err("bad image")),
        Err(ImportCacheError::Produce("bad image"))
    ));
    assert_eq!(cache.read(key).unwrap(), None);
    assert_eq!(
        cache
            .get_or_insert_with(key, || Ok::<_, Infallible>(b"output".to_vec()))
            .unwrap(),
        b"output"
    );
    let reopened = ImportCache::new(root.path()).unwrap();
    let result = reopened
        .get_or_insert_with::<Infallible>(key, || panic!("a hit must not call the producer"))
        .unwrap();
    assert_eq!(result, b"output");
}

#[test]
fn competing_producers_only_return_the_complete_published_winner() {
    let root = tempfile::tempdir().unwrap();
    let cache = ImportCache::new(root.path()).unwrap();
    let key = ImportCacheKey::new(b"image", b"sidecar", "image.v1", "pc");
    let barrier = Arc::new(Barrier::new(2));
    let outputs: Vec<_> = [123u8, 249]
        .map(|byte| {
            let cache = cache.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                cache
                    .get_or_insert_with(key, || {
                        barrier.wait();
                        Ok::<_, Infallible>(vec![byte; 65_536])
                    })
                    .unwrap()
            })
        })
        .into_iter()
        .map(|task| task.join().unwrap())
        .collect();
    let winner = cache.read(key).unwrap().unwrap();
    assert_eq!(outputs, [winner.clone(), winner]);
    assert_eq!(
        fs::read_dir(root.path().join(".nara/cache"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn cache_rejects_non_directory_scope_and_checks_cached_payload_integrity() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join(".nara");
    fs::write(&state, b"not a directory").unwrap();
    assert_eq!(
        ImportCache::new(root.path()).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    fs::remove_file(&state).unwrap();
    let cache = ImportCache::new(root.path()).unwrap();
    let key = ImportCacheKey::new(b"src", b"meta", "v1", "target");
    cache
        .get_or_insert_with(key, || Ok::<_, Infallible>(b"artifact".to_vec()))
        .unwrap();
    let file = fs::read_dir(state.join("cache"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let mut bytes = fs::read(file.path()).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(file.path(), &bytes).unwrap();
    assert_eq!(
        cache.read(key).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}
