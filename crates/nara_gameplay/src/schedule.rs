use nara_ecs::schedule::SystemSet;

/// Public semantic anchors around the immutable current-tick command batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum GameplayCommandSet {
    /// Read and consume the admitted immutable current-tick batch.
    Consume,
    /// Capture the same batch after every consumer and before engine acknowledgement.
    Capture,
}

/// Engine-owned command ingress and retirement stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub(crate) enum GameplayCommandInternalSet {
    CollectLocalCommands,
    MapLocalActions,
    Admit,
    Acknowledge,
}
