use nara_app::{App, PluginError, RealTime, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter};
use nara_ecs::{
    Res, ResMut, Resource, bevy_ecs::system::ScheduleSystem, schedule::IntoScheduleConfigs,
};

use crate::{
    GameplayCommandDraft, GameplayCommandQueue, LocalGameplayCommands,
    schedule::GameplayCommandInternalSet,
};

#[derive(Debug, Default, Resource)]
pub(super) struct LocalCommandFrameCursor {
    frame: Option<u64>,
}

pub(super) fn add_systems<M>(
    app: &mut App,
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
) -> Result<&mut App, PluginError> {
    app.add_systems(
        nara_app::__private::FrameIntent,
        systems.in_set(GameplayCommandInternalSet::CollectLocalCommands),
    )
}

pub(super) fn is_new_frame(time: Res<RealTime>, cursor: Res<LocalCommandFrameCursor>) -> bool {
    cursor.frame != Some(time.frame)
}

pub(super) fn submit_collected_commands(
    time: Res<RealTime>,
    mut cursor: ResMut<LocalCommandFrameCursor>,
    mut local_commands: ResMut<LocalGameplayCommands>,
    mut queue: ResMut<GameplayCommandQueue>,
    faults: Res<RuntimeFaultReporter>,
) {
    for command in local_commands.drain() {
        submit_local_command(&mut queue, command, &faults);
    }
    cursor.frame = Some(time.frame);
}

pub(super) fn submit_local_command(
    queue: &mut GameplayCommandQueue,
    command: GameplayCommandDraft,
    faults: &RuntimeFaultReporter,
) {
    if queue.submit_local_for_next_tick(command).is_err() {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::LocalIntentLoss,
            "nara.gameplay.local-action",
        ));
    }
}

#[cfg(test)]
mod tests;
