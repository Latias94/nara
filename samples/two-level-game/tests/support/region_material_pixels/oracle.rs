use two_level_game::{levels::LevelId, region_material};

use crate::gpu::EXTENT;

const SOURCE: [u8; 4] = [64, 224, 180, 255];
const PALETTE: [u8; 4] = [255, 255, 255, 0];
const GRADE: [f64; 4] = [1.0, 0.5, 0.25, 0.125];
const TOLERANCE: u8 = 3;
const RELICS: [[usize; 2]; 2] = [[300, 198], [300, 342]];

#[derive(Clone, Copy)]
pub enum RoomVisual {
    Closed,
    Open,
    Collected,
}

pub struct Expected {
    pub graded: [u8; 4],
}

impl Expected {
    pub fn new() -> Self {
        assert_eq!(region_material::SOURCE_PIXEL, SOURCE);
        assert_eq!(region_material::PALETTE_PIXEL, PALETTE);
        assert_eq!(region_material::GRADE.map(f64::from), GRADE);
        let decoded = SOURCE.map(decode);
        let palette = PALETTE.map(decode);
        let graded = [
            unorm(decoded[0] * GRADE[0] + palette[0] * GRADE[3]),
            unorm(decoded[1] * GRADE[1] + palette[1] * GRADE[3]),
            unorm(decoded[2] * GRADE[2] + palette[2] * GRADE[3]),
            SOURCE[3],
        ];
        assert_eq!(graded, [45, 127, 61, 255]);
        let ungraded = [
            unorm(decoded[0]),
            unorm(decoded[1]),
            unorm(decoded[2]),
            SOURCE[3],
        ];
        assert!((0..3).all(|channel| graded[channel].abs_diff(ungraded[channel]) > 2 * TOLERANCE));
        let missing_palette = [
            unorm(decoded[0] * GRADE[0]),
            unorm(decoded[1] * GRADE[1]),
            unorm(decoded[2] * GRADE[2]),
        ];
        assert!(
            (0..3)
                .all(|channel| graded[channel].abs_diff(missing_palette[channel]) > 2 * TOLERANCE)
        );
        Self { graded }
    }

    pub fn check(&self, pixels: &[u8], rooms: [RoomVisual; 2], level: LevelId) {
        assert_eq!(pixels.len(), (EXTENT.width * EXTENT.height * 4) as usize);
        for (index, [x, y]) in RELICS.into_iter().enumerate() {
            let (expected, label) = match rooms[index] {
                RoomVisual::Open => (self.graded, "actual relic Material draw"),
                RoomVisual::Closed => (background(level), "unloaded region"),
                RoomVisual::Collected => (
                    if index == 0 {
                        [0.13, 0.26, 0.20, 1.0]
                    } else {
                        [0.12, 0.23, 0.32, 1.0]
                    }
                    .map(unorm),
                    "collected relic leaves the authored floor",
                ),
            };
            for py in y - 2..=y + 2 {
                for px in x - 2..=x + 2 {
                    check(pixels, px, py, expected, label);
                }
            }
        }
        for [x, y] in [[4, 4], [955, 4], [4, 535], [955, 535]] {
            check(
                pixels,
                x,
                y,
                background(level),
                "untouched level background",
            );
        }
    }
}

fn background(level: LevelId) -> [u8; 4] {
    match level {
        LevelId::Meadow => [0.04, 0.13, 0.09, 1.0],
        LevelId::Cavern => [0.07, 0.05, 0.15, 1.0],
    }
    .map(unorm)
}

fn decode(byte: u8) -> f64 {
    let encoded = f64::from(byte) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn unorm(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn check(pixels: &[u8], x: usize, y: usize, expected: [u8; 4], label: &str) {
    let offset = (y * EXTENT.width as usize + x) * 4;
    let actual = &pixels[offset..offset + 4];
    assert!(
        actual[..3]
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= TOLERANCE)
            && actual[3] == expected[3],
        "{label} at ({x}, {y}): actual={actual:?}, expected={expected:?}, RGB tolerance={TOLERANCE}",
    );
}
