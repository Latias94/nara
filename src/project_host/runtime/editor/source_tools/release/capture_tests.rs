use std::fs;

use serde_json::json;

use super::{tests::Release, *};

const RECEIPT_NAME: &str = ".nara-installation.json";

fn payload_bytes(source: &Release) -> usize {
    source.manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .try_fold(0_usize, |total, member| {
            total.checked_add(usize::try_from(member["length"].as_u64().unwrap()).unwrap())
        })
        .unwrap()
}

fn write_receipt(source: &Release, manifest: ContentDigest, installation_id: u8) -> ContentDigest {
    let installation = [installation_id; 32];
    let mut bytes = serde_json::to_vec(&json!({
        "format_version": 1,
        "installation": installation,
        "manifest": {"length": manifest.length(), "hash": manifest.as_bytes()},
    }))
    .unwrap();
    // Charge the actual selected bytes, including whitespace omitted by canonical encoding.
    bytes.push(b'\n');
    fs::write(source.directory.join(RECEIPT_NAME), &bytes).unwrap();
    ContentDigest::of_bytes(&bytes)
}

#[test]
fn two_installed_captures_share_exact_receipt_manifest_and_payload_allowance() {
    let sources = [Release::new(), Release::new()];
    let manifest = sources[0].write_manifest();
    assert_eq!(sources[1].write_manifest(), manifest);
    let receipts = [
        write_receipt(&sources[0], manifest, 1),
        write_receipt(&sources[1], manifest, 2),
    ];
    let roots = sources
        .each_ref()
        .map(|source| source.root(TrustMode::Untrusted));
    let total_bytes = sources
        .iter()
        .zip(receipts)
        .try_fold(0_usize, |total, (source, receipt)| {
            total
                .checked_add(usize::try_from(receipt.length()).unwrap())?
                .checked_add(usize::try_from(manifest.length()).unwrap())?
                .checked_add(payload_bytes(source))
        })
        .unwrap();
    let mut budget = ReleaseCaptureBudget::new(total_bytes);
    let first = EditorSourceToolInstallationInspection::read_with_budget(
        &roots[0],
        receipts[0],
        &mut budget,
    )
    .unwrap();
    let second = EditorSourceToolInstallationInspection::read_with_budget(
        &roots[1],
        receipts[1],
        &mut budget,
    )
    .unwrap();
    assert_ne!(
        first.receipt().installation_id(),
        second.receipt().installation_id()
    );
    assert_eq!(first.release().manifest_digest(), manifest);
    assert_eq!(second.release().manifest_digest(), manifest);
    drop((first, second));
    assert!(matches!(
        EditorSourceToolInstallationInspection::read_with_budget(
            &roots[0],
            receipts[0],
            &mut budget
        ),
        Err(EditorSourceToolInstallationError::Inspection(
            EditorSourceToolReleaseError::CaptureBudget
        ))
    ));

    let mut one_byte_short = ReleaseCaptureBudget::new(total_bytes - 1);
    EditorSourceToolInstallationInspection::read_with_budget(
        &roots[0],
        receipts[0],
        &mut one_byte_short,
    )
    .unwrap();
    assert!(matches!(
        EditorSourceToolInstallationInspection::read_with_budget(
            &roots[1],
            receipts[1],
            &mut one_byte_short,
        ),
        Err(EditorSourceToolInstallationError::Inspection(
            EditorSourceToolReleaseError::CaptureBudget
        ))
    ));
}

#[test]
fn release_captures_keep_the_shared_charge_after_captured_bytes_are_dropped() {
    let source = Release::new();
    let manifest = source.write_manifest();
    let root = source.root(TrustMode::Untrusted);
    let capture_bytes = usize::try_from(manifest.length()).unwrap() + payload_bytes(&source);
    let mut budget = ReleaseCaptureBudget::new(capture_bytes.checked_mul(2).unwrap());
    for _ in 0..2 {
        let captured =
            PreparedEditorSourceToolRelease::capture_with_budget(&root, manifest, &mut budget)
                .unwrap();
        assert_eq!(captured.manifest_digest(), manifest);
    }
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture_with_budget(&root, manifest, &mut budget),
        Err(EditorSourceToolReleaseError::CaptureBudget)
    ));
}

#[test]
fn failed_payload_read_keeps_its_full_declared_charge() {
    let source = Release::new();
    let manifest = source.write_manifest();
    let receipt = write_receipt(&source, manifest, 1);
    let entry_path = source.directory.join("Tool.dll");
    let original_entry = fs::read(&entry_path).unwrap();
    let mut oversized_entry = original_entry.clone();
    oversized_entry.push(b'!');
    fs::write(&entry_path, oversized_entry).unwrap();
    let catalog_bytes = source.manifest["files"][0]["length"].as_u64().unwrap();
    let entry_bytes = source.manifest["files"][1]["length"].as_u64().unwrap();
    let attempted_bytes = receipt.length() + manifest.length() + catalog_bytes + entry_bytes;
    let mut budget = ReleaseCaptureBudget::new(usize::try_from(attempted_bytes).unwrap());
    let root = source.root(TrustMode::Untrusted);
    assert!(matches!(
        EditorSourceToolInstallationInspection::read_with_budget(&root, receipt, &mut budget),
        Err(EditorSourceToolInstallationError::Inspection(
            EditorSourceToolReleaseError::Filesystem {
                member,
                source: FsError::ByteLimitExceeded { limit },
            }
        )) if member == "Tool.dll" && limit == entry_bytes
    ));
    fs::write(entry_path, original_entry).unwrap();
    // If the failed payload read were refunded, this exact payload would fit again.
    assert!(matches!(
        capture_member(
            &root,
            "Tool.dll",
            ContentDigest::from_parts(entry_bytes, [0; 32]),
            &mut budget,
        ),
        Err(EditorSourceToolReleaseError::CaptureBudget)
    ));
    // A separate operation gets a fresh allowance and still performs normal admission.
    EditorSourceToolInstallationInspection::read(&root, receipt).unwrap();
}

#[test]
fn digest_rejection_does_not_refund_the_manifest_read() {
    let source = Release::new();
    let manifest = source.write_manifest();
    let root = source.root(TrustMode::Untrusted);
    let mut budget = ReleaseCaptureBudget::new(usize::try_from(manifest.length()).unwrap());
    let wrong_digest = ContentDigest::from_parts(manifest.length(), [0; 32]);
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture_with_budget(&root, wrong_digest, &mut budget),
        Err(EditorSourceToolReleaseError::Digest { member }) if member == MANIFEST_NAME
    ));
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture_with_budget(&root, manifest, &mut budget),
        Err(EditorSourceToolReleaseError::CaptureBudget)
    ));
}

#[test]
fn allowance_refuses_a_large_payload_before_opening_or_allocating_it() {
    let mut source = Release::new();
    source.manifest["files"][1]["length"] = json!(manifest::MAX_MEMBER_BYTES);
    let manifest = source.write_manifest();
    fs::remove_file(source.directory.join("Tool.dll")).unwrap();
    let catalog_bytes = source.manifest["files"][0]["length"].as_u64().unwrap();
    let maximum_bytes = manifest.length() + catalog_bytes + manifest::MAX_MEMBER_BYTES - 1;
    let mut budget = ReleaseCaptureBudget::new(usize::try_from(maximum_bytes).unwrap());
    assert!(matches!(
        PreparedEditorSourceToolRelease::capture_with_budget(
            &source.root(TrustMode::Untrusted),
            manifest,
            &mut budget,
        ),
        Err(EditorSourceToolReleaseError::CaptureBudget)
    ));
}

#[test]
fn shared_capture_preserves_strict_inventory_admission() {
    let source = Release::new();
    let manifest = source.write_manifest();
    let receipt = write_receipt(&source, manifest, 1);
    fs::write(source.directory.join("unlisted.txt"), b"not admitted").unwrap();
    let root = source.root(TrustMode::Untrusted);
    let mut budget = ReleaseCaptureBudget::new(128 * 1024 * 1024);
    assert!(matches!(
        EditorSourceToolInstallationInspection::read_with_budget(&root, receipt, &mut budget),
        Err(EditorSourceToolInstallationError::Changed { .. })
            | Err(EditorSourceToolInstallationError::Member { .. })
    ));
}

#[test]
fn public_defaults_allow_full_payload_manifest_and_receipt_limits() {
    let mut source = Release::new();
    let maximum_payload = usize::try_from(manifest::MAX_TOTAL_BYTES).unwrap();
    let maximum_member = usize::try_from(manifest::MAX_MEMBER_BYTES).unwrap();
    let mut remaining = maximum_payload - payload_bytes(&source);
    let bytes = vec![b'x'; maximum_member];
    for index in 0..4 {
        let length = remaining.min(maximum_member);
        let bytes = &bytes[..length];
        let path = format!("payload-{index}.bin");
        fs::write(source.directory.join(&path), bytes).unwrap();
        let digest = ContentDigest::of_bytes(bytes);
        let hash: String = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        source.manifest["files"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "path": path, "kind": "content", "length": digest.length(), "blake3": hash,
            }));
        remaining -= length;
    }
    assert_eq!(remaining, 0);
    drop(bytes);
    source.write_manifest();
    let manifest_path = source.directory.join(MANIFEST_NAME);
    let mut manifest_bytes = fs::read(&manifest_path).unwrap();
    manifest_bytes.resize(usize::try_from(MAX_MANIFEST_BYTES).unwrap(), b' ');
    fs::write(manifest_path, &manifest_bytes).unwrap();
    let manifest = ContentDigest::of_bytes(&manifest_bytes);
    write_receipt(&source, manifest, 1);
    let receipt_path = source.directory.join(RECEIPT_NAME);
    let mut receipt_bytes = fs::read(&receipt_path).unwrap();
    receipt_bytes.resize(4096, b' ');
    fs::write(receipt_path, &receipt_bytes).unwrap();
    let receipt = ContentDigest::of_bytes(&receipt_bytes);
    let root = source.root(TrustMode::Untrusted);
    let captured = PreparedEditorSourceToolRelease::capture(&root, manifest).unwrap();
    assert_eq!(captured.manifest_digest().length(), MAX_MANIFEST_BYTES);
    assert_eq!(
        captured
            .members()
            .iter()
            .map(|member| member.bytes().len())
            .sum::<usize>(),
        maximum_payload
    );
    drop(captured);
    let inspected = EditorSourceToolInstallationInspection::read(&root, receipt).unwrap();
    assert_eq!(inspected.receipt_digest().length(), 4096);
    assert_eq!(inspected.release().manifest_digest(), manifest);
    assert_eq!(
        inspected
            .release()
            .members()
            .iter()
            .map(|member| member.bytes().len())
            .sum::<usize>(),
        maximum_payload
    );
}
