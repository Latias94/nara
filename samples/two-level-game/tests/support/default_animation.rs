//! Independent expectation for the committed default explorer sequence, not a resource loader.

use two_level_game::{animation::ExplorerFrame, resume::GameClock};

pub fn expected_default_frame(clock: GameClock, held: [bool; 4]) -> ExplorerFrame {
    if held[0] == held[1] && held[2] == held[3] {
        return ExplorerFrame::Idle;
    }
    match (clock.elapsed_nanos / 100_000_000) % 3 {
        0 => ExplorerFrame::WalkOne,
        1 => ExplorerFrame::WalkTwo,
        _ => ExplorerFrame::WalkThree,
    }
}
