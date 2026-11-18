//! Exact endpoint colors avoid allowances for sRGB conversion, blending and UNORM rounding.
//!
//! The 32 by 24 Camera has a viewport height of 24. A centered-anchor 10 by 6 Sprite at
//! (-3, 2) covers physical columns [8, 18) and rows [7, 13). Its opaque palette, full-strength
//! Material and white tint produce exact RGBA8 endpoints over exact transparent black.

pub(super) const WIDTH: u32 = 32;
pub(super) const HEIGHT: u32 = 24;
pub(super) const A: [u8; 4] = [255, 0, 0, 255];
pub(super) const B: [u8; 4] = [0, 255, 0, 255];

fn image(palette: Option<[u8; 4]>) -> Vec<[u8; 4]> {
    (0..HEIGHT)
        .flat_map(|y| {
            (0..WIDTH).map(move |x| {
                if (8..18).contains(&x) && (7..13).contains(&y) {
                    palette.unwrap_or([0; 4])
                } else {
                    [0; 4]
                }
            })
        })
        .collect()
}

pub(super) fn assert_image(actual: &[[u8; 4]], palette: Option<[u8; 4]>, label: &str) {
    let expected = image(palette);
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: complete RGBA8 extent"
    );
    if let Some((index, (actual, expected))) = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .find(|(_, (actual, expected))| actual != expected)
    {
        panic!(
            "{label}: pixel ({}, {}): actual={actual:?}, expected={expected:?}",
            index % WIDTH as usize,
            index / WIDTH as usize,
        );
    }
}

#[test]
fn exact_oracle_rejects_late_palette_clear_mirrors_and_background_leaks() {
    let correct = image(Some(A));
    assert_image(&correct, Some(A), "original A");
    assert_eq!(correct.iter().filter(|pixel| **pixel == A).count(), 60);
    assert_eq!(correct[(7 * WIDTH + 8) as usize], A);
    assert_eq!(correct[(12 * WIDTH + 17) as usize], A);
    assert_eq!(correct[(13 * WIDTH + 18) as usize], [0; 4]);
    assert_ne!(A, B);

    let horizontal = (0..HEIGHT)
        .flat_map(|y| {
            let correct = &correct;
            (0..WIDTH).map(move |x| correct[(y * WIDTH + WIDTH - 1 - x) as usize])
        })
        .collect::<Vec<_>>();
    let vertical = correct
        .chunks_exact(WIDTH as usize)
        .rev()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let mut leak = correct.clone();
    leak[0] = [1, 0, 0, 0];
    let mut nearly_red = correct.clone();
    nearly_red[(7 * WIDTH + 8) as usize] = [254, 0, 0, 255];
    for incorrect in [
        image(Some(B)),
        image(None),
        horizontal,
        vertical,
        leak,
        nearly_red,
    ] {
        assert!(
            std::panic::catch_unwind(|| assert_image(&incorrect, Some(A), "negative control"))
                .is_err()
        );
    }
    assert_image(&image(None), None, "omitted item");
    assert!(std::panic::catch_unwind(|| assert_image(&correct, None, "leaked item")).is_err());
    assert!(
        std::panic::catch_unwind(|| assert_image(&correct[..767], Some(A), "short copy")).is_err()
    );
}
