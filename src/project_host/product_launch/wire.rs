//! Closed value contract for supervised desktop product activation.

mod codec;
mod launch;
mod ready;

use std::{error::Error, fmt};

use super::root::ProductProjectRootError;

pub(crate) use codec::{FRAME_LENGTH_BYTES, frame_length};
pub(crate) use launch::SUPERVISED_DESKTOP_LAUNCH_MAGIC;
pub use launch::{
    SupervisedDesktopGenerationId, SupervisedDesktopLaunchContext,
    SupervisedDesktopProductIdentity, SupervisedDesktopSessionNonce,
};
pub use ready::{
    SupervisedDesktopReadyAttestation, SupervisedDesktopRuntimeClosureFacts,
    supervised_desktop_ready_frame_length,
};

#[cfg(test)]
use super::source::MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES;
#[cfg(test)]
use launch::MAX_PRODUCT_IDENTITY_BYTES;
#[cfg(test)]
use nara_fs::ContentDigest;
#[cfg(test)]
use std::path::PathBuf;

/// Maximum executable size admitted by one supervised desktop generation and child measurement.
#[doc(hidden)]
pub const MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES: u64 = 1024 * 1024 * 1024;

/// Closed version of the supervised desktop launch-context contract.
#[doc(hidden)]
pub const SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION: u16 = 1;

/// Closed version of the one-shot supervised desktop READY attestation.
#[doc(hidden)]
pub const SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION: u16 = 1;

/// Maximum canonical launch-context frame accepted over the inherited bootstrap endpoint.
#[doc(hidden)]
pub const MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES: usize = 512 * 1024;

/// Maximum canonical READY frame accepted over the inherited bootstrap endpoint.
#[doc(hidden)]
pub const MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES: usize = 512 * 1024;

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisedDesktopWireError {
    InvalidField(&'static str),
    FieldTooLarge { field: &'static str, limit: usize },
    AllocationFailed,
    ProjectRoot(ProductProjectRootError),
    InvalidMagic,
    UnsupportedVersion,
    InvalidLength,
    IncompleteFrame,
    InvalidUtf8(&'static str),
    InvalidPathEncoding,
    FactMismatch(&'static str),
    TrailingBytes,
    NonCanonicalEncoding,
}

impl fmt::Display for SupervisedDesktopWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(field) => write!(formatter, "supervised desktop {field} is invalid"),
            Self::FieldTooLarge { field, limit } => write!(
                formatter,
                "supervised desktop {field} exceeds its {limit}-byte limit"
            ),
            Self::AllocationFailed => {
                formatter.write_str("supervised desktop wire allocation failed")
            }
            Self::ProjectRoot(error) => write!(formatter, "{error}"),
            Self::InvalidMagic => {
                formatter.write_str("supervised desktop wire magic does not match")
            }
            Self::UnsupportedVersion => {
                formatter.write_str("supervised desktop wire version is unsupported")
            }
            Self::InvalidLength => formatter.write_str("supervised desktop wire length is invalid"),
            Self::IncompleteFrame => {
                formatter.write_str("supervised desktop wire frame is incomplete")
            }
            Self::InvalidUtf8(field) => {
                write!(formatter, "supervised desktop {field} is not valid UTF-8")
            }
            Self::InvalidPathEncoding => {
                formatter.write_str("supervised desktop project-root encoding is invalid")
            }
            Self::FactMismatch(field) => {
                write!(formatter, "supervised desktop {field} does not match")
            }
            Self::TrailingBytes => {
                formatter.write_str("supervised desktop wire frame contains trailing bytes")
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("supervised desktop wire frame is not canonical")
            }
        }
    }
}

impl Error for SupervisedDesktopWireError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ProjectRoot(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    const EXPLICIT_PROJECT_ROOT: &str = r"C:\source\game";
    #[cfg(not(windows))]
    const EXPLICIT_PROJECT_ROOT: &str = "/source/game";

    fn product_identity() -> SupervisedDesktopProductIdentity {
        SupervisedDesktopProductIdentity::from_cargo_product(
            "nara-test-product",
            "0.1.0",
            "desktop",
        )
        .unwrap()
    }

    fn launch_context() -> SupervisedDesktopLaunchContext {
        SupervisedDesktopLaunchContext::new(
            PathBuf::from(EXPLICIT_PROJECT_ROOT),
            "nara.test.project",
            "desktop",
            product_identity(),
            SupervisedDesktopGenerationId::new([0x11; 32]).unwrap(),
            SupervisedDesktopSessionNonce::new([0x22; 32]).unwrap(),
            ContentDigest::of_bytes(b"executable"),
            ContentDigest::of_bytes(b"launch manifest"),
            b"canonical source receipt".to_vec(),
        )
        .unwrap()
    }

    fn runtime_closure() -> SupervisedDesktopRuntimeClosureFacts {
        SupervisedDesktopRuntimeClosureFacts::new(
            ContentDigest::of_bytes(b"manifest"),
            ContentDigest::of_bytes(b"authoring bundle"),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ContentDigest::of_bytes(b"runtime content"),
            ContentDigest::of_bytes(b"closure plan"),
        )
        .unwrap()
    }

    #[test]
    fn supervised_launch_context_is_bounded_and_canonical() {
        let context = launch_context();
        let encoded = context.encode_canonical().unwrap();

        assert!(encoded.len() <= MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES);
        assert_eq!(
            SupervisedDesktopLaunchContext::decode_canonical(&encoded).unwrap(),
            context
        );
        assert_eq!(
            context.source_receipt_digest(),
            ContentDigest::of_bytes(context.source_receipt())
        );
    }

    #[test]
    fn supervised_launch_context_rejects_source_receipt_tampering() {
        let mut encoded = launch_context().encode_canonical().unwrap();
        *encoded.last_mut().unwrap() ^= 0x01;

        assert_eq!(
            SupervisedDesktopLaunchContext::decode_canonical(&encoded),
            Err(SupervisedDesktopWireError::FactMismatch(
                "source receipt digest"
            ))
        );
    }

    #[test]
    fn ready_attestation_echoes_one_exact_launch_and_child_runtime_closure() {
        let launch = launch_context();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            ContentDigest::of_bytes(b"executable"),
            runtime_closure(),
        )
        .unwrap();
        let encoded = ready.encode_canonical().unwrap();
        let decoded = SupervisedDesktopReadyAttestation::decode_canonical(&encoded).unwrap();

        assert!(decoded.matches_launch(&launch));
        assert_eq!(decoded, ready);
        assert_eq!(
            decoded.runtime_closure().content_digest(),
            ContentDigest::of_bytes(b"runtime content")
        );
        assert_eq!(
            decoded.runtime_closure().authoring_bundle_digest(),
            ContentDigest::of_bytes(b"authoring bundle")
        );
    }

    #[test]
    fn ready_attestation_exposes_child_identity_mismatches_to_the_parent() {
        let launch = launch_context();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            SupervisedDesktopProductIdentity::from_cargo_product(
                "nara-wrong-product",
                "0.1.0",
                "desktop",
            )
            .unwrap(),
            ContentDigest::of_bytes(b"different executable"),
            runtime_closure(),
        )
        .unwrap();

        assert!(!ready.matches_launch(&launch));
        assert_ne!(ready.executable_digest(), launch.executable_digest());
        assert_ne!(ready.product_identity(), launch.product_identity());
    }

    #[test]
    fn ready_launch_match_compares_the_complete_source_receipt() {
        let mut launch = launch_context();
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch,
            product_identity(),
            ContentDigest::of_bytes(b"executable"),
            runtime_closure(),
        )
        .unwrap();
        launch.source_receipt[0] ^= 0x01;

        assert_eq!(
            ready.source_receipt_digest(),
            launch.source_receipt_digest()
        );
        assert!(!ready.matches_launch(&launch));
    }

    #[test]
    fn ready_frame_length_is_incremental_but_decode_is_single_record_exact() {
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch_context(),
            product_identity(),
            ContentDigest::of_bytes(b"executable"),
            runtime_closure(),
        )
        .unwrap();
        let encoded = ready.encode_canonical().unwrap();

        assert_eq!(supervised_desktop_ready_frame_length(&[]).unwrap(), None);
        assert_eq!(
            supervised_desktop_ready_frame_length(&encoded[..4]).unwrap(),
            None
        );
        assert_eq!(
            supervised_desktop_ready_frame_length(&encoded).unwrap(),
            Some(encoded.len())
        );

        let mut duplicate = encoded.clone();
        duplicate.extend_from_slice(&encoded);
        assert_eq!(
            SupervisedDesktopReadyAttestation::decode_canonical(&duplicate),
            Err(SupervisedDesktopWireError::TrailingBytes)
        );
    }

    #[test]
    fn ready_frame_length_accepts_the_exact_bound_and_rejects_one_byte_more() {
        let ready = SupervisedDesktopReadyAttestation::from_launch(
            &launch_context(),
            product_identity(),
            ContentDigest::of_bytes(b"executable"),
            runtime_closure(),
        )
        .unwrap();
        let encoded = ready.encode_canonical().unwrap();
        let header_length =
            b"NARA_SPL_READY\0".len() + std::mem::size_of::<u16>() + FRAME_LENGTH_BYTES;
        let mut header = encoded[..header_length].to_vec();
        let payload_length = MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES - header_length;
        header[header_length - FRAME_LENGTH_BYTES..]
            .copy_from_slice(&u32::try_from(payload_length).unwrap().to_le_bytes());

        assert_eq!(
            supervised_desktop_ready_frame_length(&header).unwrap(),
            Some(MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES)
        );

        let oversized_payload = payload_length + 1;
        header[header_length - FRAME_LENGTH_BYTES..]
            .copy_from_slice(&u32::try_from(oversized_payload).unwrap().to_le_bytes());
        assert_eq!(
            supervised_desktop_ready_frame_length(&header),
            Err(SupervisedDesktopWireError::InvalidLength)
        );
    }

    #[test]
    fn supervised_wire_rejects_unbounded_or_nonfresh_launch_facts() {
        assert_eq!(
            SupervisedDesktopSessionNonce::new([0; 32]),
            Err(SupervisedDesktopWireError::InvalidField("session nonce"))
        );
        assert!(matches!(
            SupervisedDesktopLaunchContext::new(
                PathBuf::from(EXPLICIT_PROJECT_ROOT),
                "nara.test.project",
                "desktop",
                product_identity(),
                SupervisedDesktopGenerationId::new([0x11; 32]).unwrap(),
                SupervisedDesktopSessionNonce::new([0x22; 32]).unwrap(),
                ContentDigest::of_bytes(b"executable"),
                ContentDigest::of_bytes(b"launch manifest"),
                vec![0; MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES + 1],
            ),
            Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "source receipt",
                limit: MAX_SUPERVISED_DESKTOP_SOURCE_RECEIPT_BYTES,
            })
        ));
        assert_eq!(
            SupervisedDesktopLaunchContext::new(
                PathBuf::from(EXPLICIT_PROJECT_ROOT),
                "nara.test.project",
                "desktop",
                product_identity(),
                SupervisedDesktopGenerationId::new([0x11; 32]).unwrap(),
                SupervisedDesktopSessionNonce::new([0x22; 32]).unwrap(),
                ContentDigest::from_parts(MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES + 1, [0x44; 32],),
                ContentDigest::of_bytes(b"launch manifest"),
                b"canonical source receipt".to_vec(),
            ),
            Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "executable digest",
                limit: MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES as usize,
            })
        );
        assert_eq!(
            SupervisedDesktopRuntimeClosureFacts::new(
                ContentDigest::of_bytes(b"manifest"),
                ContentDigest::from_parts(0, [0x33; 32]),
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                ContentDigest::of_bytes(b"runtime content"),
                ContentDigest::of_bytes(b"closure plan"),
            ),
            Err(SupervisedDesktopWireError::InvalidField(
                "runtime authoring bundle digest"
            ))
        );
        assert_eq!(
            SupervisedDesktopProductIdentity::from_cargo_product("nara/test", "0.1.0", "desktop",),
            Err(SupervisedDesktopWireError::InvalidField(
                "Cargo package name"
            ))
        );
        assert_eq!(
            SupervisedDesktopProductIdentity::new("untyped-product"),
            Err(SupervisedDesktopWireError::InvalidField("product identity"))
        );
        assert!(matches!(
            SupervisedDesktopProductIdentity::from_cargo_product(
                &"x".repeat(MAX_PRODUCT_IDENTITY_BYTES + 1),
                "0.1.0",
                "desktop",
            ),
            Err(SupervisedDesktopWireError::FieldTooLarge {
                field: "Cargo package name",
                limit: MAX_PRODUCT_IDENTITY_BYTES,
            })
        ));
    }
}
