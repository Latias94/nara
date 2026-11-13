use nara_ecs::schedule::SystemSet;

/// Ordering anchors for normalized input projection, custom UI routing and action delivery.
///
/// The first four sets run in [`nara_app::CoreStage::First`] in declaration order.
/// [`Self::ResolveActions`] runs in [`nara_app::CoreStage::PreUpdate`]. Ordering constraints apply
/// only within the same schedule. Membership does not order systems within a set.
///
/// Configure seat maps before [`Self::StageActions`] to affect that frame's observed lifetime,
/// or before [`Self::ProjectTimeline`] when the configuration also changes routing. Custom UI
/// adapters join [`Self::RouteUi`] and complete pending decisions through
/// [`crate::InputTimeline::route_pending_ui_input`]. These labels do not grant mutation authority
/// over the engine's projection, fixed command admission or presentation outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum InputSet {
    /// Projects retained normalized input into the current frame's button/pointer observations.
    ProjectTimeline,
    /// Publishes the immutable frame observations after physical projection.
    PublishObservations,
    /// Runs UI adapters before unresolved input falls through to gameplay staging.
    RouteUi,
    /// Observes seat/context/mapping lifetimes and finalizes remaining UI decisions for actions.
    StageActions,
    /// Resolves staged binding and semantic outcomes before local gameplay command collection.
    ResolveActions,
}
