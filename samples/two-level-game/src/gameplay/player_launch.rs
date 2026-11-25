use std::num::NonZeroU64;

use nara::{app::FixedTime, ecs::World};

use super::{
    GamePhase, RunOptions, Scenario, Session, storage::Storage, verification::Verification,
};
use crate::command::{CommandSchedule, CommandSource, GameCommand, ScheduledCommand};

const SOURCE: &str = "two-level-game.player-launch";
const MAX_SAVE_PATH_BYTES: usize = 4096;

/// Encodes finite Player launch intent for the original gameplay ingress at tick one.
///
/// The executable and cooker use the same fixed placement-specific cooked recipe. Save paths
/// must be explicit, absolute UTF-8 paths; launch intent never changes canonical configuration.
pub fn player_launch(options: RunOptions) -> Result<Option<ScheduledCommand>, String> {
    validate(&options)?;
    if options.scenario == Scenario::Interactive && options.save_path.is_none() {
        return Ok(None);
    }
    Ok(Some(ScheduledCommand::new(
        NonZeroU64::MIN,
        CommandSource::external(SOURCE).expect("static Player launch source"),
        NonZeroU64::MIN,
        GameCommand::Launch(options),
    )))
}

fn validate(options: &RunOptions) -> Result<(), String> {
    if matches!(options.scenario, Scenario::Save | Scenario::Restore) && options.save_path.is_none()
    {
        return Err("Player Save and Restore require an explicit application save path".into());
    }
    if let Some(path) = options.save_path.as_deref() {
        let text = path.to_str().ok_or("Player save path must be UTF-8")?;
        if text.len() > MAX_SAVE_PATH_BYTES
            || text.contains('\0')
            || !path.is_absolute()
            || path.file_name().is_none()
        {
            return Err(format!(
                "Player save path must be absolute with a file name and at most {MAX_SAVE_PATH_BYTES} UTF-8 bytes"
            ));
        }
    }
    Ok(())
}

pub(super) fn consume(world: &mut World) -> Result<(), String> {
    let result = match apply(world) {
        Ok(false) => return Ok(()),
        Ok(true) => Ok(()),
        Err(error) => Err(error),
    };
    let mut session = world.resource_mut::<Session>();
    if session.player_launch.is_none() {
        session.player_launch = Some(result.clone());
    }
    result
}

/// Observes the first Player launch consumed by the original gameplay owner.
///
/// This diagnostic survives later ordinary action errors and is not mutation authority or
/// authoritative game state for exact-state reconstruction. `None` means no launch was consumed.
#[must_use]
pub fn player_launch_result(world: &World) -> Option<Result<(), &str>> {
    world
        .get_resource::<Session>()?
        .player_launch
        .as_ref()
        .map(|result| result.as_ref().copied().map_err(String::as_str))
}

fn apply(world: &mut World) -> Result<bool, String> {
    let batch = world.resource::<CommandSchedule>().current();
    let mut launches = batch.iter().filter_map(|command| match command.intent() {
        GameCommand::Launch(options) => Some((command, options)),
        _ => None,
    });
    let Some((command, options)) = launches.next() else {
        return Ok(false);
    };
    if launches.next().is_some() {
        return Err("Player launch must occur exactly once in its first-tick batch".into());
    }
    if command.tick().get() != 1
        || command.source_sequence().get() != 1
        || !command.source().is_external(SOURCE)
    {
        return Err("Player launch requires its dedicated first-tick ingress and sequence".into());
    }
    validate(options)?;
    if options.scenario == Scenario::Interactive && options.save_path.is_none() {
        return Err("interactive Player launch without a save path requires no command".into());
    }
    if world.resource::<FixedTime>().tick() != 1
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<Session>().phase != GamePhase::Running
        || !world.resource::<Verification>().is_interactive()
        || !world.resource::<Storage>().is_unconfigured()
        || batch
            .iter()
            .any(|command| matches!(command.intent(), GameCommand::Resume(_)))
    {
        return Err(
            "Player launch requires a fresh interactive game without a save slot or exact-state resume"
                .into(),
        );
    }
    let served =
        !world.resource::<Session>().local_player && options.scenario == Scenario::Interactive;
    world
        .resource::<Storage>()
        .open_slot(options.save_path.as_deref(), served)?;
    let scenario = options.scenario;
    *world.resource_mut::<Verification>() = Verification::new(scenario);
    Ok(true)
}

#[cfg(all(test, feature = "local-player"))]
mod tests;
