//! Finite data exchanged by independent motion fixture authors, not an engine motion API.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use nara_core::Vec2;
use nara_ecs::{Component, Entity, Resource};

#[derive(Debug, Clone, Copy, Resource)]
pub struct Actors {
    pub parent: Entity,
    pub kinematic: Entity,
    pub dynamic: Entity,
}

#[derive(Debug, Clone, Copy, Resource)]
pub struct MotionInput {
    pub tick: u64,
    pub delta: Vec2,
    pub parent_translation: Vec2,
}

#[derive(Debug, Clone, Copy, Component)]
pub struct RootMotionIntent {
    pub tick: u64,
    pub delta: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Intent,
    Target,
    WorkerStarted,
    WorkerCompleted,
    Joined,
    PoseQueued,
    Callback,
    Published,
    NonSend,
    Capture,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub tick: u64,
    pub phase: Phase,
    pub thread: std::thread::ThreadId,
}

#[derive(Debug, Clone, Default, Resource)]
pub struct Trace(Arc<Mutex<Vec<Observation>>>);

impl Trace {
    pub fn record(&self, tick: u64, phase: Phase) {
        self.0.lock().unwrap().push(Observation {
            tick,
            phase,
            thread: std::thread::current().id(),
        });
    }

    pub fn observations(&self) -> Vec<Observation> {
        self.0.lock().unwrap().clone()
    }
}
