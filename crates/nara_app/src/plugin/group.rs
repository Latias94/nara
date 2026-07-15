use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use super::{
    Plugin, PluginDeclaration, PluginGroupId, PluginId, PluginSlotId, definition::PluginDefinition,
    resolve::PluginPlanError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSlotPresence {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginSlot {
    pub(super) id: PluginSlotId,
    pub(super) expected_plugin: PluginId,
    pub(super) presence: PluginSlotPresence,
}

impl PluginSlot {
    #[must_use]
    pub const fn required(id: PluginSlotId, expected_plugin: PluginId) -> Self {
        Self {
            id,
            expected_plugin,
            presence: PluginSlotPresence::Required,
        }
    }

    #[must_use]
    pub const fn optional(id: PluginSlotId, expected_plugin: PluginId) -> Self {
        Self {
            id,
            expected_plugin,
            presence: PluginSlotPresence::Optional,
        }
    }

    #[must_use]
    pub const fn id(self) -> PluginSlotId {
        self.id
    }

    #[must_use]
    pub const fn expected_plugin(self) -> PluginId {
        self.expected_plugin
    }

    #[must_use]
    pub const fn presence(self) -> PluginSlotPresence {
        self.presence
    }
}

pub trait PluginGroup: Sized + Send + Sync + 'static {
    const ID: PluginGroupId;

    fn build(self) -> PluginGroupBuilder;

    #[must_use]
    fn edit(self) -> EditedPluginGroup<Self> {
        EditedPluginGroup {
            group: self,
            edits: Vec::new(),
        }
    }
}

pub struct PluginGroupBuilder {
    pub(super) items: Vec<PluginGroupItem>,
}

impl Default for PluginGroupBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginGroupBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    #[must_use]
    pub fn add_definition(mut self, definition: PluginDefinition) -> Self {
        self.items.push(PluginGroupItem::Entry {
            slot: None,
            definition,
        });
        self
    }

    #[must_use]
    pub fn add_slot(mut self, slot: PluginSlot, definition: PluginDefinition) -> Self {
        self.items.push(PluginGroupItem::Entry {
            slot: Some(slot),
            definition,
        });
        self
    }

    #[must_use]
    pub fn add_group<G: PluginGroup>(mut self, group: G) -> Self {
        self.items.push(PluginGroupItem::Group {
            group: Box::new(TypedPluginGroup(Some(group))),
            edits: Vec::new(),
        });
        self
    }

    #[must_use]
    pub fn add_edited_group<G: PluginGroup>(mut self, group: EditedPluginGroup<G>) -> Self {
        self.items.push(PluginGroupItem::Group {
            group: Box::new(TypedPluginGroup(Some(group.group))),
            edits: group.edits,
        });
        self
    }
}

pub(super) enum PluginGroupItem {
    Entry {
        slot: Option<PluginSlot>,
        definition: PluginDefinition,
    },
    Group {
        group: Box<dyn ErasedPluginGroup>,
        edits: Vec<PluginEdit>,
    },
}

pub(super) trait ErasedPluginGroup: Send + Sync {
    fn id(&self) -> PluginGroupId;
    fn build(self: Box<Self>) -> PluginGroupBuilder;
}

struct TypedPluginGroup<G>(Option<G>);

impl<G: PluginGroup> ErasedPluginGroup for TypedPluginGroup<G> {
    fn id(&self) -> PluginGroupId {
        G::ID
    }

    fn build(mut self: Box<Self>) -> PluginGroupBuilder {
        self.0
            .take()
            .expect("an erased plugin group is expanded at most once")
            .build()
    }
}

#[derive(Debug, Clone)]
pub(super) enum EditTarget {
    Plugin(fn() -> &'static PluginDeclaration),
    Slot(PluginSlotId),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ResolvedEditTarget {
    Plugin(PluginId),
    Slot(PluginSlotId),
}

impl EditTarget {
    fn resolve(&self) -> Result<ResolvedEditTarget, PluginPlanError> {
        match self {
            Self::Plugin(provider) => catch_unwind(AssertUnwindSafe(*provider))
                .map(|declaration| ResolvedEditTarget::Plugin(declaration.id))
                .map_err(|_| PluginPlanError::DeclarationPanicked),
            Self::Slot(slot) => Ok(ResolvedEditTarget::Slot(*slot)),
        }
    }
}

#[derive(Clone)]
pub(super) enum PluginEdit {
    Disable(EditTarget),
    Configure(EditTarget, PluginDefinition),
    InsertAfter(EditTarget, PluginDefinition),
    InsertBefore(EditTarget, PluginDefinition),
}

#[derive(Clone)]
pub(super) enum ResolvedPluginEdit {
    Disable(ResolvedEditTarget),
    Configure(ResolvedEditTarget, PluginDefinition),
    InsertAfter(ResolvedEditTarget, PluginDefinition),
    InsertBefore(ResolvedEditTarget, PluginDefinition),
}

impl PluginEdit {
    fn resolve(self) -> Result<ResolvedPluginEdit, PluginPlanError> {
        match self {
            Self::Disable(target) => Ok(ResolvedPluginEdit::Disable(target.resolve()?)),
            Self::Configure(target, definition) => Ok(ResolvedPluginEdit::Configure(
                target.resolve()?,
                definition.resolve_declaration()?,
            )),
            Self::InsertAfter(target, definition) => Ok(ResolvedPluginEdit::InsertAfter(
                target.resolve()?,
                definition.resolve_declaration()?,
            )),
            Self::InsertBefore(target, definition) => Ok(ResolvedPluginEdit::InsertBefore(
                target.resolve()?,
                definition.resolve_declaration()?,
            )),
        }
    }
}

pub(super) fn resolve_edits(
    edits: Vec<PluginEdit>,
) -> Result<Vec<ResolvedPluginEdit>, PluginPlanError> {
    edits.into_iter().map(PluginEdit::resolve).collect()
}

pub struct EditedPluginGroup<G> {
    group: G,
    edits: Vec<PluginEdit>,
}

impl<G: PluginGroup> EditedPluginGroup<G> {
    #[must_use]
    pub fn disable<P: Plugin>(mut self) -> Self {
        self.edits
            .push(PluginEdit::Disable(EditTarget::Plugin(P::declaration)));
        self
    }

    #[must_use]
    pub fn disable_slot(mut self, slot: PluginSlotId) -> Self {
        self.edits.push(PluginEdit::Disable(EditTarget::Slot(slot)));
        self
    }

    #[must_use]
    pub fn configure(mut self, definition: PluginDefinition) -> Self {
        self.edits.push(PluginEdit::Configure(
            EditTarget::Plugin(definition.declaration_provider),
            definition,
        ));
        self
    }

    #[must_use]
    pub fn configure_slot(mut self, slot: PluginSlotId, definition: PluginDefinition) -> Self {
        self.edits
            .push(PluginEdit::Configure(EditTarget::Slot(slot), definition));
        self
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.edits.push(PluginEdit::InsertAfter(
            EditTarget::Plugin(P::declaration),
            definition,
        ));
        self
    }

    #[must_use]
    pub fn insert_after_slot(mut self, slot: PluginSlotId, definition: PluginDefinition) -> Self {
        self.edits
            .push(PluginEdit::InsertAfter(EditTarget::Slot(slot), definition));
        self
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.edits.push(PluginEdit::InsertBefore(
            EditTarget::Plugin(P::declaration),
            definition,
        ));
        self
    }
}

#[doc(hidden)]
pub struct PluginMarker;
#[doc(hidden)]
pub struct PluginDefinitionMarker;
#[doc(hidden)]
pub struct PluginGroupMarker;
#[doc(hidden)]
pub struct EditedPluginGroupMarker;

pub(super) mod sealed {
    use super::PluginInputCollection;

    pub trait Plugins<Marker> {
        fn collect(self, collection: &mut PluginInputCollection);
    }

    pub trait ReplayablePlugins<Marker> {
        fn collect_replayable(self, collection: &mut PluginInputCollection);
    }
}

pub trait Plugins<Marker>: sealed::Plugins<Marker> {}
impl<T, Marker> Plugins<Marker> for T where T: sealed::Plugins<Marker> {}

pub trait ReplayablePlugins<Marker>: sealed::ReplayablePlugins<Marker> {}
impl<T, Marker> ReplayablePlugins<Marker> for T where T: sealed::ReplayablePlugins<Marker> {}

impl<P: Plugin> sealed::Plugins<PluginMarker> for P {
    fn collect(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Direct {
            declaration: P::declaration(),
            plugin: Arc::new(self),
        });
    }
}

impl sealed::Plugins<PluginDefinitionMarker> for PluginDefinition {
    fn collect(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Definition(self));
    }
}

impl sealed::ReplayablePlugins<PluginDefinitionMarker> for PluginDefinition {
    fn collect_replayable(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Definition(self));
    }
}

impl<G: PluginGroup> sealed::Plugins<PluginGroupMarker> for G {
    fn collect(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Group {
            group: Box::new(TypedPluginGroup(Some(self))),
            edits: Vec::new(),
        });
    }
}

impl<G: PluginGroup> sealed::ReplayablePlugins<PluginGroupMarker> for G {
    fn collect_replayable(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Group {
            group: Box::new(TypedPluginGroup(Some(self))),
            edits: Vec::new(),
        });
    }
}

impl<G: PluginGroup> sealed::Plugins<EditedPluginGroupMarker> for EditedPluginGroup<G> {
    fn collect(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Group {
            group: Box::new(TypedPluginGroup(Some(self.group))),
            edits: self.edits,
        });
    }
}

impl<G: PluginGroup> sealed::ReplayablePlugins<EditedPluginGroupMarker> for EditedPluginGroup<G> {
    fn collect_replayable(self, collection: &mut PluginInputCollection) {
        collection.roots.push(PluginInput::Group {
            group: Box::new(TypedPluginGroup(Some(self.group))),
            edits: self.edits,
        });
    }
}

macro_rules! impl_plugin_tuple {
    ($(($type:ident, $marker:ident, $index:tt)),+ $(,)?) => {
        impl<$($type, $marker),+> sealed::Plugins<($($marker,)+)> for ($($type,)+)
        where
            $($type: Plugins<$marker>,)+
        {
            fn collect(self, collection: &mut PluginInputCollection) {
                $(sealed::Plugins::<$marker>::collect(self.$index, collection);)+
            }
        }

        impl<$($type, $marker),+> sealed::ReplayablePlugins<($($marker,)+)> for ($($type,)+)
        where
            $($type: ReplayablePlugins<$marker>,)+
        {
            fn collect_replayable(self, collection: &mut PluginInputCollection) {
                $(sealed::ReplayablePlugins::<$marker>::collect_replayable(self.$index, collection);)+
            }
        }
    };
}

impl_plugin_tuple!((A, MA, 0), (B, MB, 1));
impl_plugin_tuple!((A, MA, 0), (B, MB, 1), (C, MC, 2));
impl_plugin_tuple!((A, MA, 0), (B, MB, 1), (C, MC, 2), (D, MD, 3));
impl_plugin_tuple!((A, MA, 0), (B, MB, 1), (C, MC, 2), (D, MD, 3), (E, ME, 4));
impl_plugin_tuple!(
    (A, MA, 0),
    (B, MB, 1),
    (C, MC, 2),
    (D, MD, 3),
    (E, ME, 4),
    (F, MF, 5)
);
impl_plugin_tuple!(
    (A, MA, 0),
    (B, MB, 1),
    (C, MC, 2),
    (D, MD, 3),
    (E, ME, 4),
    (F, MF, 5),
    (G, MG, 6)
);
impl_plugin_tuple!(
    (A, MA, 0),
    (B, MB, 1),
    (C, MC, 2),
    (D, MD, 3),
    (E, ME, 4),
    (F, MF, 5),
    (G, MG, 6),
    (H, MH, 7)
);

#[derive(Default)]
pub struct PluginInputCollection {
    pub(super) roots: Vec<PluginInput>,
}

pub(super) enum PluginInput {
    Direct {
        declaration: &'static PluginDeclaration,
        plugin: Arc<dyn Plugin>,
    },
    Definition(PluginDefinition),
    Group {
        group: Box<dyn ErasedPluginGroup>,
        edits: Vec<PluginEdit>,
    },
}
