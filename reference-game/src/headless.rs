//! Deterministic exercises driven through the same plugin and fixed schedule as desktop play.

use std::{error::Error, time::Duration};

use nara::prelude::*;

use crate::{Failure, GamePlugin, PilotInput, Round, RoundStatus, RunRules, arena};

type GameResult<T> = Result<T, Box<dyn Error>>;
const STEP: Duration = FixedTime::DEFAULT_TIMESTEP;
pub const MAX_ROUNDS: u32 = 200;

#[derive(Debug, PartialEq, Eq)]
pub struct StressReport {
    pub rounds: u32,
    pub fixed_ticks: u64,
    pub wall_contacts_per_round: u32,
    pub escape_time: Duration,
}

pub fn run_stress(rounds: u32) -> GameResult<StressReport> {
    if !(1..=MAX_ROUNDS).contains(&rounds) {
        return Err(format!("choose 1–{MAX_ROUNDS} stress rounds").into());
    }
    let mut app = new_app(RunRules::default())?;
    let baseline = exercise_round(&mut app)?;
    for _ in 1..rounds {
        if exercise_round(&mut app)? != baseline {
            return Err("the fixed-step replay diverged".into());
        }
    }
    let report = StressReport {
        rounds,
        fixed_ticks: app.world().resource::<FixedTime>().tick(),
        wall_contacts_per_round: baseline.0,
        escape_time: baseline.1.elapsed,
    };
    app.finish()
        .map_err(|_| "could not retire the headless game")?;
    Ok(report)
}

fn new_app(rules: RunRules) -> GameResult<App> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless())?
        .register_schema::<RunRules>()?
        .add_plugins(GamePlugin(rules))?;
    Ok(app)
}

fn exercise_round(app: &mut App) -> GameResult<(u32, Round)> {
    restart(app)?;
    for _ in 0..40 {
        step(app, Vec2::NEG_X)?;
    }
    let round = app.world().resource::<Round>();
    let contacts = round.wall_contacts();
    if contacts == 0 || round.position().x < arena::at(1, 2).x - 6.1 {
        return Err("the west wall did not stop the pilot".into());
    }

    restart(app)?;
    follow(app, &[(2, 5), (8, 5), (8, 3), (11, 3)])?;
    for _ in 0..600 {
        if app.world().resource::<Round>().status() != RoundStatus::Running {
            break;
        }
        step(app, Vec2::ZERO)?;
    }
    if app.world().resource::<Round>().status() != RoundStatus::Failed(Failure::ShieldDepleted) {
        return Err("the patrol did not deplete the pilot's shields".into());
    }

    restart(app)?;
    follow(
        app,
        &[
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
        ],
    )?;
    let round = app.world().resource::<Round>();
    if round.status() != RoundStatus::Escaped || round.collected() != arena::CELLS.len() {
        return Err("the pilot could not power the gate and escape".into());
    }
    Ok((contacts, round.clone()))
}

fn restart(app: &mut App) -> GameResult<()> {
    *app.world_mut()?.resource_mut::<PilotInput>() = PilotInput {
        movement: Vec2::ZERO,
        restart: true,
    };
    app.step_fixed(STEP)?;
    let round = app.world().resource::<Round>();
    if round.status() != RoundStatus::Running
        || round.position() != arena::START
        || round.collected() != 0
        || !round.elapsed.is_zero()
    {
        return Err("restart did not restore the starting round".into());
    }
    Ok(())
}

fn step(app: &mut App, movement: Vec2) -> GameResult<()> {
    app.world_mut()?.resource_mut::<PilotInput>().movement = movement;
    app.step_fixed(STEP)?;
    Ok(())
}

fn follow(app: &mut App, route: &[(usize, usize)]) -> GameResult<()> {
    let stride = RunRules::default().move_speed * STEP.as_secs_f32();
    for &(column, row) in route {
        let target = arena::at(column, row);
        for _ in 0..240 {
            let round = app.world().resource::<Round>();
            if round.status() == RoundStatus::Escaped || round.position().distance(target) < 0.1 {
                break;
            }
            let movement = (target - round.position()) / stride;
            step(app, movement)?;
        }
        let round = app.world().resource::<Round>();
        if round.status() != RoundStatus::Escaped && round.position().distance(target) >= 0.1 {
            return Err(format!("the pilot did not reach arena tile ({column}, {row})").into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_replay_checks_collision_shield_loss_restart_and_escape() -> GameResult<()> {
        let report = run_stress(2)?;
        assert_eq!(report.rounds, 2);
        assert!(report.wall_contacts_per_round > 0);
        assert!(report.escape_time < Duration::from_secs(38));
        Ok(())
    }

    #[test]
    fn time_expiry_freezes_the_round_until_restart() -> GameResult<()> {
        let mut app = new_app(RunRules {
            time_limit_seconds: 1,
            ..Default::default()
        })?;
        for _ in 0..60 {
            step(&mut app, Vec2::ZERO)?;
        }
        let stopped = app.world().resource::<Round>().clone();
        assert_eq!(stopped.status(), RoundStatus::Failed(Failure::TimeExpired));
        step(&mut app, Vec2::X)?;
        assert_eq!(app.world().resource::<Round>(), &stopped);
        restart(&mut app)?;
        step(&mut app, Vec2::X)?;
        let round = app.world().resource::<Round>();
        assert!(round.position().x > arena::START.x);
        assert_eq!(round.shields(), RunRules::default().shield_charges);
        Ok(())
    }

    #[test]
    fn reaching_an_unpowered_gate_keeps_the_round_running() -> GameResult<()> {
        let mut app = new_app(RunRules::default())?;
        follow(
            &mut app,
            &[(2, 5), (8, 5), (8, 4), (19, 4), (19, 9), (17, 9), (17, 8)],
        )?;
        let round = app.world().resource::<Round>();
        assert!(round.position().distance(arena::GATE) < 0.1);
        assert_eq!(round.collected(), 0);
        assert_eq!(round.status(), RoundStatus::Running);
        Ok(())
    }
}
