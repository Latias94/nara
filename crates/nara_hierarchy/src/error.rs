use nara_ecs::Entity;
use thiserror::Error;

/// A rejected hierarchy construction or validation operation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HierarchyError {
    #[error("hierarchy mutation would run unsupported lifecycle work")]
    LifecycleWork,
    #[error("hierarchy entity {entity:?} does not exist")]
    MissingEntity { entity: Entity },
    #[error("hierarchy child entity {child:?} does not exist")]
    MissingChild { child: Entity },
    #[error("hierarchy parent {parent:?} referenced by child {child:?} does not exist")]
    MissingParent { child: Entity, parent: Entity },
    #[error("entity {entity:?} cannot be its own structural parent")]
    SelfParent { entity: Entity },
    #[error("hierarchy child {child:?} already has parent {parent:?}")]
    AlreadyParented { child: Entity, parent: Entity },
    #[error("hierarchy entity {entity:?} is already placed")]
    AlreadyPlaced { entity: Entity },
    #[error("hierarchy entity {entity:?} is not placed")]
    Unplaced { entity: Entity },
    #[error("hierarchy entity {entity:?} source placement does not match its current placement")]
    PlacementContextMismatch { entity: Entity },
    #[error("hierarchy parent {parent:?} for entity {entity:?} is not placed")]
    OwnerUnplaced { entity: Entity, parent: Entity },
    #[error("hierarchy entity {entity:?} cannot use itself as a placement anchor")]
    SelfAnchor { entity: Entity },
    #[error("hierarchy anchor {anchor:?} for entity {entity:?} is missing from the target owner")]
    AnchorMissing { entity: Entity, anchor: Entity },
    #[error("hierarchy entity {entity:?} cannot be removed while it has placed children")]
    HasChildren { entity: Entity },
    #[error("hierarchy contains a structural cycle through entity {entity:?}")]
    Cycle { entity: Entity },
    #[error("registered hierarchy root {entity:?} does not exist")]
    RootEntityMissing { entity: Entity },
    #[error("hierarchy root {entity:?} is registered more than once")]
    RootDuplicate { entity: Entity },
    #[error("hierarchy root {entity:?} also has parent {parent:?}")]
    RootParented { entity: Entity, parent: Entity },
    #[error("hierarchy participant {entity:?} has no registered root-or-parent placement")]
    PlacementMissing { entity: Entity },
    #[error("hierarchy participant {entity:?} lost its private membership component")]
    MembershipMissing { entity: Entity },
    #[error("parent {parent:?} is missing child {child:?} from its reverse projection")]
    ReverseMissing { parent: Entity, child: Entity },
    #[error("parent {parent:?} contains duplicate child {child:?} in its reverse projection")]
    ReverseDuplicate { parent: Entity, child: Entity },
    #[error("parent {parent:?} has an empty reverse projection component")]
    ReverseEmpty { parent: Entity },
    #[error("parent {parent:?} reverse projection references missing child {child:?}")]
    ReverseChildMissing { parent: Entity, child: Entity },
    #[error(
        "parent {parent:?} reverse projection contains child {child:?}, whose actual parent is {actual_parent:?}"
    )]
    ReverseUnexpected {
        parent: Entity,
        child: Entity,
        actual_parent: Option<Entity>,
    },
}
