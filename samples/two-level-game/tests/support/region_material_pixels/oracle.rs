use two_level_game::{levels::LevelId, region_material, region_tileset};

use crate::gpu::EXTENT;

const SOURCE: [u8; 4] = [64, 224, 180, 255];
const PALETTE: [u8; 4] = [255, 255, 255, 0];
const GRADE: [f64; 4] = [1.0, 0.5, 0.25, 0.125];
const TOLERANCE: u8 = 3;
const RELICS: [[usize; 2]; 2] = [[300, 198], [300, 342]];
const ORIGINS: [[f64; 2]; 2] = [[-120.0, 48.0], [-120.0, -48.0]];
const TILE_SIZE: [f64; 2] = [16.0, 20.0];
const FLOOR_COLORS: [[[f64; 4]; 2]; 2] = [
    [[0.20, 0.40, 0.35, 1.0], [0.30, 0.60, 0.50, 1.0]],
    [[0.25, 0.35, 0.60, 1.0], [0.40, 0.50, 0.75, 1.0]],
];

#[derive(Clone, Copy)]
pub enum RoomVisual {
    Closed,
    Open,
    Collected,
}

pub struct Expected {
    pub graded: [u8; 4],
    pub floors: [[[u8; 4]; 2]; 2],
}

impl Expected {
    pub fn new() -> Self {
        assert_eq!(region_material::SOURCE_PIXEL, SOURCE);
        assert_eq!(region_material::PALETTE_PIXEL, PALETTE);
        assert_eq!(region_material::GRADE.map(f64::from), GRADE);
        assert_eq!(
            region_tileset::TILE_SIZE.to_array().map(f64::from),
            TILE_SIZE
        );
        assert_eq!(
            region_tileset::SIDE_ROOM_ORIGIN.to_array().map(f64::from),
            ORIGINS[0]
        );
        assert_eq!(
            region_tileset::LOOKOUT_ORIGIN.to_array().map(f64::from),
            ORIGINS[1]
        );
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
        let floors = FLOOR_COLORS.map(|colors| colors.map(floor_color));
        assert_eq!(
            floors,
            [
                [[3, 76, 41, 255], [4, 114, 58, 255]],
                [[3, 67, 70, 255], [5, 95, 87, 255]],
            ]
        );
        Self { graded, floors }
    }

    pub fn check(&self, pixels: &[u8], rooms: [RoomVisual; 2], level: LevelId) {
        assert_eq!(pixels.len(), (EXTENT.width * EXTENT.height * 4) as usize);
        for (index, [x, y]) in RELICS.into_iter().enumerate() {
            for py in y - 2..=y + 2 {
                for px in x - 2..=x + 2 {
                    let (expected, label) = match rooms[index] {
                        RoomVisual::Open => (self.graded, "actual relic Material draw"),
                        RoomVisual::Closed => (background(level), "unloaded region"),
                        RoomVisual::Collected => (
                            self.floor(index, px, py),
                            "collected relic exposes the patterned Tilemap",
                        ),
                    };
                    check(pixels, px, py, expected, label);
                }
            }
            for cx in -2..=1 {
                for cy in -1..=0 {
                    let local_x = if cx == -2 || cx == 1 {
                        f64::from(cx) * TILE_SIZE[0] + 4.0
                    } else {
                        (f64::from(cx) + 0.5) * TILE_SIZE[0]
                    };
                    let local_y = if cx == 1 && cy == 0 {
                        14.0
                    } else {
                        (f64::from(cy) + 0.5) * TILE_SIZE[1]
                    };
                    let x = (480.0 + (ORIGINS[index][0] + local_x) * 1.5) as usize;
                    let y = (270.0 - (ORIGINS[index][1] + local_y) * 1.5) as usize;
                    for py in y - 1..=y + 1 {
                        for px in x - 1..=x + 1 {
                            let (expected, label) = match rooms[index] {
                                RoomVisual::Open | RoomVisual::Collected => {
                                    (self.floor(index, px, py), "original captured Tilemap cell")
                                }
                                RoomVisual::Closed => (
                                    background(level),
                                    "independent close removes the Tilemap cell",
                                ),
                            };
                            check(pixels, px, py, expected, label);
                        }
                    }
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

    fn floor(&self, room: usize, x: usize, y: usize) -> [u8; 4] {
        let world_x = (x as f64 + 0.5 - 480.0) / 1.5;
        let world_y = (270.0 - y as f64 - 0.5) / 1.5;
        let cx = ((world_x - ORIGINS[room][0]) / TILE_SIZE[0]).floor() as i32;
        let cy = ((world_y - ORIGINS[room][1]) / TILE_SIZE[1]).floor() as i32;
        assert!((-2..=1).contains(&cx) && (-1..=0).contains(&cy));
        self.floors[room][(cx + cy).rem_euclid(2) as usize]
    }
}

fn floor_color(tint: [f64; 4]) -> [u8; 4] {
    let decoded = SOURCE.map(decode);
    [
        unorm(decoded[0] * tint[0]),
        unorm(decoded[1] * tint[1]),
        unorm(decoded[2] * tint[2]),
        SOURCE[3],
    ]
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
