use std::path::Path;

use nara::{
    app::FixedTime,
    ecs::World,
    gameplay::{
        GameplayCommandBatch, GameplayCommandIngressSource, GameplayCommandPayload,
        GameplayCommandSource, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick, GameplayCommandValue,
    },
};

use super::{
    GamePhase, RunOptions, Scenario, Session,
    commands::{PLAYER_LAUNCH, action},
    storage::Storage,
    verification::Verification,
};

const SOURCE: &str = "two-level-game.player-launch";
const VERSION: i64 = 1;
const MAX_SAVE_PATH_BYTES: usize = 4096;

/// Encodes finite Player launch intent for the original gameplay ingress at tick one.
///
/// The executable and cooker use the same fixed placement-specific cooked recipe. Save paths
/// must be explicit, absolute UTF-8 paths; launch intent never changes canonical configuration.
pub fn player_launch(options: RunOptions) -> Result<Option<GameplayCommandSubmission>, String> {
    validate(&options)?;
    if options.scenario == Scenario::Interactive && options.save_path.is_none() {
        return Ok(None);
    }
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("version", GameplayCommandValue::I64(VERSION))
        .map_err(|error| error.to_string())?;
    let scenario = match options.scenario {
        Scenario::Interactive => "interactive",
        Scenario::Travel => "travel",
        Scenario::Save => "save",
        Scenario::Restore => "restore",
    };
    payload
        .insert("scenario", GameplayCommandValue::String(scenario.into()))
        .map_err(|error| error.to_string())?;
    if let Some(path) = options.save_path {
        let path = path.to_str().ok_or("Player save path must be UTF-8")?;
        payload
            .insert("save_path", GameplayCommandValue::String(path.into()))
            .map_err(|error| error.to_string())?;
    }
    Ok(Some(GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).expect("the first fixed tick is nonzero"),
        GameplayCommandIngressSource::external(SOURCE).expect("static Player launch source"),
        GameplayCommandSourceSequence::new(1).expect("the first source sequence is nonzero"),
        action(PLAYER_LAUNCH).with_payload(payload),
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

fn decode(payload: &GameplayCommandPayload) -> Result<RunOptions, String> {
    if payload
        .iter()
        .any(|(key, _)| !matches!(key.as_str(), "version" | "scenario" | "save_path"))
        || payload.get("version") != Some(&GameplayCommandValue::I64(VERSION))
    {
        return Err("Player launch requires its exact versioned payload fields".into());
    }
    let scenario = match payload.get("scenario") {
        Some(GameplayCommandValue::String(value)) => match value.as_str() {
            "interactive" => Scenario::Interactive,
            "travel" => Scenario::Travel,
            "save" => Scenario::Save,
            "restore" => Scenario::Restore,
            _ => return Err("Player launch scenario is unsupported".into()),
        },
        _ => return Err("Player launch requires a scenario string".into()),
    };
    let save_path = match payload.get("save_path") {
        None => None,
        Some(GameplayCommandValue::String(path)) => Some(Path::new(path).to_path_buf()),
        _ => return Err("Player launch save path must be a string".into()),
    };
    let options = RunOptions {
        scenario,
        save_path,
    };
    validate(&options)?;
    if scenario == Scenario::Interactive && options.save_path.is_none() {
        return Err("interactive Player launch without a save path requires no command".into());
    }
    Ok(options)
}

pub(super) fn consume(world: &mut World) -> Result<(), String> {
    if !world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .any(|command| command.command_type().as_str() == PLAYER_LAUNCH)
    {
        return Ok(());
    }
    let result = apply(world);
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

fn apply(world: &mut World) -> Result<(), String> {
    let batch = world.resource::<GameplayCommandBatch>();
    let mut launches = batch
        .commands()
        .iter()
        .filter(|command| command.command_type().as_str() == PLAYER_LAUNCH);
    let Some(command) = launches.next() else {
        return Ok(());
    };
    if launches.next().is_some() {
        return Err("Player launch must occur exactly once in its first-tick batch".into());
    }
    if command.tick().get() != 1
        || command.source_sequence().get() != 1
        || !matches!(
            command.source(),
            GameplayCommandSource::External { producer } if producer.as_str() == SOURCE
        )
        || command.target().is_some()
    {
        return Err("Player launch requires its dedicated first-tick ingress and sequence".into());
    }
    let options = decode(command.payload())?;
    if world.resource::<FixedTime>().tick() != 1
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<Session>().phase != GamePhase::Running
        || !world.resource::<Verification>().is_interactive()
        || !world.resource::<Storage>().is_unconfigured()
        || batch
            .commands()
            .iter()
            .any(|command| command.command_type().as_str() == super::resume::RESTORE_STATE)
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
    *world.resource_mut::<Verification>() = Verification::new(options.scenario);
    Ok(())
}

#[cfg(all(test, feature = "local-player"))]
mod tests;
