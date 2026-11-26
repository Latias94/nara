use nara::prelude::Vec2;

pub(crate) const TILE: f32 = 32.0;
pub(crate) const WIDTH: usize = 21;
pub(crate) const HEIGHT: usize = 11;
pub(crate) const WALLS: [&[u8; WIDTH]; HEIGHT] = [
    b"#####################",
    b"#.....#.......#.....#",
    b"#.....#.......#.....#",
    b"#.....#.......#.....#",
    b"#.....#.............#",
    b"#.........###...#...#",
    b"#..####.........#...#",
    b"#...........#.......#",
    b"#...........#.......#",
    b"#...................#",
    b"#####################",
];

pub(crate) const START: Vec2 = at(2, 2);
pub(crate) const CELLS: [Vec2; 3] = [at(9, 2), at(17, 2), at(3, 8)];
pub(crate) const GATE: Vec2 = at(17, 8);
pub(crate) const DRONE_RADIUS: f32 = 11.0;
pub(crate) const PATROLS: [Patrol; 3] = [
    Patrol::new(at(9, 3), at(13, 3), 4.0, 0.0),
    Patrol::new(at(3, 7), at(10, 7), 6.0, 0.35),
    Patrol::new(at(18, 5), at(18, 8), 3.6, 0.85),
];

pub(crate) const fn at(column: usize, row: usize) -> Vec2 {
    Vec2::new(
        (column as f32 - (WIDTH - 1) as f32 * 0.5) * TILE,
        ((HEIGHT - 1) as f32 * 0.5 - row as f32) * TILE,
    )
}

pub(crate) fn tiles() -> impl Iterator<Item = (usize, usize, bool)> {
    WALLS.iter().enumerate().flat_map(|(row, cells)| {
        cells
            .iter()
            .enumerate()
            .map(move |(column, &cell)| (column, row, cell == b'#'))
    })
}

#[derive(Clone, Copy)]
pub(crate) struct Patrol {
    pub(crate) start: Vec2,
    pub(crate) end: Vec2,
    period: f32,
    phase: f32,
}

impl Patrol {
    const fn new(start: Vec2, end: Vec2, period: f32, phase: f32) -> Self {
        Self {
            start,
            end,
            period,
            phase,
        }
    }

    pub(crate) fn position(self, elapsed_seconds: f32) -> Vec2 {
        let phase = (elapsed_seconds / self.period + self.phase).fract();
        let fraction = 1.0 - (2.0 * phase - 1.0).abs();
        self.start.lerp(self.end, fraction)
    }
}
