use nara_app::{App, PluginError, RealTime};
use nara_ecs::{Res, bevy_ecs::system::ScheduleSystem, schedule::IntoScheduleConfigs};

use crate::{ActionCommandLoweringCursor, schedule::GameplayCommandInternalSet};

pub(super) fn add_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(
        nara_app::__private::FrameIntent,
        systems.in_set(GameplayCommandInternalSet::CollectLocalCommands),
    )
}

pub(super) fn needs_collection(
    time: Res<RealTime>,
    cursor: Res<ActionCommandLoweringCursor>,
) -> bool {
    cursor.frame != Some(time.frame)
}

#[cfg(test)]
mod tests;
