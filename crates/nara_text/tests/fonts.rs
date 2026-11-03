use std::sync::Arc;

use nara_text::{
    FontFace, FontFaceError, MAX_FONT_CONTAINER_BYTES, MAX_FONT_FACES, MAX_TOTAL_FONT_BYTES,
    TextDiagnostic, TextEngine, TextEngineCreateError, TextLayoutRequest, TextLimits,
    bundled_font_face,
};

const PRIMARY_BYTES: &[u8] = include_bytes!("../assets/FiraSans-Bold.ttf");
const FALLBACK_BYTES: &[u8] = include_bytes!("fixtures/FiraMono-Medium.ttf");

#[test]
fn one_face_constructor_preserves_layout_raster_and_primary_identity() {
    let face = bundled_font_face().unwrap();
    let mut single = TextEngine::new(face.clone(), TextLimits::default()).unwrap();
    let mut explicit = TextEngine::with_fonts(vec![face.clone()], TextLimits::default()).unwrap();
    let request = TextLayoutRequest::new("Café Ω abc אבג", 20.0)
        .unwrap()
        .with_wrap_width(80.0)
        .unwrap()
        .with_scale_factor(1.5)
        .unwrap();

    let expected = single.layout(&request).unwrap();
    let actual = explicit.layout(&request).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(explicit.font_face_id(), face.id());
    assert!(explicit.contains_font(face.id()));
    for glyph in actual.glyphs() {
        assert_eq!(glyph.key().face_id(), face.id());
        assert_eq!(
            explicit.rasterize(glyph.key()).unwrap(),
            single.rasterize(glyph.key()).unwrap()
        );
    }
}

#[test]
fn empty_duplicate_and_excessive_font_sets_are_rejected() {
    assert_eq!(MAX_FONT_FACES, 8);
    assert!(matches!(
        TextEngine::with_fonts(Vec::new(), TextLimits::default()),
        Err(TextEngineCreateError::EmptyFontSet)
    ));

    let face = bundled_font_face().unwrap();
    let duplicate = FontFace::from_bytes(PRIMARY_BYTES.to_vec(), 0).unwrap();
    assert!(matches!(
        TextEngine::with_fonts(vec![face.clone(), duplicate], TextLimits::default()),
        Err(TextEngineCreateError::DuplicateFontFace { face_id }) if face_id == face.id()
    ));
    assert!(matches!(
        TextEngine::with_fonts(vec![face; MAX_FONT_FACES + 1], TextLimits::default()),
        Err(TextEngineCreateError::FontCountExceeded { actual, maximum })
            if actual == MAX_FONT_FACES + 1 && maximum == MAX_FONT_FACES
    ));
}

#[test]
fn eight_distinct_faces_preserve_explicit_order_even_with_identical_family_names() {
    let fonts = (0..MAX_FONT_FACES)
        .map(|marker| {
            let mut bytes = PRIMARY_BYTES.to_vec();
            bytes.push(u8::try_from(marker).unwrap());
            FontFace::from_bytes(bytes, 0).unwrap()
        })
        .collect::<Vec<_>>();
    let mut engine = TextEngine::with_fonts(fonts.clone(), TextLimits::default()).unwrap();
    for face in &fonts {
        assert!(engine.contains_font(face.id()));
    }
    assert!(!engine.contains_font(bundled_font_face().unwrap().id()));
    let layout = engine
        .layout(&TextLayoutRequest::new("A", 20.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs()[0].key().face_id(), fonts[0].id());

    let mut reversed = fonts;
    reversed.reverse();
    let mut engine = TextEngine::with_fonts(reversed.clone(), TextLimits::default()).unwrap();
    let layout = engine
        .layout(&TextLayoutRequest::new("A", 20.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs()[0].key().face_id(), reversed[0].id());
}

#[test]
fn full_container_byte_budget_is_exact_and_cannot_be_raised_by_shared_ownership() {
    assert_eq!(MAX_FONT_CONTAINER_BYTES, 32 * 1024 * 1024);
    assert_eq!(MAX_TOTAL_FONT_BYTES, 64 * 1024 * 1024);
    let mut bytes = collection_bytes(&[PRIMARY_BYTES, FALLBACK_BYTES]);
    bytes.resize(MAX_FONT_CONTAINER_BYTES, 0);
    let shared = Arc::new(bytes);
    let primary = FontFace::from_shared_bytes(Arc::clone(&shared), 0).unwrap();
    let fallback = FontFace::from_shared_bytes(shared, 1).unwrap();
    let engine = TextEngine::with_fonts(
        vec![primary.clone(), fallback.clone()],
        TextLimits::default(),
    )
    .expect("two complete 32 MiB containers meet the total byte ceiling exactly");
    assert!(engine.contains_font(primary.id()));
    assert!(engine.contains_font(fallback.id()));
    drop(engine);

    let extra = bundled_font_face().unwrap();
    let expected_bytes = MAX_TOTAL_FONT_BYTES + extra.byte_len();
    assert!(matches!(
        TextEngine::with_fonts(vec![primary, fallback, extra], TextLimits::default()),
        Err(TextEngineCreateError::FontBytesExceeded { actual, maximum })
            if actual == expected_bytes && maximum == MAX_TOTAL_FONT_BYTES
    ));
}

#[test]
fn collection_faces_retain_original_bytes_and_index_identity_through_rasterization() {
    let bytes = collection_bytes(&[PRIMARY_BYTES, FALLBACK_BYTES]);
    let expected_primary_id = face_id(&bytes, 0);
    let expected_fallback_id = face_id(&bytes, 1);
    let shared = Arc::new(bytes);
    let primary = FontFace::from_shared_bytes(Arc::clone(&shared), 0).unwrap();
    let fallback = FontFace::from_shared_bytes(shared, 1).unwrap();
    assert_eq!(primary.id().as_bytes(), &expected_primary_id);
    assert_eq!(fallback.id().as_bytes(), &expected_fallback_id);
    assert_ne!(primary.id(), fallback.id());
    assert_eq!(primary.family_name(), "Fira Sans");
    assert_eq!(fallback.family_name(), "Fira Mono");

    let mut engine = TextEngine::with_fonts(
        vec![primary.clone(), fallback.clone()],
        TextLimits::default(),
    )
    .unwrap();
    let layout = engine
        .layout(&TextLayoutRequest::new("A\u{2229}", 24.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs().len(), 2);
    assert_eq!(layout.glyphs()[0].key().face_id(), primary.id());
    assert_eq!(layout.glyphs()[1].key().face_id(), fallback.id());
    assert!(layout.diagnostics().is_empty());
    let mut fallback_engine = TextEngine::new(fallback, TextLimits::default()).unwrap();
    let key = layout.glyphs()[1].key();
    let raster = engine.rasterize(key).unwrap();
    assert!(raster.width() > 0 && raster.height() > 0);
    assert_eq!(raster, fallback_engine.rasterize(key).unwrap());
}

#[test]
fn unselected_collection_faces_cannot_supply_implicit_glyphs() {
    let bytes = collection_bytes(&[PRIMARY_BYTES, FALLBACK_BYTES]);
    let primary = FontFace::from_bytes(bytes, 0).unwrap();
    let mut engine = TextEngine::new(primary.clone(), TextLimits::default()).unwrap();
    let layout = engine
        .layout(&TextLayoutRequest::new("\u{2229}", 24.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs().len(), 1);
    assert_eq!(layout.glyphs()[0].key().face_id(), primary.id());
    assert_eq!(layout.glyphs()[0].key().glyph_id(), 0);
    assert_eq!(
        layout.diagnostics(),
        &[TextDiagnostic::MissingGlyph { scalar: '\u{2229}' }]
    );
}

#[test]
fn same_family_collection_faces_are_distinct_ordered_selections() {
    let shared = Arc::new(collection_bytes(&[PRIMARY_BYTES, PRIMARY_BYTES]));
    let first = FontFace::from_shared_bytes(Arc::clone(&shared), 0).unwrap();
    let second = FontFace::from_shared_bytes(shared, 1).unwrap();
    let mut engine =
        TextEngine::with_fonts(vec![second.clone(), first.clone()], TextLimits::default()).unwrap();
    assert_eq!(engine.font_face_id(), second.id());
    assert!(engine.contains_font(first.id()));
    let layout = engine
        .layout(&TextLayoutRequest::new("A", 24.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs().len(), 1);
    assert_eq!(layout.glyphs()[0].key().face_id(), second.id());
    assert!(engine.rasterize(layout.glyphs()[0].key()).unwrap().width() > 0);
}

#[test]
fn collection_indices_and_offset_tables_are_checked_before_admission() {
    let valid = collection_bytes(&[PRIMARY_BYTES, FALLBACK_BYTES]);
    for requested in [2, u32::MAX] {
        assert_eq!(
            FontFace::from_bytes(valid.clone(), requested),
            Err(FontFaceError::FaceIndexOutOfBounds {
                requested,
                faces: 2
            })
        );
    }
    let mut invalid_offset = valid;
    invalid_offset[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        FontFace::from_bytes(invalid_offset, 1),
        Err(FontFaceError::Malformed)
    );
    let mut truncated_offsets = b"ttcf\0\x01\0\0".to_vec();
    truncated_offsets.extend_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        FontFace::from_bytes(truncated_offsets, 0),
        Err(FontFaceError::Malformed)
    );
}

fn face_id(bytes: &[u8], face_index: u32) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    hasher.update(&face_index.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn collection_bytes(fonts: &[&[u8]]) -> Vec<u8> {
    let mut bytes = b"ttcf\0\x01\0\0".to_vec();
    bytes.extend_from_slice(&u32::try_from(fonts.len()).unwrap().to_be_bytes());
    bytes.resize(12 + fonts.len() * 4, 0);
    for (font_index, font) in fonts.iter().enumerate() {
        let face_offset = u32::try_from(bytes.len()).unwrap();
        let offset_start = 12 + font_index * 4;
        bytes[offset_start..offset_start + 4].copy_from_slice(&face_offset.to_be_bytes());
        let mut relocated = font.to_vec();
        let table_count = u16::from_be_bytes(font[4..6].try_into().unwrap());
        for table_index in 0..usize::from(table_count) {
            let offset_start = 12 + table_index * 16 + 8;
            let offset =
                u32::from_be_bytes(font[offset_start..offset_start + 4].try_into().unwrap());
            relocated[offset_start..offset_start + 4]
                .copy_from_slice(&(offset + face_offset).to_be_bytes());
        }
        bytes.extend_from_slice(&relocated);
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }
    bytes
}
