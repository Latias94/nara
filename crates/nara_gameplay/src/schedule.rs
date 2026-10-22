use nara_ecs::schedule::SystemSet;

/// Public semantic anchors around the immutable current-tick command batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum GameplayCommandSet {
    /// Read and consume the admitted immutable current-tick batch.
    Consume,
    /// Capture the same batch after successful consumption and before engine acknowledgement.
    ///
    /// A managed error reported by consumption or its deferred commands skips this set. Errors
    /// within Capture prevent acknowledgement; they do not roll back earlier capture effects or
    /// abort other members already eligible to run. Skipped members are not errors.
    Capture,
}

/// Engine-owned command ingress and retirement stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub(crate) enum GameplayCommandInternalSet {
    CollectLocalCommands,
    MapLocalActions,
    Admit,
    CompleteConsumption,
    Acknowledge,
}
