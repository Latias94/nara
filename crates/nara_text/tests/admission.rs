use nara_text::{
    BUNDLED_FONT_BYTE_LEN, BUNDLED_FONT_LICENSE_SHA256, BUNDLED_FONT_SHA256, FontFace,
    FontFaceError, MAX_FONT_CONTAINER_BYTES, bundled_font_face,
};
use sha2::{Digest, Sha256};

const BUNDLED_BYTES: &[u8] = include_bytes!("../assets/FiraSans-Bold.ttf");
const LICENSE_BYTES: &[u8] = include_bytes!("../assets/FiraSans-LICENSE.txt");

#[test]
fn bundled_face_has_exact_provenance_and_metadata() {
    assert_eq!(BUNDLED_BYTES.len(), BUNDLED_FONT_BYTE_LEN);
    assert_eq!(sha256(BUNDLED_BYTES), BUNDLED_FONT_SHA256);
    assert_eq!(sha256(LICENSE_BYTES), BUNDLED_FONT_LICENSE_SHA256);

    let face = bundled_font_face().expect("the bundled face should remain admissible");
    assert_eq!(face.byte_len(), 438_028);
    assert_eq!(face.face_index(), 0);
    assert_eq!(face.family_name(), "Fira Sans");
    assert!(!face.id().as_bytes().iter().all(|byte| *byte == 0));
}

#[test]
fn malformed_collection_and_nonzero_face_index_fail_before_publication() {
    assert_eq!(
        FontFace::from_bytes(Vec::new(), 0),
        Err(FontFaceError::Empty)
    );
    assert!(matches!(
        FontFace::from_bytes(vec![0; 32], 0),
        Err(FontFaceError::Malformed)
    ));
    assert!(matches!(
        FontFace::from_bytes(BUNDLED_BYTES[..64].to_vec(), 0),
        Err(FontFaceError::Malformed)
    ));
    assert!(matches!(
        FontFace::from_bytes(BUNDLED_BYTES.to_vec(), 1),
        Err(FontFaceError::UnsupportedFaceIndex { requested: 1 })
    ));

    let mut collection = b"ttcf\0\x01\0\0\0\0\0\x02".to_vec();
    collection.extend_from_slice(&20_u32.to_be_bytes());
    collection.extend_from_slice(&20_u32.to_be_bytes());
    assert!(matches!(
        FontFace::from_bytes(collection, 0),
        Err(FontFaceError::CollectionUnsupported { faces: 2 })
    ));
}

#[test]
fn oversized_font_is_rejected_before_parser_work() {
    let oversized = vec![0; MAX_FONT_CONTAINER_BYTES + 1];
    assert_eq!(
        FontFace::from_bytes(oversized, 0),
        Err(FontFaceError::ContainerTooLarge {
            actual: MAX_FONT_CONTAINER_BYTES + 1,
            maximum: MAX_FONT_CONTAINER_BYTES,
        })
    );
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
