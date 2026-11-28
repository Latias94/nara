use nara::prelude::Vec2;

pub(crate) const ESCAPE_ROUTE: [(usize, usize); 16] = [
    (2, 5),
    (8, 5),
    (8, 2),
    (9, 2),
    (8, 2),
    (8, 4),
    (17, 4),
    (17, 2),
    (17, 4),
    (19, 4),
    (19, 9),
    (3, 9),
    (3, 8),
    (3, 9),
    (17, 9),
    (17, 8),
];

#[cfg(any(feature = "desktop", test))]
pub(crate) const SECOND_ESCAPE_ROUTE: [Vec2; 9] = [
    Vec2::new(-96.0, -96.0),
    Vec2::new(-96.0, -64.0),
    Vec2::new(256.0, -64.0),
    Vec2::new(280.0, -64.0),
    Vec2::new(280.0, 112.0),
    Vec2::new(160.0, 112.0),
    Vec2::new(160.0, 96.0),
    Vec2::new(160.0, 112.0),
    Vec2::new(-256.0, 112.0),
];

pub(crate) const fn at(column: usize, row: usize) -> Vec2 {
    Vec2::new((column as f32 - 10.0) * 32.0, (5.0 - row as f32) * 32.0)
}
