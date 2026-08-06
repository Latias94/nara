use nara_ecs::schedule::SystemSet;

/// Engine-owned stages for normalized input projection and delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum InputSet {
    ProjectTimeline,
    PublishObservations,
    RouteUi,
    StageActions,
    ResolveActions,
}
