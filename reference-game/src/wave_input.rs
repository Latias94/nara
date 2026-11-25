use std::{collections::BTreeMap, num::NonZeroU64};

use nara::{
    ProductConfiguration, ProductRecipe, ProductRecipeError,
    app::{Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, StartupStage},
    ecs::{Command, Res, ResMut, Resource, World, bevy_ecs::error::BevyError},
    prelude::{App, FixedTime},
};

use crate::{MovementDirection, REFERENCE_WAVE_PLUGIN_ID};

const MAX_PENDING_INPUTS: usize = 4_096;
const MAX_FUTURE_TICKS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaveIntent {
    Move {
        direction: MovementDirection,
        pressed: bool,
    },
    Retry {
        ui_sequence: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveCommand {
    tick: NonZeroU64,
    sequence: NonZeroU64,
    intent: WaveIntent,
}

impl WaveCommand {
    pub(crate) const fn new(tick: NonZeroU64, sequence: NonZeroU64, intent: WaveIntent) -> Self {
        Self {
            tick,
            sequence,
            intent,
        }
    }
}

impl Command for WaveCommand {
    type Out = Result<(), BevyError>;

    fn apply(self, world: &mut World) -> Self::Out {
        let tick = world.resource::<FixedTime>().tick();
        let mut input = world
            .get_resource_mut::<WaveInput>()
            .ok_or_else(|| BevyError::error("wave input is not installed"))?;
        input.schedule(tick, self)
    }
}

#[derive(Default, Resource)]
pub(crate) struct WaveInput {
    local: Vec<WaveIntent>,
    scripted: BTreeMap<(u64, NonZeroU64), WaveIntent>,
    current: Vec<WaveIntent>,
}

impl WaveInput {
    fn schedule(&mut self, current_tick: u64, command: WaveCommand) -> Result<(), BevyError> {
        let key = (command.tick.get(), command.sequence);
        if key.0 <= current_tick {
            return Err(BevyError::error("wave input targets a completed tick"));
        }
        if key.0 - current_tick > MAX_FUTURE_TICKS {
            return Err(BevyError::error(
                "wave input exceeds the future tick horizon",
            ));
        }
        if self.scripted.contains_key(&key) {
            return Err(BevyError::error(
                "wave input is already scheduled at this tick and sequence",
            ));
        }
        self.require_space()?;
        self.scripted.insert(key, command.intent);
        Ok(())
    }

    pub(crate) fn push(&mut self, intent: WaveIntent) -> Result<(), BevyError> {
        self.require_space()?;
        self.local.push(intent);
        Ok(())
    }

    fn require_space(&self) -> Result<(), BevyError> {
        if self.local.len() + self.scripted.len() == MAX_PENDING_INPUTS {
            return Err(BevyError::error("too many pending wave inputs"));
        }
        Ok(())
    }

    pub(crate) fn current(&self) -> &[WaveIntent] {
        &self.current
    }

    pub(crate) fn retry_requests(&self) -> impl Iterator<Item = Option<u64>> + '_ {
        self.current.iter().filter_map(|intent| match *intent {
            WaveIntent::Retry { ui_sequence } => Some(ui_sequence),
            WaveIntent::Move { .. } => None,
        })
    }
}

pub(crate) fn prepare_wave_input(time: Res<FixedTime>, mut input: ResMut<WaveInput>) {
    let input = &mut *input;
    input.current.clear();
    std::mem::swap(&mut input.current, &mut input.local);
    while let Some((&(tick, _), _)) = input.scripted.first_key_value() {
        if tick > time.tick() {
            break;
        }
        let (_, intent) = input
            .scripted
            .pop_first()
            .expect("script has a first entry");
        input.current.push(intent);
    }
}

pub(crate) fn finish_wave_input(mut input: ResMut<WaveInput>) {
    input.current.clear();
}

/// Counts typed Retry requests in the current fixed tick, for `ReferenceWaveSet::Capture` observers.
pub fn wave_retry_request_count(world: &World) -> usize {
    world.resource::<WaveInput>().retry_requests().count()
}

#[derive(Clone)]
struct WaveScript(Vec<WaveCommand>);

impl ProductConfiguration for WaveScript {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        for command in &self.0 {
            output.extend_from_slice(&command.tick.get().to_le_bytes());
            output.extend_from_slice(&command.sequence.get().to_le_bytes());
            match command.intent {
                WaveIntent::Move { direction, pressed } => {
                    output.extend_from_slice(&[0, direction as u8, u8::from(pressed)]);
                }
                WaveIntent::Retry { ui_sequence } => {
                    output.extend_from_slice(&[1, u8::from(ui_sequence.is_some())]);
                    output.extend_from_slice(&ui_sequence.unwrap_or_default().to_le_bytes());
                }
            }
        }
    }
}

struct WaveScriptPlugin(WaveScript);

impl Plugin for WaveScriptPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("reference-game.wave-script"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[REFERENCE_WAVE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let script = self.0.clone();
        app.add_systems(
            StartupStage::Runtime,
            move |mut input: ResMut<WaveInput>| -> Result<(), BevyError> {
                for command in &script.0 {
                    input.schedule(0, *command)?;
                }
                Ok(())
            },
        )?;
        Ok(())
    }
}

pub fn with_wave_script(
    recipe: ProductRecipe,
    commands: Vec<WaveCommand>,
) -> Result<ProductRecipe, ProductRecipeError> {
    if commands.is_empty() {
        return Ok(recipe);
    }
    recipe.add_configured_plugin(WaveScript(commands), |script| {
        WaveScriptPlugin(script.clone())
    })
}
