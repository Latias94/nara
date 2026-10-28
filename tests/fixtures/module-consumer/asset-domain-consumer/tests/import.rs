use std::sync::Arc;

use nara_asset::{
    AssetImportError, AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate,
    AssetPath, AssetSourceKind, SourceAssetImporter, StableAssetId,
};
use nara_asset_domain_consumer::{TextDomain, TextSettings};
use nara_core::ByteLimit;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq)]
enum ReadFailure {
    Missing { sentinel: u32 },
    TooLarge { actual: usize, maximum: usize },
}

fn metadata(settings: impl Serialize) -> AssetMetaIdentityCandidate {
    let identity = AssetMetaIdentity::new(
        StableAssetId::parse_str("c4d051e8-73ee-4b16-9d13-8c5fcb8a6a32").unwrap(),
        AssetPath::new("caption.txt").unwrap(),
        AssetSourceKind::Other(TextDomain::SOURCE_KIND.to_owned()),
    );
    let encoded = AssetMeta::new(identity, settings).to_json_string().unwrap();
    AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap()
}

fn assert_domain_error(error: AssetImportError<ReadFailure>, code: &str) {
    match error {
        AssetImportError::Domain(diagnostic) => assert_eq!(diagnostic.code().as_str(), code),
        AssetImportError::Source(error) => panic!("expected a domain failure, got {error:?}"),
    }
}

#[test]
fn public_inputs_prepare_typed_text_with_shared_bounded_dependency_reads() {
    let metadata = metadata(TextSettings { max_characters: 8 });
    let suffix: Arc<[u8]> = Arc::from(" \u{4e16}\u{754c}".as_bytes());
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.clone(), limit.get()));
        Ok::<_, ReadFailure>(Arc::clone(&suffix))
    };
    let product = {
        let mut source =
            AssetImportSource::new(&metadata, Arc::from(&b"suffix.txt\nHello"[..]), &mut reader);
        TextDomain::prepare(&mut source).unwrap()
    };
    assert_eq!(product.text, "Hello \u{4e16}\u{754c}");
    assert_eq!(
        requests,
        [
            (AssetPath::new("suffix.txt").unwrap(), 1024),
            (AssetPath::new("suffix.txt").unwrap(), 1024),
        ]
    );
}

#[test]
fn source_failure_preserves_a_concrete_reader_sentinel() {
    let metadata = metadata(TextSettings { max_characters: 64 });
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.clone(), limit.get()));
        Err(ReadFailure::Missing { sentinel: 73 })
    };
    let error = {
        let mut source =
            AssetImportSource::new(&metadata, Arc::from(&b"suffix.txt\nHello"[..]), &mut reader);
        TextDomain::prepare(&mut source).unwrap_err()
    };
    match error {
        AssetImportError::Source(error) => {
            assert_eq!(error, ReadFailure::Missing { sentinel: 73 });
        }
        AssetImportError::Domain(diagnostic) => panic!("unexpected domain failure: {diagnostic:?}"),
    }
    assert_eq!(requests, [(AssetPath::new("suffix.txt").unwrap(), 1024)]);
}

#[test]
fn dependency_bound_failure_preserves_the_callers_exact_limit_details() {
    let metadata = metadata(TextSettings {
        max_characters: 4096,
    });
    let mut reader = |_: &AssetPath, limit: ByteLimit| {
        let actual = 1025;
        if actual > limit.get() {
            return Err(ReadFailure::TooLarge {
                actual,
                maximum: limit.get(),
            });
        }
        Ok(Arc::<[u8]>::from(vec![b'x'; actual]))
    };
    let mut source =
        AssetImportSource::new(&metadata, Arc::from(&b"suffix.txt\nHello"[..]), &mut reader);
    match TextDomain::prepare(&mut source).unwrap_err() {
        AssetImportError::Source(error) => assert_eq!(
            error,
            ReadFailure::TooLarge {
                actual: 1025,
                maximum: 1024,
            }
        ),
        AssetImportError::Domain(diagnostic) => panic!("unexpected domain failure: {diagnostic:?}"),
    }
}

#[test]
fn invalid_typed_settings_reject_before_reading_auxiliary_inputs() {
    for metadata in [
        metadata(TextSettings { max_characters: 0 }),
        metadata(TextSettings {
            max_characters: 4097,
        }),
        metadata("invalid settings shape"),
    ] {
        let mut reads = 0;
        let mut reader = |_: &AssetPath, _: ByteLimit| {
            reads += 1;
            Err(ReadFailure::Missing { sentinel: 91 })
        };
        let error = {
            let mut source = AssetImportSource::new(
                &metadata,
                Arc::from(&b"suffix.txt\nHello"[..]),
                &mut reader,
            );
            TextDomain::prepare(&mut source).unwrap_err()
        };
        assert_domain_error(error, "consumer.text.settings-invalid");
        assert_eq!(reads, 0);
    }
}

#[test]
fn invalid_primary_bytes_and_paths_are_domain_failures_without_auxiliary_reads() {
    let metadata = metadata(TextSettings { max_characters: 64 });
    for bytes in [
        &b"\xff"[..],
        &b"missing dependency line"[..],
        &b"../suffix.txt\nHello"[..],
    ] {
        let mut reads = 0;
        let mut reader = |_: &AssetPath, _: ByteLimit| {
            reads += 1;
            Err(ReadFailure::Missing { sentinel: 91 })
        };
        let error = {
            let mut source = AssetImportSource::new(&metadata, Arc::from(bytes), &mut reader);
            TextDomain::prepare(&mut source).unwrap_err()
        };
        assert_domain_error(error, "consumer.text.source-invalid");
        assert_eq!(reads, 0);
    }
}

#[test]
fn character_budget_rejection_is_distinct_from_a_source_read_failure() {
    let metadata = metadata(TextSettings { max_characters: 7 });
    let suffix: Arc<[u8]> = Arc::from(" \u{4e16}\u{754c}".as_bytes());
    let mut reader = |_: &AssetPath, _: ByteLimit| Ok::<_, ReadFailure>(Arc::clone(&suffix));
    let mut source =
        AssetImportSource::new(&metadata, Arc::from(&b"suffix.txt\nHello"[..]), &mut reader);
    assert_domain_error(
        TextDomain::prepare(&mut source).unwrap_err(),
        "consumer.text.too-long",
    );
}
