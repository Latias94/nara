use std::{error::Error, fmt};

use nara_core::Vec2;
use rapier2d::prelude::{
    BroadPhaseBvh, CCDSolver, ColliderHandle, ColliderSet, ImpulseJointSet, IntegrationParameters,
    IslandManager, MultibodyJointSet, NarrowPhase, PhysicsPipeline, RigidBodyHandle, RigidBodySet,
};
use serde::Serialize;

use super::{Sample, Solver};

/// Concrete persistent solver state for the finite fixture's quiescent attempt boundaries.
#[derive(Clone, Serialize)]
pub struct SolverSnapshot {
    sample: Sample,
    target: Vec2,
    integration: IntegrationParameters,
    islands: IslandManager,
    broad: BroadPhaseBvh,
    narrow: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse: ImpulseJointSet,
    multibody: MultibodyJointSet,
    ccd: CCDSolver,
    kinematic: RigidBodyHandle,
    dynamic: RigidBodyHandle,
    floor_collider: ColliderHandle,
    dynamic_collider: ColliderHandle,
}

/// A refusal to capture opaque external state or a failure to encode the evidence.
#[derive(Debug)]
pub enum SolverCheckpointError {
    WorkerGateConfigured,
    Encoding(bincode::Error),
}

impl fmt::Display for SolverCheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerGateConfigured => {
                formatter.write_str("solver checkpoint requires no configured worker gate")
            }
            Self::Encoding(error) => {
                write!(formatter, "solver checkpoint encoding failed: {error}")
            }
        }
    }
}

impl Error for SolverCheckpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WorkerGateConfigured => None,
            Self::Encoding(error) => Some(error.as_ref()),
        }
    }
}

impl SolverSnapshot {
    /// Encodes same-build evidence for complete-byte comparisons and continuation controls.
    ///
    /// Byte determinism must be established by the consumer. These bytes have no public load API
    /// or cross-build persistence contract.
    pub fn encoded(&self) -> Result<Vec<u8>, SolverCheckpointError> {
        bincode::serialize(self).map_err(SolverCheckpointError::Encoding)
    }
}

impl Solver {
    /// Captures persistent state after the consumer has completed commands and joined workers.
    pub fn checkpoint(&self) -> Result<SolverSnapshot, SolverCheckpointError> {
        if self.gate.is_some() {
            return Err(SolverCheckpointError::WorkerGateConfigured);
        }
        Ok(SolverSnapshot {
            sample: self.sample,
            target: self.target,
            integration: self.integration,
            islands: self.islands.clone(),
            broad: self.broad.clone(),
            narrow: self.narrow.clone(),
            bodies: self.bodies.clone(),
            colliders: self.colliders.clone(),
            impulse: self.impulse.clone(),
            multibody: self.multibody.clone(),
            ccd: self.ccd.clone(),
            kinematic: self.kinematic,
            dynamic: self.dynamic,
            floor_collider: self.floor_collider,
            dynamic_collider: self.dynamic_collider,
        })
    }

    /// Restores persistent state, rebuilds Rapier scratch, and revokes prior join permission.
    ///
    /// The receiving solver retains its execution mode. Rapier 0.33.0 documents
    /// `PhysicsPipeline` as temporary working memory that can be recreated at any time.
    pub fn restore(&mut self, snapshot: &SolverSnapshot) -> Result<(), SolverCheckpointError> {
        if self.gate.is_some() {
            return Err(SolverCheckpointError::WorkerGateConfigured);
        }
        self.joined_tick = None;
        self.sample = snapshot.sample;
        self.target = snapshot.target;
        self.integration = snapshot.integration;
        self.islands = snapshot.islands.clone();
        self.broad = snapshot.broad.clone();
        self.narrow = snapshot.narrow.clone();
        self.bodies = snapshot.bodies.clone();
        self.colliders = snapshot.colliders.clone();
        self.impulse = snapshot.impulse.clone();
        self.multibody = snapshot.multibody.clone();
        self.ccd = snapshot.ccd.clone();
        self.kinematic = snapshot.kinematic;
        self.dynamic = snapshot.dynamic;
        self.floor_collider = snapshot.floor_collider;
        self.dynamic_collider = snapshot.dynamic_collider;
        self.pipeline = PhysicsPipeline::new();
        Ok(())
    }
}
