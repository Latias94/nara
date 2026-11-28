//! Fixed-step 2D particles with an emitter-owned pool and sprite-ready quads.

use std::{collections::TryReserveError, mem, ops::RangeInclusive, time::Duration};

use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginId,
};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use nara_ecs::{Component, Query, Res};
use nara_scene::spatial::{GlobalTransform2d, SPATIAL_PLUGIN_ID, SpatialSet, Transform2d};

use crate::{image::ImageAsset, material::AlphaMode2d};

/// Settings for [`ParticleEmitter2d`]. Ranges are sampled independently at birth.
#[derive(Debug, Clone)]
pub struct ParticleEmitterConfig {
    /// Hard limit on live particles. The emitter reserves this many slots at construction.
    pub max_particles: usize,
    /// Continuous emission in particles per second.
    pub rate: f32,
    /// One-shot count queued at construction.
    pub burst: usize,
    /// Positive lifetime in seconds.
    pub lifetime_range: RangeInclusive<f32>,
    /// Nonnegative initial speed in emitter units per second.
    pub speed_range: RangeInclusive<f32>,
    /// Counterclockwise launch angle in radians from the emitter's positive X axis.
    pub angle_range: RangeInclusive<f32>,
    /// Acceleration in simulation-space units per second squared.
    pub gravity: Vec2,
    /// Start and end side lengths of a square quad, interpolated linearly over its life.
    pub size_over_life: [f32; 2],
    /// Start and end linear RGBA colors with straight alpha, interpolated over its life.
    pub color_over_life: [Color; 2],
    /// Optional full-image texture. An omitted texture draws a solid-color quad.
    pub texture: Option<Handle<ImageAsset>>,
    pub alpha_mode: AlphaMode2d,
    /// Local particles follow the emitter's current transform. World particles inherit the
    /// emitter's transform at birth and then move independently in world space.
    pub local_space: bool,
    /// Initial state of this emitter's independent random stream.
    pub seed: u64,
    /// Draw layer, combined with the particle's world Z as on a sprite.
    pub layer: i32,
    /// Explicit sort group within a draw layer.
    pub sort_key: i32,
}

impl Default for ParticleEmitterConfig {
    fn default() -> Self {
        Self {
            max_particles: 128,
            rate: 0.0,
            burst: 0,
            lifetime_range: 0.5..=0.5,
            speed_range: 0.0..=0.0,
            angle_range: 0.0..=std::f32::consts::TAU,
            gravity: Vec2::ZERO,
            size_over_life: [1.0, 1.0],
            color_over_life: [Color::WHITE, Color::rgba(1.0, 1.0, 1.0, 0.0)],
            texture: None,
            alpha_mode: AlphaMode2d::Blend,
            local_space: false,
            seed: 0,
            layer: 0,
            sort_key: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParticleEmitterError {
    #[error("particle capacity must be positive")]
    ZeroCapacity,
    #[error("particle rate must be finite and nonnegative")]
    InvalidRate,
    #[error("particle lifetime bounds must be finite, positive and ordered")]
    InvalidLifetimeRange,
    #[error("particle speed bounds must be finite, nonnegative and ordered")]
    InvalidSpeedRange,
    #[error("particle angle bounds must be finite and ordered")]
    InvalidAngleRange,
    #[error("particle gravity must be finite")]
    InvalidGravity,
    #[error("particle sizes must be finite and nonnegative")]
    InvalidSize,
    #[error("particle colors must be finite with alpha in 0..=1")]
    InvalidColor,
    #[error("could not allocate a pool of {capacity} particles")]
    PoolAllocation {
        capacity: usize,
        #[source]
        source: TryReserveError,
    },
}

/// A sprite-ready particle quad. The world axes are half-extents of the quad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleRenderInstance {
    pub world_center: Vec2,
    pub world_x_axis: Vec2,
    pub world_y_axis: Vec2,
    pub world_z: f32,
    pub color: Color,
    pub texture: Option<Handle<ImageAsset>>,
    pub alpha_mode: AlphaMode2d,
    pub layer: i32,
    pub sort_key: i32,
}

/// Fixed-capacity particle simulation advanced by [`ParticlePlugin`].
///
/// Each fixed tick advances existing particles and frees expired slots before spawning. New
/// particles start at age zero at the published emitter transform for that tick. Whole emissions
/// beyond capacity are dropped. Identical seeds and fixed-step inputs produce the same sequence
/// on the same target. Removing or despawning the emitter drops its entire pool.
#[derive(Debug, Component)]
#[require(Transform2d)]
pub struct ParticleEmitter2d {
    config: ParticleEmitterConfig,
    particles: Vec<Particle>,
    pending_burst: usize,
    rate_remainder: f64,
    random_state: u64,
}

impl ParticleEmitter2d {
    /// Creates the pool and queues the configured initial burst for the first fixed tick.
    ///
    /// Returns an error if the configuration is invalid or the pool cannot be reserved.
    pub fn new(config: ParticleEmitterConfig) -> Result<Self, ParticleEmitterError> {
        if config.max_particles == 0 {
            return Err(ParticleEmitterError::ZeroCapacity);
        }
        validate_rate(config.rate)?;
        if !finite_ordered_range(&config.lifetime_range) || *config.lifetime_range.start() <= 0.0 {
            return Err(ParticleEmitterError::InvalidLifetimeRange);
        }
        if !finite_ordered_range(&config.speed_range) || *config.speed_range.start() < 0.0 {
            return Err(ParticleEmitterError::InvalidSpeedRange);
        }
        if !finite_ordered_range(&config.angle_range) {
            return Err(ParticleEmitterError::InvalidAngleRange);
        }
        if !config.gravity.is_finite() {
            return Err(ParticleEmitterError::InvalidGravity);
        }
        if config
            .size_over_life
            .iter()
            .any(|&size| !size.is_finite() || size < 0.0)
        {
            return Err(ParticleEmitterError::InvalidSize);
        }
        if config.color_over_life.iter().any(|&Color { r, g, b, a }| {
            ![r, g, b].into_iter().all(f32::is_finite) || !(0.0..=1.0).contains(&a)
        }) {
            return Err(ParticleEmitterError::InvalidColor);
        }

        let mut particles = Vec::new();
        particles
            .try_reserve_exact(config.max_particles)
            .map_err(|source| ParticleEmitterError::PoolAllocation {
                capacity: config.max_particles,
                source,
            })?;
        Ok(Self {
            pending_burst: config.burst.min(config.max_particles),
            random_state: config.seed,
            config,
            particles,
            rate_remainder: 0.0,
        })
    }

    #[must_use]
    pub const fn rate(&self) -> f32 {
        self.config.rate
    }

    /// Sets a finite, nonnegative rate in particles per second. Fractional emission credit is
    /// preserved, including when the rate is set to zero.
    pub fn set_rate(&mut self, rate: f32) -> Result<(), ParticleEmitterError> {
        validate_rate(rate)?;
        self.config.rate = rate;
        Ok(())
    }

    /// Queues a burst for the next fixed tick. Multiple calls combine up to the pool limit.
    pub fn emit(&mut self, count: usize) {
        self.pending_burst = self
            .pending_burst
            .saturating_add(count)
            .min(self.config.max_particles);
    }

    /// Iterates live sprite-ready quads in birth order without allocating.
    pub fn render_instances<'a>(
        &'a self,
        global: &'a GlobalTransform2d,
    ) -> impl ExactSizeIterator<Item = ParticleRenderInstance> + 'a {
        self.particles.iter().map(move |particle| {
            let (center, transform) = if self.config.local_space {
                (global.affine().transform_point2(particle.position), global)
            } else {
                (particle.position, &particle.spawn_transform)
            };
            let progress = f64::from(particle.age.as_secs_f32() / particle.lifetime);
            let [start_size, end_size] = self.config.size_over_life;
            let [start, end] = self.config.color_over_life;
            let half_size = lerp(start_size, end_size, progress) * 0.5;
            let basis = transform.affine().matrix2;

            ParticleRenderInstance {
                world_center: center,
                world_x_axis: basis.x_axis * half_size,
                world_y_axis: basis.y_axis * half_size,
                world_z: transform.z(),
                color: Color::rgba(
                    lerp(start.r, end.r, progress),
                    lerp(start.g, end.g, progress),
                    lerp(start.b, end.b, progress),
                    lerp(start.a, end.a, progress),
                ),
                texture: self.config.texture,
                alpha_mode: self.config.alpha_mode,
                layer: self.config.layer,
                sort_key: self.config.sort_key,
            }
        })
    }

    fn advance(&mut self, delta: Duration, global: &GlobalTransform2d) {
        let seconds = delta.as_secs_f32();
        let velocity_change = self.config.gravity * seconds;
        self.particles.retain_mut(|particle| {
            particle.age = particle.age.saturating_add(delta);
            if particle.age.as_secs_f32() >= particle.lifetime {
                return false;
            }
            particle.position += (particle.velocity + velocity_change * 0.5) * seconds;
            particle.velocity += velocity_change;
            true
        });

        // Accumulate in particle-nanoseconds to preserve exact whole emissions for common
        // rates and tick durations, such as one particle after ten 100 ms ticks at 1 Hz.
        const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
        let credit = self.rate_remainder + f64::from(self.config.rate) * delta.as_nanos() as f64;
        self.rate_remainder = credit % NANOS_PER_SECOND;
        let rate_count = (credit / NANOS_PER_SECOND) as usize;
        let requested = mem::take(&mut self.pending_burst).saturating_add(rate_count);
        let count = requested.min(self.config.max_particles - self.particles.len());
        let spawn_transform = if self.config.local_space {
            GlobalTransform2d::IDENTITY
        } else {
            *global
        };

        for _ in 0..count {
            let lifetime = sample(&self.config.lifetime_range, &mut self.random_state);
            let speed = sample(&self.config.speed_range, &mut self.random_state);
            let angle = sample(&self.config.angle_range, &mut self.random_state);
            let (sin, cos) = angle.sin_cos();
            let velocity = spawn_transform
                .affine()
                .transform_vector2(Vec2::new(cos, sin) * speed);

            self.particles.push(Particle {
                position: spawn_transform.translation(),
                velocity,
                spawn_transform,
                age: Duration::ZERO,
                lifetime,
            });
        }
    }
}

#[derive(Debug)]
struct Particle {
    position: Vec2,
    velocity: Vec2,
    spawn_transform: GlobalTransform2d,
    age: Duration,
    lifetime: f32,
}

fn validate_rate(rate: f32) -> Result<(), ParticleEmitterError> {
    if rate.is_finite() && rate >= 0.0 {
        Ok(())
    } else {
        Err(ParticleEmitterError::InvalidRate)
    }
}

fn finite_ordered_range(range: &RangeInclusive<f32>) -> bool {
    range.start().is_finite() && range.end().is_finite() && range.start() <= range.end()
}

fn lerp(start: f32, end: f32, progress: f64) -> f32 {
    ((1.0 - progress) * f64::from(start) + progress * f64::from(end)) as f32
}

fn sample(range: &RangeInclusive<f32>, state: &mut u64) -> f32 {
    // SplitMix64 has a full-period stream, including for seed zero. Use 24 bits for the fraction.
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut bits = *state;
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    bits ^= bits >> 31;
    let progress = (bits >> 40) as f64 / 16_777_216.0;
    lerp(*range.start(), *range.end(), progress)
}

/// Updates particles in fixed-step finalization, after [`SpatialSet::AfterFixed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct ParticleUpdateSet;

/// Installs fixed-step particle simulation. Requires `SpatialPlugin`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParticlePlugin;

pub const PARTICLE_PLUGIN_ID: PluginId = PluginId::new("nara.particles");
const PARTICLE_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PARTICLE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[SPATIAL_PLUGIN_ID]);

impl Plugin for ParticlePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PARTICLE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.configure_sets(
            CoreStage::FixedUpdate,
            ParticleUpdateSet
                .in_set(FixedUpdateSet::Finalize)
                .after(SpatialSet::AfterFixed),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            advance_particles.in_set(ParticleUpdateSet),
        )?;
        Ok(())
    }
}

fn advance_particles(
    time: Res<FixedTime>,
    mut emitters: Query<(&mut ParticleEmitter2d, &GlobalTransform2d)>,
) {
    for (mut emitter, global) in &mut emitters {
        emitter.advance(time.delta(), global);
    }
}
