use std::num::NonZeroU32;

use nara::{
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, RuntimeFrameStatus},
    ecs::{With, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    input::{ButtonDriverInput, InputSeatId, InputTimeline, InputWindowId, KeyCode},
    prelude::{
        App, CoreStage, FixedTime, FixedUpdateSet, Plugin, Query, Res, ResMut, Sprite, Transform2d,
    },
    project_host::{HeadlessProductRun, HeadlessRunOutcome},
};

const WINDOW: InputWindowId = InputWindowId::new(7);
const SEAT: InputSeatId = InputSeatId::PRIMARY;

fn main() {
    let recipe = nara_game::product_recipe()
        .unwrap()
        .add_plugin::<ObserveMovement>()
        .unwrap();
    let report = HeadlessProductRun::<RuntimeFrameStatus>::from_recipe(
        env!("CARGO_MANIFEST_DIR"),
        recipe,
        NonZeroU32::new(6).unwrap(),
        Vec::new(),
    )
    .run()
    .unwrap();
    assert!(
        matches!(report.outcome(), HeadlessRunOutcome::Completed(outcome) if outcome.fixed_tick == 6),
        "{:#?}",
        report.diagnostics()
    );
    assert!(!report.diagnostics().has_errors());
    println!(
        "STARTER_MOVEMENT_OK continuous_ticks=2,3 held_then_released=true confirm_resets=true"
    );
}

#[derive(Default)]
struct ObserveMovement;

impl Plugin for ObserveMovement {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("push-box.observe-starter-movement"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            observe.after(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

fn observe(
    time: Res<FixedTime>,
    sprites: Query<&Transform2d, With<Sprite>>,
    mut timeline: ResMut<InputTimeline>,
) -> Result<(), BevyError> {
    let (expected_x, next) = match time.tick() {
        1 => {
            timeline.connect_endpoint(WINDOW, SEAT)?;
            (0.0, Some(ButtonDriverInput::Press(KeyCode::Character('d'))))
        }
        2 => (4.8, None),
        3 => (
            9.6,
            Some(ButtonDriverInput::Release(KeyCode::Character('d'))),
        ),
        4 => (9.6, Some(ButtonDriverInput::Press(KeyCode::Enter))),
        5 => (0.0, Some(ButtonDriverInput::Release(KeyCode::Enter))),
        6 => (0.0, None),
        _ => return Err("movement probe exceeded its fixed tick bound".into()),
    };
    let translation = sprites.single()?.translation;
    if (translation.x - expected_x).abs() > 1e-4 || translation.y != 0.0 {
        return Err(format!(
            "tick {} expected ({expected_x}, 0), observed {translation}",
            time.tick()
        )
        .into());
    }
    if let Some(input) = next
        && timeline.admit_keyboard(WINDOW, SEAT, input)?.admitted() != 1
    {
        return Err("starter probe must admit one edge".into());
    }
    Ok(())
}
