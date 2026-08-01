use nara_ecs::Entity;
use thiserror::Error;

/// A rejected hierarchy construction or validation operation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HierarchyError {
    #[error("hierarchy child entity {child:?} does not exist")]
    MissingChild { child: Entity },
    #[error("hierarchy parent {parent:?} referenced by child {child:?} does not exist")]
    MissingParent { child: Entity, parent: Entity },
    #[error("entity {entity:?} cannot be its own structural parent")]
    SelfParent { entity: Entity },
    #[error("hierarchy child {child:?} already has parent {parent:?}")]
    AlreadyParented { child: Entity, parent: Entity },
    #[error("hierarchy contains a structural cycle through entity {entity:?}")]
    Cycle { entity: Entity },
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
