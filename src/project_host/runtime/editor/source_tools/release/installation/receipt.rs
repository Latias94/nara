use nara_fs::ContentDigest;
use serde::{Deserialize, Serialize};

use super::super::manifest::MAX_MANIFEST_BYTES;

pub(super) const RECEIPT_NAME: &str = ".nara-installation.json";
pub(super) const MAX_RECEIPT_BYTES: u64 = 4096;

/// Bounded persisted installation facts for inspection, without execution or deletion authority.
///
/// Live file provenance remains with Host-owned filesystem handles. The manifest digest binds
/// package and Schema facts without duplicating them in this receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolInstallationReceipt {
    id: [u8; 32],
    manifest: ContentDigest,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptDto {
    format_version: u32,
    installation: [u8; 32],
    manifest: DigestDto,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestDto {
    length: u64,
    hash: [u8; 32],
}

impl EditorSourceToolInstallationReceipt {
    #[must_use]
    pub const fn installation_id(&self) -> &[u8; 32] {
        &self.id
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest
    }

    pub(super) fn issue(manifest: ContentDigest) -> Result<Self, &'static str> {
        validate_manifest(manifest)?;
        let mut id = [0; 32];
        getrandom::fill(&mut id).map_err(|_| "installation ID generation failed")?;
        if id == [0; 32] {
            return Err("installation ID must be nonzero");
        }
        Ok(Self { id, manifest })
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let bytes = serde_json::to_vec(&ReceiptDto {
            format_version: 1,
            installation: self.id,
            manifest: DigestDto {
                length: self.manifest.length(),
                hash: *self.manifest.as_bytes(),
            },
        })
        .map_err(|_| "installation receipt encoding failed")?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("installation receipt exceeds byte limit");
        }
        Ok(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("installation receipt exceeds byte limit");
        }
        let input: ReceiptDto =
            serde_json::from_slice(bytes).map_err(|_| "invalid installation receipt JSON")?;
        if input.format_version != 1 {
            return Err("unsupported installation receipt version");
        }
        let receipt = Self {
            id: input.installation,
            manifest: ContentDigest::from_parts(input.manifest.length, input.manifest.hash),
        };
        receipt.validate()?;
        Ok(receipt)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.id == [0; 32] {
            return Err("installation ID must be nonzero");
        }
        validate_manifest(self.manifest)
    }
}

fn validate_manifest(manifest: ContentDigest) -> Result<(), &'static str> {
    if manifest.length() == 0 || manifest.length() > MAX_MANIFEST_BYTES {
        return Err("invalid installation manifest length");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> EditorSourceToolInstallationReceipt {
        EditorSourceToolInstallationReceipt {
            id: [1; 32],
            manifest: ContentDigest::from_parts(42, [2; 32]),
        }
    }

    fn fixture_json() -> String {
        String::from_utf8(fixture().encode().unwrap()).unwrap()
    }

    #[test]
    fn issued_receipts_roundtrip_with_canonical_fields() {
        for (length, hash) in [(1, [0; 32]), (42, [2; 32]), (MAX_MANIFEST_BYTES, [255; 32])] {
            let manifest = ContentDigest::from_parts(length, hash);
            let receipt = EditorSourceToolInstallationReceipt::issue(manifest).unwrap();
            assert_ne!(receipt.installation_id(), &[0; 32]);
            assert_eq!(receipt.manifest_digest(), manifest);
            let bytes = receipt.encode().unwrap();
            let expected = format!(
                "{{\"format_version\":1,\"installation\":{},\"manifest\":{{\"length\":{length},\"hash\":{}}}}}",
                serde_json::to_string(receipt.installation_id()).unwrap(),
                serde_json::to_string(manifest.as_bytes()).unwrap(),
            );
            assert_eq!(bytes, expected.as_bytes());
            let decoded = EditorSourceToolInstallationReceipt::decode(&bytes).unwrap();
            assert_eq!(decoded, receipt);
            assert_eq!(decoded.encode().unwrap(), bytes);
        }
    }

    #[test]
    fn rejects_invalid_json_versions_and_fields() {
        let valid = fixture_json();
        let cases = [
            String::new(),
            "null".to_owned(),
            "{}".to_owned(),
            valid[..valid.len() - 1].to_owned(),
            format!("{valid} {{}}"),
            valid.replace("\"format_version\":1", "\"format_version\":0"),
            valid.replace("\"format_version\":1", "\"format_version\":2"),
            valid.replace("\"format_version\":1", "\"format_version\":-1"),
            valid.replace("\"format_version\":1", "\"format_version\":1.0"),
            valid.replace("\"format_version\":1", "\"format_version\":\"1\""),
            valid.replace("\"format_version\":1,", ""),
            valid.replace("\"length\":42,", ""),
            valid.replace("\"length\":42", "\"length\":-1"),
            valid.replace("\"length\":42", "\"length\":\"42\""),
            valid.replacen('{', "{\"package\":\"untrusted\",", 1),
            valid.replace("\"length\":42", "\"length\":42,\"schema\":1"),
        ];
        for input in cases {
            assert!(
                EditorSourceToolInstallationReceipt::decode(input.as_bytes()).is_err(),
                "accepted {input}",
            );
        }
    }

    #[test]
    fn rejects_duplicate_fields_at_each_object_level() {
        let valid = fixture_json();
        let installation = serde_json::to_string(&[1; 32]).unwrap();
        let hash = serde_json::to_string(&[2; 32]).unwrap();
        let manifest = format!("{{\"length\":42,\"hash\":{hash}}}");
        for (field, value) in [
            ("format_version", "1"),
            ("installation", installation.as_str()),
            ("manifest", manifest.as_str()),
            ("length", "42"),
            ("hash", hash.as_str()),
        ] {
            let original = format!("\"{field}\":{value}");
            let input = valid.replace(&original, &format!("{original},{original}"));
            assert!(
                EditorSourceToolInstallationReceipt::decode(input.as_bytes()).is_err(),
                "accepted duplicate {field}",
            );
        }
    }

    #[test]
    fn rejects_invalid_id_and_hash_arrays() {
        let valid = fixture_json();
        let cases = [
            "[]".to_owned(),
            serde_json::to_string(&vec![1_u8; 31]).unwrap(),
            serde_json::to_string(&vec![1_u8; 33]).unwrap(),
            format!("[-1,{}]", vec!["1"; 31].join(",")),
            format!("[256,{}]", vec!["1"; 31].join(",")),
            format!("[1.5,{}]", vec!["1"; 31].join(",")),
            "\"010203\"".to_owned(),
            "null".to_owned(),
        ];
        for (field, array) in [("installation", [1; 32]), ("hash", [2; 32])] {
            let original = format!("\"{field}\":{}", serde_json::to_string(&array).unwrap());
            for value in &cases {
                let input = valid.replace(&original, &format!("\"{field}\":{value}"));
                assert!(
                    EditorSourceToolInstallationReceipt::decode(input.as_bytes()).is_err(),
                    "accepted {field}: {value}",
                );
            }
        }
    }

    #[test]
    fn enforces_manifest_length_and_nonzero_id_on_encode_and_decode() {
        for (length, valid) in [
            (0, false),
            (1, true),
            (MAX_MANIFEST_BYTES, true),
            (MAX_MANIFEST_BYTES + 1, false),
            (u64::MAX, false),
        ] {
            let mut receipt = fixture();
            receipt.manifest = ContentDigest::from_parts(length, [0; 32]);
            assert_eq!(receipt.encode().is_ok(), valid, "length {length}");
            let input = fixture_json().replace("\"length\":42", &format!("\"length\":{length}"));
            assert_eq!(
                EditorSourceToolInstallationReceipt::decode(input.as_bytes()).is_ok(),
                valid,
                "length {length}",
            );
            if !valid {
                assert!(EditorSourceToolInstallationReceipt::issue(receipt.manifest).is_err());
            }
        }
        for (id, valid) in [([0; 32], false), ([1; 32], true), ([255; 32], true)] {
            let mut receipt = fixture();
            receipt.id = id;
            assert_eq!(receipt.encode().is_ok(), valid);
            let input = fixture_json().replace(
                &serde_json::to_string(&[1; 32]).unwrap(),
                &serde_json::to_string(&id).unwrap(),
            );
            assert_eq!(
                EditorSourceToolInstallationReceipt::decode(input.as_bytes()).is_ok(),
                valid,
            );
        }
    }

    #[test]
    fn enforces_receipt_byte_limit_before_parsing() {
        for (length, valid) in [(4095, true), (4096, true), (4097, false)] {
            let mut bytes = fixture().encode().unwrap();
            bytes.resize(length, b' ');
            let result = EditorSourceToolInstallationReceipt::decode(&bytes);
            assert_eq!(result.is_ok(), valid, "receipt length {length}");
            if !valid {
                assert_eq!(
                    result.unwrap_err(),
                    "installation receipt exceeds byte limit"
                );
            }
        }
        let oversized_invalid = vec![b'!'; MAX_RECEIPT_BYTES as usize + 1];
        assert_eq!(
            EditorSourceToolInstallationReceipt::decode(&oversized_invalid).unwrap_err(),
            "installation receipt exceeds byte limit",
        );
    }
}
