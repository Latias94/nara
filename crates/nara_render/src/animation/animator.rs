use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, FrameCleanup, Plugin, PluginCategory,
    PluginDeclaration, PluginError, PluginId, VirtualTime,
};
use nara_asset::{ASSET_PLUGIN_ID, AssetAppExt, Assets, Handle};
use nara_ecs::{
    Component, Entity, IntoScheduleConfigs, Query, Res, ResMut, SystemSet,
    bevy_ecs::message::{Message, Messages},
};
use thiserror::Error;

use super::{Clip, FrameSample, SpriteAnimationLoader, SpriteAnimationSet};
use crate::sprite::Sprite;

/// Time source and schedule for a [`SpriteAnimator`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SpriteAnimationClock {
    /// Advance once in `Update` using [`VirtualTime::delta`].
    #[default]
    Virtual,
    /// Advance once per tick in `FixedUpdate` using [`FixedTime::delta`].
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackState {
    Playing,
    Paused,
    Completed,
}

/// Samples a named sprite animation and applies its region to the same entity's [`Sprite`].
///
/// Playback waits for the sprite and the requested clip to be available. A paused or completed
/// animator holds its elapsed time. Looping and ping-pong elapsed time accumulates across cycles
/// up to [`Duration::MAX`].
#[derive(Debug, Clone, Component)]
pub struct SpriteAnimator {
    set: Handle<SpriteAnimationSet>,
    clip: String,
    elapsed: Option<Duration>,
    speed: f32,
    fractional_nanos: f64,
    state: PlaybackState,
    clock: SpriteAnimationClock,
    #[cfg(feature = "animation-scene")]
    pub(super) generated_by_source: bool,
}

impl SpriteAnimator {
    /// Starts the named clip from zero with virtual time at normal speed.
    #[must_use]
    pub fn new(set: Handle<SpriteAnimationSet>, clip: impl Into<String>) -> Self {
        Self {
            set,
            clip: clip.into(),
            elapsed: None,
            speed: 1.0,
            fractional_nanos: 0.0,
            state: PlaybackState::Playing,
            clock: SpriteAnimationClock::Virtual,
            #[cfg(feature = "animation-scene")]
            generated_by_source: false,
        }
    }

    #[must_use]
    pub const fn with_clock(mut self, clock: SpriteAnimationClock) -> Self {
        self.clock = clock;
        self
    }

    #[must_use]
    pub const fn clock(&self) -> SpriteAnimationClock {
        self.clock
    }

    #[must_use]
    pub const fn set(&self) -> Handle<SpriteAnimationSet> {
        self.set
    }

    #[must_use]
    pub fn clip(&self) -> &str {
        &self.clip
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed.unwrap_or_default()
    }

    #[must_use]
    pub const fn speed(&self) -> f32 {
        self.speed
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }

    /// Selects a clip and starts playback. Selecting the current clip resumes it if paused
    /// and starts it over if completed. Use [`Self::restart`] to reset an active clip.
    pub fn play(&mut self, clip: impl AsRef<str>) {
        let clip = clip.as_ref();
        if self.clip != clip {
            self.clip.clear();
            self.clip.push_str(clip);
            self.restart();
        } else if self.state == PlaybackState::Completed {
            self.restart();
        } else {
            self.resume();
        }
    }

    /// Starts the current clip from zero, including its first frame's marker.
    pub fn restart(&mut self) {
        self.elapsed = None;
        self.fractional_nanos = 0.0;
        self.state = PlaybackState::Playing;
    }

    /// Holds the current playback time.
    pub fn pause(&mut self) {
        if self.is_playing() {
            self.state = PlaybackState::Paused;
        }
    }

    /// Continues paused playback. Completed clips can be started again with [`Self::play`].
    pub fn resume(&mut self) {
        if self.state == PlaybackState::Paused {
            self.state = PlaybackState::Playing;
        }
    }

    /// Sets a finite, nonnegative scale for subsequent time deltas.
    pub fn set_speed(&mut self, speed: f32) -> Result<(), SpriteAnimationSpeedError> {
        if !speed.is_finite() || speed < 0.0 {
            return Err(SpriteAnimationSpeedError(speed));
        }
        self.speed = speed;
        Ok(())
    }

    #[cfg(feature = "animation-scene")]
    pub(super) fn apply_source(&mut self, set: Handle<SpriteAnimationSet>, clip: &str) {
        if self.set != set {
            self.set = set;
            self.clip.clear();
            self.clip.push_str(clip);
            self.restart();
        } else if self.clip != clip {
            self.play(clip);
        }
    }

    fn advance(
        &mut self,
        clip: &Clip,
        entity: Entity,
        delta: Duration,
        events: &mut Messages<SpriteAnimationEvent>,
    ) -> FrameSample {
        let previous = self.elapsed;
        let elapsed = self.elapsed().saturating_add(self.scale_delta(delta));
        let sample = clip.sample(elapsed);
        let elapsed = if sample.finished {
            clip.duration()
        } else {
            elapsed
        };
        clip.visit_markers(previous, elapsed, |marker| {
            events.write(SpriteAnimationEvent::FrameMarker {
                entity,
                clip: self.clip.clone(),
                marker: marker.to_owned(),
            });
        });
        self.elapsed = Some(elapsed);
        if sample.finished {
            self.state = PlaybackState::Completed;
            events.write(SpriteAnimationEvent::Completed {
                entity,
                clip: self.clip.clone(),
            });
        }
        sample
    }

    fn scale_delta(&mut self, delta: Duration) -> Duration {
        if self.speed == 0.0 || delta.is_zero() {
            return Duration::ZERO;
        }
        if self.speed == 1.0 {
            return delta;
        }
        let scaled = delta.as_nanos() as f64 * f64::from(self.speed) + self.fractional_nanos;
        self.fractional_nanos = scaled.fract();
        let nanos = (scaled as u128).min(Duration::MAX.as_nanos());
        Duration::new(
            (nanos / 1_000_000_000) as u64,
            (nanos % 1_000_000_000) as u32,
        )
    }
}

/// An invalid sprite animation playback speed.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[error("sprite animation speed must be finite and nonnegative, got {0}")]
pub struct SpriteAnimationSpeedError(pub f32);

/// A marker crossed or a clip completed by a [`SpriteAnimator`].
///
/// Markers are sent in playback order on entering a frame, including the initial frame and
/// cycle wraps. Each step sends up to [`super::MAX_MARKERS_PER_STEP`] markers per animator;
/// the elapsed time and sampled frame include the entire step. `Once` sends one completion after
/// its final frame's duration, after that step's markers. Read messages after
/// [`SpriteAnimationUpdateSet`] within the animator's schedule, or in a later schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteAnimationEvent {
    FrameMarker {
        entity: Entity,
        clip: String,
        marker: String,
    },
    Completed {
        entity: Entity,
        clip: String,
    },
}

impl Message for SpriteAnimationEvent {}

/// Advances animators and writes sprite regions and messages in `Update` and `FixedUpdate`.
/// In `FixedUpdate`, this set runs in [`FixedUpdateSet::Finalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct SpriteAnimationUpdateSet;

/// Registers sprite animation assets and updates sprites before extraction.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteAnimationPlugin;

/// Stable ID for the sprite animation plugin in Nara plugin groups.
pub const SPRITE_ANIMATION_PLUGIN_ID: PluginId = PluginId::new("nara.sprite-animation");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SPRITE_ANIMATION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[ASSET_PLUGIN_ID]);

impl Plugin for SpriteAnimationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_asset_loader(SpriteAnimationLoader)?
            .init_resource::<Messages<SpriteAnimationEvent>>()?
            .configure_sets(
                CoreStage::FixedUpdate,
                SpriteAnimationUpdateSet.in_set(FixedUpdateSet::Finalize),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                advance_fixed.in_set(SpriteAnimationUpdateSet),
            )?
            .add_systems(
                CoreStage::Update,
                advance_virtual.in_set(SpriteAnimationUpdateSet),
            )?
            .add_systems(
                FrameCleanup,
                |mut events: ResMut<Messages<SpriteAnimationEvent>>| events.update(),
            )?;
        #[cfg(feature = "animation-scene")]
        super::hydration::install(app)?;
        Ok(())
    }
}

type AnimatedSprites<'w, 's> =
    Query<'w, 's, (Entity, &'static mut SpriteAnimator, &'static mut Sprite)>;

fn advance_virtual(
    time: Res<VirtualTime>,
    assets: Res<Assets<SpriteAnimationSet>>,
    sprites: AnimatedSprites,
    mut events: ResMut<Messages<SpriteAnimationEvent>>,
) {
    advance_animations(
        SpriteAnimationClock::Virtual,
        time.delta,
        &assets,
        sprites,
        &mut events,
    );
}

fn advance_fixed(
    time: Res<FixedTime>,
    assets: Res<Assets<SpriteAnimationSet>>,
    sprites: AnimatedSprites,
    mut events: ResMut<Messages<SpriteAnimationEvent>>,
) {
    advance_animations(
        SpriteAnimationClock::Fixed,
        time.delta(),
        &assets,
        sprites,
        &mut events,
    );
}

fn advance_animations(
    clock: SpriteAnimationClock,
    delta: Duration,
    assets: &Assets<SpriteAnimationSet>,
    mut sprites: AnimatedSprites,
    events: &mut Messages<SpriteAnimationEvent>,
) {
    for (entity, mut animator, mut sprite) in &mut sprites {
        if animator.clock != clock {
            continue;
        }
        let Some(clip) = assets
            .get(animator.set)
            .and_then(|set| set.get(&animator.clip))
        else {
            continue;
        };
        let sample = if animator.is_playing() {
            animator.advance(clip, entity, delta, events)
        } else {
            clip.sample(animator.elapsed())
        };
        if sprite.texture_region != Some(sample.region) {
            sprite.texture_region = Some(sample.region);
        }
    }
}
