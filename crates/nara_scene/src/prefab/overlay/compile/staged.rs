//! Source skeletons precede layered layout and selected component interpretation.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_identity::ReferenceInputId;
use nara_reflect::ComponentTypeId;

use super::*;
use crate::SceneComponentRecord;
use crate::prefab::SceneCompilationWorkingSet;
use crate::provenance::{provenance_asset_ref_bytes, provenance_string_bytes};

#[cfg(feature = "serde")]
mod extraction;
mod inputs;
mod output;
mod reference_program;
mod values;

#[cfg(feature = "serde")]
pub(crate) use extraction::SourceCutMembership;
#[cfg(feature = "serde")]
pub(super) use extraction::capture_source_cut;

type NodeId = usize;
type ScopeId = usize;

/// Locates a declaration in an immutable input, never in a growing arena allocation.
#[derive(Debug, Clone)]
struct Site {
    source: ScopeId,
    entity: usize,
    segments: Vec<PrefabInstancePathSegment>,
}

#[derive(Debug, Clone, Copy)]
enum Record {
    Source { scope: ScopeId, entity: usize },
    Addition { scope: ScopeId, index: usize },
    Set { scope: ScopeId, index: usize },
}

#[derive(Debug, Clone)]
struct Program {
    record: Record,
    properties: Vec<(ScopeId, usize)>,
}

#[derive(Debug, PartialEq, Eq)]
struct State {
    overlay: Option<PrefabInstanceLocation>,
    demands: Vec<(DemandSite, usize, usize)>,
    reference: Option<ReferenceDemand>,
}

#[derive(Debug, PartialEq, Eq)]
struct ReferenceDemand {
    reference: nara_identity::AuthoredEntityReference,
    remaining_hops: usize,
    lexical_source: PrefabDeclarationSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DemandSite {
    Source(PrefabDeclarationSource),
    Overlay(PrefabInstanceLocation),
    RootBinding(PrefabInstanceLocation),
}

struct Node {
    scope: ScopeId,
    origin: PrefabEntityOrigin,
    record: Record,
    locator: SceneEntityId,
    slot: Option<Site>,
    entered: Option<ScopeId>,
    opened: bool,
    programs: BTreeMap<ComponentTypeId, Program>,
}

/// Separate namespaces allow borrowed source-ID lookup without constructing an owned origin.
#[derive(Default)]
struct ScopeNodes {
    source: BTreeMap<SceneEntityId, NodeId>,
    additions: BTreeMap<SceneEntityId, NodeId>,
}

impl ScopeNodes {
    fn source_entity(&self, entity: &SceneEntityId) -> Option<&NodeId> {
        self.source.get(entity)
    }

    fn get(&self, origin: &PrefabEntityOrigin) -> Option<&NodeId> {
        match origin {
            PrefabEntityOrigin::SourceEntity(entity) => self.source_entity(entity),
            PrefabEntityOrigin::InstanceAddition(entity) => self.additions.get(entity),
        }
    }

    fn insert(&mut self, origin: PrefabEntityOrigin, node: NodeId) -> Option<NodeId> {
        match origin {
            PrefabEntityOrigin::SourceEntity(entity) => self.source.insert(entity, node),
            PrefabEntityOrigin::InstanceAddition(entity) => self.additions.insert(entity, node),
        }
    }

    fn len(&self) -> usize {
        self.source.len() + self.additions.len()
    }

    fn values(&self) -> impl Iterator<Item = &NodeId> {
        // Preserve the derived PrefabEntityOrigin ordering: sources precede additions.
        self.source.values().chain(self.additions.values())
    }
}

struct Scope {
    source: Option<ResolvedPrefabSource>,
    request: Option<AssetRef>,
    site: Option<Site>,
    anchor: Option<NodeId>,
    depth: usize,
    nodes: ScopeNodes,
    roots: Vec<NodeId>,
    /// Declaration facets remain addressable after their roots share an anchor's entity.
    aliases: BTreeMap<NodeId, NodeId>,
    state: Option<State>,
    input: BTreeSet<NodeId>,
    output: BTreeSet<NodeId>,
}

/// Borrows declarations independently of the compiler's mutable admission owner.
fn instance_from_inputs<'a>(
    root_entities: &'a [SceneEntityRecord],
    scopes: &'a [Scope],
    site: &Site,
) -> Option<&'a PrefabInstance> {
    let entities = scopes[site.source]
        .source
        .as_ref()
        .map_or(root_entities, |source| &source.document().entities);
    let mut instance = entities.get(site.entity)?.prefab.as_ref()?;
    for segment in &site.segments {
        let overlay = instance.overrides.as_ref()?;
        instance = match *segment {
            PrefabInstancePathSegment::Addition { index } => {
                overlay.additions.get(index)?.prefab.as_ref()?
            }
            PrefabInstancePathSegment::InstanceClaim { index } => {
                overlay.instances.get(index)?.value.as_ref()?
            }
        };
    }
    Some(instance)
}

struct Compiler<'root, 'context, 'owner, F, E> {
    root: SourceTopology<'root>,
    selected_anchor: Option<NodeId>,
    context: &'context mut SceneCompilationContext<'owner, F, E>,
    scopes: Vec<Scope>,
    nodes: Vec<Node>,
    locators: BTreeMap<SceneEntityId, NodeId>,
    postorder: Vec<ScopeId>,
}

/// Borrows the actual source topology without changing its nominal document kind.
#[derive(Clone, Copy)]
pub(super) struct SourceTopology<'a> {
    pub(super) inputs: Option<&'a BTreeSet<ReferenceInputId>>,
    pub(super) placements: &'a [SceneEntityPlacement],
    pub(super) entities: &'a [SceneEntityRecord],
    /// Witnessed containing-source references that must survive explicit source conversion.
    pub(super) reference_queries: &'a [nara_identity::AuthoredEntityReference],
}

/// Preserves a standalone Prefab's declared input environment at the source boundary.
pub(super) fn expand_source<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: SourceTopology<'_>,
) -> Result<SceneDraft, SceneCompilationFailure>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    compile(context, source, None).map(|output| output.document)
}

#[cfg(feature = "serde")]
pub(super) fn expand_selected<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: SourceTopology<'_>,
    anchor: &SceneEntityId,
) -> Result<StagedSceneOutput, SceneCompilationFailure>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    // The containing scope registers direct shells first, in this source record order.
    let selected = source
        .entities
        .iter()
        .position(|entity| &entity.id == anchor && entity.prefab.is_some())
        .ok_or(SceneCompilationFailureKind::MissingTarget)?;
    compile(context, source, Some(selected))
}

fn compile<F, E>(
    context: &mut SceneCompilationContext<'_, F, E>,
    source: SourceTopology<'_>,
    selected_anchor: Option<NodeId>,
) -> Result<StagedSceneOutput, SceneCompilationFailure>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    #[cfg(feature = "work-observation")]
    let _work_span = tracing::info_span!(target: "nara::scene::work", "scene_expand").entered();
    let mut compiler = Compiler::start(context, source, selected_anchor)?;
    let mut scanned = 0;
    let mut layout = compiler.close_structure(&mut scanned)?;
    let baseline = compiler.context.usage;
    let source_upgrade_required = compiler.context.source_upgrade_required;
    // Reference intent follows the effective leaves, including every property suffix.
    // A newly entered scope invalidates this unpublished value/visibility candidate.
    loop {
        compiler.compose_programs()?;
        let mut references = compiler.materialize_components(&mut layout)?;
        if compiler.close_reference_demands(&mut references)?
            && compiler.close_source_reference_demands()?
        {
            return compiler.materialize(layout, references);
        }

        drop(references);
        drop(layout);
        // Release only the discarded materialization. Source acquisition, structural
        // allocations and all normalization scratch remain cumulatively admitted.
        compiler.context.usage.materialized_components = baseline.materialized_components;
        compiler.context.usage.materialized_value_nodes = baseline.materialized_value_nodes;
        compiler.context.usage.materialized_value_bytes = baseline.materialized_value_bytes;
        compiler.context.source_upgrade_required = source_upgrade_required;
        layout = compiler.close_structure(&mut scanned)?;
    }
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn start(
        context: &'context mut SceneCompilationContext<'owner, F, E>,
        source: SourceTopology<'root>,
        selected_anchor: Option<NodeId>,
    ) -> Result<Self, SceneCompilationFailure> {
        let mut compiler = Self {
            root: source,
            selected_anchor,
            context,
            scopes: Vec::new(),
            nodes: Vec::new(),
            locators: BTreeMap::new(),
            postorder: Vec::new(),
        };
        compiler.admit(Some(size_of::<Scope>()))?;
        compiler.scopes.push(Scope {
            source: None,
            request: None,
            site: None,
            anchor: None,
            depth: 0,
            nodes: ScopeNodes::default(),
            roots: Vec::new(),
            aliases: BTreeMap::new(),
            state: None,
            input: BTreeSet::new(),
            output: BTreeSet::new(),
        });
        compiler.register_scope(0)?;
        Ok(compiler)
    }

    fn admit(&mut self, bytes: Option<usize>) -> Result<(), SceneCompilationFailure> {
        self.context
            .reserve_scratch_bytes(bytes, None, None)
            .then_some(())
            .ok_or_else(|| SceneCompilationFailureKind::WorkLimit.into())
    }

    /// Ownership follows instance entries, not visual parentage or locator spelling.
    fn selects_values(&self, node: NodeId) -> bool {
        let Some(selected) = self.selected_anchor else {
            return true;
        };
        if node == selected
            && self.nodes[node].entered.is_some_and(|scope| {
                self.scopes[scope]
                    .site
                    .as_ref()
                    .and_then(|site| self.instance(site))
                    .is_some_and(|instance| {
                        matches!(instance.binding, crate::PrefabInstanceBinding::Root { .. })
                    })
            })
        {
            return true;
        }
        let mut scope = self.nodes[node].scope;
        while let Some(anchor) = self.scopes[scope].anchor {
            if anchor == selected {
                return true;
            }
            scope = self.nodes[anchor].scope;
        }
        false
    }

    fn source_entities(&self, scope: ScopeId) -> &[SceneEntityRecord] {
        self.scopes[scope]
            .source
            .as_ref()
            .map_or(self.root.entities, |source| &source.document().entities)
    }

    fn source_placements(&self, scope: ScopeId) -> &[SceneEntityPlacement] {
        self.scopes[scope]
            .source
            .as_ref()
            .map_or(self.root.placements, |source| &source.document().placements)
    }

    fn bound_root_target(&self, scope: ScopeId) -> Option<&PrefabEntityTarget> {
        let instance = self.instance(self.scopes[scope].site.as_ref()?)?;
        match &instance.binding {
            crate::PrefabInstanceBinding::Root { root, .. } => Some(root),
            crate::PrefabInstanceBinding::Forest => None,
        }
    }

    /// Root routes enter their sources during demand closure. Layout borrows the admitted
    /// route afterward, preserving source/addition facets before alias normalization.
    fn bound_root_node(&self, scope: ScopeId) -> Result<Option<NodeId>, SceneCompilationFailure> {
        let Some(target) = self.bound_root_target(scope) else {
            return Ok(None);
        };
        let mut current = scope;
        for hop in &target.through {
            let anchor = *self.scopes[current]
                .nodes
                .get(&hop.anchor)
                .ok_or(SceneCompilationFailureKind::RootBinding)?;
            current = self.nodes[anchor]
                .entered
                .ok_or(SceneCompilationFailureKind::RootBinding)?;
            if self.scopes[current].source.as_ref().unwrap().key() != &hop.expected_base {
                return Err(SceneCompilationFailureKind::SourceChanged.into());
            }
        }
        let facet = *self.scopes[current]
            .nodes
            .get(&target.entity)
            .ok_or(SceneCompilationFailureKind::RootBinding)?;
        Ok(Some(self.canonical_node(scope, facet)))
    }

    /// Resolves only materialized identity. Declaration lookup always retains the original facet.
    fn canonical_node(&self, scope: ScopeId, node: NodeId) -> NodeId {
        self.scopes[scope]
            .aliases
            .get(&node)
            .copied()
            .unwrap_or(node)
    }

    fn close_source_reference_demands(&mut self) -> Result<bool, SceneCompilationFailure> {
        use inputs::{ReferenceResolution, ResolvedReference};
        for query in self.root.reference_queries {
            match self.reference_target(0, query)? {
                ReferenceResolution::Entered => return Ok(false),
                ReferenceResolution::Invalid { reason, .. } => return Err(reason.into()),
                ReferenceResolution::Resolved {
                    reference: ResolvedReference::Local(node),
                    declaring_scope,
                } => {
                    if !self.scopes[declaring_scope].output.contains(&node)
                        || !self.scopes[0].output.contains(&node)
                    {
                        return Err(SceneCompilationFailureKind::Reference.into());
                    }
                }
                ReferenceResolution::Resolved { .. } => {}
            }
        }
        Ok(true)
    }

    fn instance(&self, site: &Site) -> Option<&PrefabInstance> {
        instance_from_inputs(self.root.entities, &self.scopes, site)
    }

    fn overlay(&self, scope: ScopeId) -> Option<&PrefabOverlay> {
        self.scopes[scope]
            .site
            .as_ref()
            .and_then(|site| self.instance(site))?
            .overrides
            .as_ref()
    }

    /// Overlay targets start in the selected base; payloads start at the physical declaration.
    fn overlay_reference_scope(&self, scope: ScopeId) -> Result<ScopeId, SceneCompilationFailure> {
        self.scopes[scope]
            .site
            .as_ref()
            .map(|site| site.source)
            .ok_or_else(|| SceneCompilationFailureKind::OwnerAdmission.into())
    }

    fn record_reference_scope(&self, record: Record) -> Result<ScopeId, SceneCompilationFailure> {
        match record {
            Record::Source { scope, .. } => Ok(scope),
            Record::Addition { scope, .. } | Record::Set { scope, .. } => {
                self.overlay_reference_scope(scope)
            }
        }
    }

    fn components(&self, record: Record) -> &BTreeMap<ComponentTypeId, SceneComponentRecord> {
        match record {
            Record::Source { scope, entity } => &self.source_entities(scope)[entity].components,
            Record::Addition { scope, index } => {
                &self
                    .overlay(scope)
                    .expect("addition has declaring overlay")
                    .additions[index]
                    .components
            }
            Record::Set { .. } => unreachable!("a Set locates one component"),
        }
    }

    fn component(&self, record: Record, component: &ComponentTypeId) -> &SceneComponentRecord {
        match record {
            Record::Set { scope, index } => match &self.overlay(scope).unwrap().components[index] {
                PrefabComponentIntent::Set { record, .. } => record,
                _ => unreachable!("a Set program retains its declaration"),
            },
            record => &self.components(record)[component],
        }
    }

    fn stable_site(&self, site: &Site) -> PrefabInstanceLocation {
        let entity = self.source_entities(site.source)[site.entity].id.clone();
        let source = self.scopes[site.source]
            .source
            .as_ref()
            .map_or(PrefabDeclarationSource::Containing, |source| {
                PrefabDeclarationSource::Prefab(source.key().clone())
            });
        PrefabInstanceLocation {
            source,
            path: PrefabInstancePath {
                anchor: entity,
                steps: site.segments.clone(),
            },
        }
    }

    fn site_bytes(&self, site: &Site) -> Option<usize> {
        size_of::<PrefabInstanceLocation>()
            .checked_add(provenance_string_bytes(
                self.source_entities(site.source)[site.entity].id.as_str(),
            )?)?
            .checked_add(
                self.scopes[site.source]
                    .source
                    .as_ref()
                    .map_or(Some(0), |source| source.key().checked_retained_bytes())?,
            )?
            .checked_add(
                site.segments
                    .len()
                    .checked_mul(size_of::<PrefabInstancePathSegment>())?,
            )
    }

    fn failure_with_location(
        &mut self,
        mut failure: SceneCompilationFailure,
        slot: Option<PrefabOverlaySlot>,
        bytes: Option<usize>,
        locate: impl FnOnce(&Self) -> PrefabInstanceLocation,
    ) -> SceneCompilationFailure {
        if failure.location.is_some()
            || matches!(
                failure.reason,
                SceneCompilationFailureKind::WorkLimit
                    | SceneCompilationFailureKind::OwnerAdmission
            )
        {
            return failure;
        }
        if self.admit(bytes).is_err() {
            return SceneCompilationFailureKind::WorkLimit.into();
        }
        failure.location = Some(locate(self));
        failure.slot = slot;
        failure
    }

    fn site_failure(
        &mut self,
        site: &Site,
        slot: Option<PrefabOverlaySlot>,
        failure: SceneCompilationFailure,
    ) -> SceneCompilationFailure {
        self.failure_with_location(failure, slot, self.site_bytes(site), |this| {
            this.stable_site(site)
        })
    }

    fn scope_failure(
        &mut self,
        scope: ScopeId,
        slot: Option<PrefabOverlaySlot>,
        failure: SceneCompilationFailure,
    ) -> SceneCompilationFailure {
        let Some(site) = self.scopes[scope].site.as_ref() else {
            return failure;
        };
        let bytes = self.site_bytes(site);
        self.failure_with_location(failure, slot, bytes, |this| {
            this.stable_site(
                this.scopes[scope]
                    .site
                    .as_ref()
                    .expect("the declaring source is retained"),
            )
        })
    }

    fn record_failure(
        &mut self,
        record: Record,
        failure: SceneCompilationFailure,
    ) -> SceneCompilationFailure {
        let (scope, slot) = match record {
            Record::Source { .. } => return failure,
            Record::Addition { scope, index } => {
                (scope, Some(PrefabOverlaySlot::Addition { index }))
            }
            Record::Set { scope, index } => (scope, Some(PrefabOverlaySlot::Component { index })),
        };
        self.scope_failure(scope, slot, failure)
    }

    fn register_scope(&mut self, scope: ScopeId) -> Result<(), SceneCompilationFailure> {
        self.register_scope_inner(scope)
            .map_err(|failure| self.scope_failure(scope, None, failure))
    }

    fn register_scope_inner(&mut self, scope: ScopeId) -> Result<(), SceneCompilationFailure> {
        let maximum = self.context.options.limits.working_bytes().get();
        let placement_cost = super::super::cost::scene_placements_cost(
            self.source_placements(scope),
            super::super::cost::PrefabOverlayCostLimits {
                nodes: maximum / 128,
                depth: self.context.materialization_limits().depth().get(),
                logical_bytes: maximum,
                ..super::super::cost::PrefabOverlayCostLimits::UNBOUNDED
            },
        )
        .map_err(|_| SceneCompilationFailureKind::WorkLimit)?;
        self.admit(
            placement_cost
                .nodes
                .checked_mul(128)
                .and_then(|bytes| bytes.checked_add(placement_cost.structural_logical_bytes)),
        )?;
        let entities = self.source_entities(scope);
        let count = entities.len();
        let additions = self
            .overlay(scope)
            .map_or(0, |overlay| overlay.additions.len());
        let bytes = entities
            .iter()
            .try_fold(size_of::<Scope>(), |bytes, entity| {
                bytes
                    .checked_add(512)?
                    .checked_add(provenance_string_bytes(entity.id.as_str())?.checked_mul(4)?)?
                    .checked_add(if scope == 0 && self.selected_anchor.is_some() {
                        0
                    } else {
                        entity
                            .components
                            .keys()
                            .try_fold(0usize, |bytes, component| {
                                bytes
                                    .checked_add(128)?
                                    .checked_add(provenance_string_bytes(component.as_str())?)
                            })?
                    })
            })
            .and_then(|bytes| bytes.checked_add(additions.checked_mul(512)?));
        self.admit(bytes)?;
        let entities = self.source_entities(scope);
        let structure = validate_source_structure(self.source_placements(scope), entities);
        if structure.has_errors() {
            self.context.diagnostics.extend(structure);
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        if let Some(overlay) = self.overlay(scope) {
            let maximum = self.context.options.limits.working_bytes().get();
            let limits = super::super::cost::PrefabOverlayCostLimits {
                nodes: maximum / 128,
                depth: self.context.materialization_limits().depth().get(),
                declarations: self
                    .context
                    .options
                    .limits
                    .overlay_declarations()
                    .get()
                    .saturating_sub(self.context.usage.overlay_declarations),
                logical_bytes: maximum,
                ..super::super::cost::PrefabOverlayCostLimits::UNBOUNDED
            };
            let anchor = self.scopes[scope]
                .anchor
                .map(|node| self.nodes[node].origin.entity().clone());
            let cost =
                super::super::cost::prefab_overlay_cost(overlay, limits).map_err(|error| {
                    let kind =
                        if error.kind == super::super::cost::PrefabOverlayCostKind::Declarations {
                            SceneCompilationBudgetKind::OverlayDeclarations
                        } else {
                            SceneCompilationBudgetKind::WorkingBytes
                        };
                    let (observed, maximum) =
                        if kind == SceneCompilationBudgetKind::OverlayDeclarations {
                            (
                                self.context
                                    .usage
                                    .overlay_declarations
                                    .saturating_add(error.observed),
                                self.context.options.limits.overlay_declarations().get(),
                            )
                        } else {
                            (error.observed, error.maximum)
                        };
                    self.context
                        .fail_budget(kind, observed, maximum, anchor.as_ref(), None);
                    SceneCompilationFailureKind::WorkLimit
                })?;
            let declarations = self
                .context
                .observe_item_budget(
                    SceneCompilationBudgetKind::OverlayDeclarations,
                    self.context.usage.overlay_declarations,
                    cost.declarations,
                    self.context.options.limits.overlay_declarations(),
                    anchor.as_ref(),
                    None,
                )
                .ok_or(SceneCompilationFailureKind::WorkLimit)?;
            self.context.usage.overlay_declarations = declarations;
            self.admit(
                cost.nodes
                    .checked_mul(128)
                    .and_then(|bytes| bytes.checked_add(cost.structural_logical_bytes)),
            )?;
            let overlay = self.overlay(scope).unwrap();
            let mut claimed = BTreeSet::new();
            for claim in &overlay.instances {
                if !claimed.insert(&claim.target)
                    || (claim.target.through.is_empty()
                        && matches!(claim.target.entity, PrefabEntityOrigin::InstanceAddition(_)))
                {
                    return Err(SceneCompilationFailureKind::DuplicateIntent.into());
                }
            }
        }
        for index in 0..count + additions {
            let (origin, record, slot) = if index < count {
                let entity = &self.source_entities(scope)[index];
                (
                    PrefabEntityOrigin::SourceEntity(entity.id.clone()),
                    Record::Source {
                        scope,
                        entity: index,
                    },
                    entity.prefab.as_ref().map(|_| Site {
                        source: scope,
                        entity: index,
                        segments: Vec::new(),
                    }),
                )
            } else {
                let index = index - count;
                let addition = &self.overlay(scope).unwrap().additions[index];
                let slot = addition.prefab.as_ref().map(|_| {
                    let mut site = self.scopes[scope].site.as_ref().unwrap().clone();
                    site.segments
                        .push(PrefabInstancePathSegment::Addition { index });
                    site
                });
                (
                    PrefabEntityOrigin::InstanceAddition(addition.id.clone()),
                    Record::Addition { scope, index },
                    slot,
                )
            };
            let node = self.nodes.len();
            let locator =
                SceneEntityId::new(format!("skeleton_{node}")).expect("internal locator is valid");
            if self.scopes[scope]
                .nodes
                .insert(origin.clone(), node)
                .is_some()
            {
                return Err(SceneCompilationFailureKind::DuplicateIntent.into());
            }
            self.locators.insert(locator.clone(), node);
            self.nodes.push(Node {
                scope,
                origin,
                record,
                locator,
                slot,
                entered: None,
                opened: false,
                programs: BTreeMap::new(),
            });
        }
        // These are unpublished layout seeds. Every directly owned source declaration must
        // supply its own desired placement; canonical record sorting is never authored order.
        self.scopes[scope].roots =
            (self.nodes.len() - count - additions..self.nodes.len()).collect();
        if scope == 0 {
            for entity in self.root.entities {
                if !self.context.reserve_provenance_bytes(
                    provenance_entry_bytes(&entity.id, &entity.id),
                    Some(&entity.id),
                    None,
                ) {
                    return Err(SceneCompilationFailureKind::WorkLimit.into());
                }
            }
        }
        Ok(())
    }

    /// Original typed ancestry is independent of physical placement and entity survival.
    fn relative(&self, scope: ScopeId, node: NodeId) -> Option<PrefabEntityTarget> {
        let mut through = Vec::new();
        let mut current = self.nodes[node].scope;
        while current != scope {
            let occurrence = &self.scopes[current];
            let anchor = occurrence.anchor?;
            through.push(PrefabInstanceHop {
                anchor: self.nodes[anchor].origin.clone(),
                expected_base: occurrence.source.as_ref()?.key().clone(),
            });
            current = self.nodes[anchor].scope;
        }
        through.reverse();
        Some(PrefabEntityTarget {
            through,
            entity: self.nodes[node].origin.clone(),
        })
    }

    fn relative_bytes(&self, scope: ScopeId, node: NodeId) -> Option<usize> {
        let mut bytes = size_of::<PrefabEntityTarget>().checked_add(provenance_string_bytes(
            self.nodes[node].origin.entity().as_str(),
        )?)?;
        let mut current = self.nodes[node].scope;
        while current != scope {
            let occurrence = &self.scopes[current];
            let anchor = occurrence.anchor?;
            bytes = bytes
                .checked_add(size_of::<PrefabInstanceHop>())?
                .checked_add(provenance_string_bytes(
                    self.nodes[anchor].origin.entity().as_str(),
                )?)?
                .checked_add(occurrence.source.as_ref()?.key().checked_retained_bytes()?)?;
            current = self.nodes[anchor].scope;
        }
        Some(bytes)
    }

    fn target_index_bytes(&self, scope: ScopeId) -> Option<usize> {
        self.scopes[scope]
            .output
            .iter()
            .try_fold(0usize, |bytes, &node| {
                bytes
                    .checked_add(self.relative_bytes(scope, node)?)?
                    .checked_add(provenance_string_bytes(self.nodes[node].locator.as_str())?)?
                    .checked_add(128)
            })
    }

    fn state(
        &mut self,
        node: NodeId,
        site: &Site,
        _key: &SourceDocumentKey,
        reference: Option<(&nara_identity::AuthoredEntityReference, usize, ScopeId)>,
    ) -> Result<State, SceneCompilationFailure> {
        // Only work which can still enter this slot distinguishes a finite re-entry.
        // Stable source positions, rather than arena addresses, detect actual recursion.
        self.admit(
            self.site_bytes(site)
                .and_then(|bytes| bytes.checked_add(size_of::<State>())),
        )?;
        let overlay = self
            .instance(site)
            .and_then(|instance| instance.overrides.as_ref())
            .map(|_| self.stable_site(site));
        let mut demands = Vec::new();
        let mut current = self.nodes[node].scope;
        loop {
            if self
                .bound_root_target(current)
                .is_some_and(|root| !root.through.is_empty())
            {
                self.admit(
                    self.site_bytes(self.scopes[current].site.as_ref().unwrap())
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<DemandSite>() + 3 * size_of::<usize>())
                        })
                        .and_then(|bytes| {
                            bytes.checked_add(self.relative_bytes(current, node)?.checked_mul(2)?)
                        }),
                )?;
                let relative = self
                    .relative(current, node)
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
                let prefix = relative
                    .through
                    .iter()
                    .map(|hop| &hop.anchor)
                    .chain(std::iter::once(&relative.entity))
                    .collect::<Vec<_>>();
                let target = self.bound_root_target(current).unwrap();
                if target.through.len() >= prefix.len()
                    && target
                        .through
                        .iter()
                        .zip(&prefix)
                        .all(|(hop, origin)| &hop.anchor == *origin)
                {
                    demands.push((
                        DemandSite::RootBinding(
                            self.stable_site(self.scopes[current].site.as_ref().unwrap()),
                        ),
                        0,
                        target.through.len() - prefix.len(),
                    ));
                }
            }
            if let Some(layer) = self.overlay(current) {
                let mut count = 0usize;
                structural_targets(layer, |_, _| count = count.saturating_add(1));
                let declaration_bytes =
                    self.site_bytes(self.scopes[current].site.as_ref().unwrap());
                let bytes = declaration_bytes
                    .and_then(|bytes| {
                        bytes
                            .checked_add(3 * size_of::<usize>())?
                            .checked_mul(count.checked_add(1)?)
                    })
                    .and_then(|bytes| {
                        bytes.checked_add(self.relative_bytes(current, node)?.checked_mul(2)?)
                    });
                self.admit(bytes)?;
                let layer = self.overlay(current).unwrap();
                let relative = self
                    .relative(current, node)
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
                let prefix = relative
                    .through
                    .iter()
                    .map(|hop| &hop.anchor)
                    .chain(std::iter::once(&relative.entity))
                    .collect::<Vec<_>>();
                let declaration = self.stable_site(self.scopes[current].site.as_ref().unwrap());
                let mut index = 0;
                structural_targets(layer, |_, target| {
                    if target.through.len() >= prefix.len()
                        && target
                            .through
                            .iter()
                            .zip(&prefix)
                            .all(|(hop, origin)| &hop.anchor == *origin)
                    {
                        demands.push((
                            DemandSite::Overlay(declaration.clone()),
                            index,
                            target.through.len() - prefix.len(),
                        ));
                    }
                    index += 1;
                });
            }
            let placements = self.source_placements(current);
            let mut count = 0usize;
            placement_targets(placements, |_, _| count = count.saturating_add(1));
            let source_bytes = self.scopes[current]
                .source
                .as_ref()
                .map_or(Some(0), |source| source.key().checked_retained_bytes());
            self.admit(
                source_bytes
                    .and_then(|bytes| {
                        bytes.checked_add(size_of::<DemandSite>() + 3 * size_of::<usize>())
                    })
                    .and_then(|bytes| bytes.checked_mul(count.checked_add(1)?))
                    .and_then(|bytes| {
                        bytes.checked_add(self.relative_bytes(current, node)?.checked_mul(2)?)
                    }),
            )?;
            let declaration = DemandSite::Source(
                self.scopes[current]
                    .source
                    .as_ref()
                    .map_or(PrefabDeclarationSource::Containing, |source| {
                        PrefabDeclarationSource::Prefab(source.key().clone())
                    }),
            );
            let relative = self
                .relative(current, node)
                .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
            let prefix = relative
                .through
                .iter()
                .map(|hop| &hop.anchor)
                .chain(std::iter::once(&relative.entity))
                .collect::<Vec<_>>();
            placement_targets(self.source_placements(current), |index, target| {
                if target.through.len() >= prefix.len()
                    && target
                        .through
                        .iter()
                        .zip(&prefix)
                        .all(|(hop, origin)| &hop.anchor == *origin)
                {
                    demands.push((
                        declaration.clone(),
                        index,
                        target.through.len() - prefix.len(),
                    ));
                }
            });
            let Some(anchor) = self.scopes[current].anchor else {
                break;
            };
            current = self.nodes[anchor].scope;
        }
        let reference = if let Some((reference, remaining_hops, lexical_scope)) = reference {
            self.admit(
                inputs::authored_clone_bytes(reference)
                    .and_then(|bytes| bytes.checked_add(size_of::<ReferenceDemand>()))
                    .and_then(|bytes| {
                        bytes.checked_add(
                            self.scopes[lexical_scope]
                                .source
                                .as_ref()
                                .map_or(Some(0), |source| source.key().checked_retained_bytes())?,
                        )
                    }),
            )?;
            let lexical_source = self.scopes[lexical_scope]
                .source
                .as_ref()
                .map_or(PrefabDeclarationSource::Containing, |source| {
                    PrefabDeclarationSource::Prefab(source.key().clone())
                });
            Some(ReferenceDemand {
                reference: reference.clone(),
                remaining_hops,
                lexical_source,
            })
        } else {
            None
        };
        Ok(State {
            overlay,
            demands,
            reference,
        })
    }

    fn reserve_occurrence(
        &mut self,
        node: NodeId,
        request: &AssetRef,
        key: &SourceDocumentKey,
    ) -> Result<(), SceneCompilationFailure> {
        self.context
            .reserve_provenance_bytes(
                size_of::<AssetRef>()
                    .checked_add(size_of::<SourceDocumentKey>())
                    .and_then(|bytes| bytes.checked_add(provenance_asset_ref_bytes(request)?))
                    .and_then(|bytes| bytes.checked_add(key.checked_retained_bytes()?)),
                Some(self.nodes[node].origin.entity()),
                Some(request),
            )
            .then_some(())
            .ok_or_else(|| SceneCompilationFailureKind::WorkLimit.into())
    }

    fn select_site(&mut self, node: NodeId) -> Result<Option<Site>, SceneCompilationFailure> {
        let mut site = self.nodes[node].slot.clone();
        let mut current = self.nodes[node].scope;
        loop {
            if let Some(overlay) = self
                .overlay(current)
                .filter(|overlay| !overlay.instances.is_empty())
            {
                let target = self
                    .relative(current, node)
                    .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
                let mut matched = false;
                for (index, claim) in overlay.instances.iter().enumerate() {
                    if claim
                        .target
                        .through
                        .iter()
                        .map(|hop| &hop.anchor)
                        .eq(target.through.iter().map(|hop| &hop.anchor))
                        && claim.target.entity == target.entity
                    {
                        if matched {
                            return Err(self.scope_failure(
                                current,
                                Some(PrefabOverlaySlot::InstanceClaim { index }),
                                SceneCompilationFailureKind::DuplicateIntent.into(),
                            ));
                        }
                        if claim.target != target {
                            return Err(self.scope_failure(
                                current,
                                Some(PrefabOverlaySlot::InstanceClaim { index }),
                                SceneCompilationFailureKind::SourceChanged.into(),
                            ));
                        }
                        matched = true;
                        site = claim.value.as_ref().map(|_| {
                            let mut site = self.scopes[current].site.as_ref().unwrap().clone();
                            site.segments
                                .push(PrefabInstancePathSegment::InstanceClaim { index });
                            site
                        });
                    }
                }
            }
            let Some(anchor) = self.scopes[current].anchor else {
                break;
            };
            current = self.nodes[anchor].scope;
        }
        Ok(site)
    }

    fn open_slot(&mut self, node: NodeId) -> Result<Option<ScopeId>, SceneCompilationFailure> {
        self.open_slot_with_reference(node, None)
    }

    fn open_reference_slot(
        &mut self,
        node: NodeId,
        reference: &nara_identity::AuthoredEntityReference,
        remaining_hops: usize,
        lexical_scope: ScopeId,
    ) -> Result<Option<ScopeId>, SceneCompilationFailure> {
        self.open_slot_with_reference(node, Some((reference, remaining_hops, lexical_scope)))
    }

    fn open_slot_with_reference(
        &mut self,
        node: NodeId,
        reference: Option<(&nara_identity::AuthoredEntityReference, usize, ScopeId)>,
    ) -> Result<Option<ScopeId>, SceneCompilationFailure> {
        if self.nodes[node].opened {
            return Ok(self.nodes[node].entered);
        }
        let mut selection_bytes = Some(size_of::<Site>());
        let mut current = self.nodes[node].scope;
        loop {
            if self
                .overlay(current)
                .is_some_and(|overlay| !overlay.instances.is_empty())
            {
                selection_bytes = selection_bytes.and_then(|bytes| {
                    bytes
                        .checked_add(self.relative_bytes(current, node)?)?
                        .checked_add(
                            self.scopes[current]
                                .site
                                .as_ref()
                                .map_or(Some(0), |site| self.site_bytes(site))?,
                        )
                });
            }
            let Some(anchor) = self.scopes[current].anchor else {
                break;
            };
            current = self.nodes[anchor].scope;
        }
        self.admit(selection_bytes)?;
        let Some(site) = self.select_site(node)? else {
            self.nodes[node].opened = true;
            return Ok(None);
        };
        self.open_selected_slot(node, &site, reference)
            .map_err(|failure| self.site_failure(&site, None, failure))
    }

    fn open_selected_slot(
        &mut self,
        node: NodeId,
        site: &Site,
        reference: Option<(&nara_identity::AuthoredEntityReference, usize, ScopeId)>,
    ) -> Result<Option<ScopeId>, SceneCompilationFailure> {
        self.admit(Some(512))?;
        let instance = self
            .instance(&site)
            .ok_or(SceneCompilationFailureKind::OwnerAdmission)?;
        if !instance.source.is_primary() {
            let diagnostic = with_asset_ref(
                diagnostic_error(
                    "scene.prefab-source-kind-invalid",
                    "Auxiliary imported products cannot select a native Prefab source",
                ),
                "asset-ref",
                &instance.source,
            );
            self.context.diagnostics.push(diagnostic);
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        self.admit(
            provenance_asset_ref_bytes(&instance.source)
                .and_then(|bytes| bytes.checked_add(size_of::<AssetRef>())),
        )?;
        let request = self.instance(&site).unwrap().source.clone();
        let depth = self.scopes[self.nodes[node].scope].depth + 1;
        let anchor = self.nodes[node].origin.entity().clone();
        if depth > self.context.options.max_depth {
            self.context.diagnostics.push(with_asset_ref(
                with_public_locator(
                    with_public_u64(
                        with_public_u64(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.prefab-depth-exceeded",
                                    "Prefab expansion depth limit was exceeded",
                                ),
                                "entity-id",
                                anchor.as_str(),
                            ),
                            "current-depth",
                            usize_to_u64(depth - 1),
                        ),
                        "maximum-depth",
                        usize_to_u64(self.context.options.max_depth),
                    ),
                    "field-path",
                    "prefab.source",
                ),
                "asset-ref",
                &request,
            ));
            return Err(SceneCompilationFailureKind::WorkLimit.into());
        }
        if !self.context.reserve_resolved_instance(&anchor, &request) {
            return Err(SceneCompilationFailureKind::WorkLimit.into());
        }
        let mut working = self.context.working_set();
        working.source_depth = depth;
        let resolved = (self.context.resolver)(&request, &mut working);
        self.context.options.limits = self
            .context
            .options
            .limits
            .with_working_bytes(working.working_limit)
            .with_reference_work_nodes(working.reference_limits.0)
            .with_reference_work_bytes(working.reference_limits.1);
        let source = match resolved {
            Ok(source) => source,
            Err(error) => {
                self.context.load_error = Some(error);
                return Err(SceneCompilationFailureKind::OwnerAdmission.into());
            }
        };
        if !self
            .context
            .admit_working_usage(self.context.usage, Some(&anchor), Some(&request))
            || self.context.admit_terminal_work(0, 0, Some(0)).is_err()
        {
            return Err(SceneCompilationFailureKind::WorkLimit.into());
        }
        let Some(source) = source else {
            self.context.diagnostics.push(with_asset_ref(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.prefab-source-missing",
                            "Prefab source could not be resolved",
                        ),
                        "entity-id",
                        anchor.as_str(),
                    ),
                    "field-path",
                    "prefab.source",
                ),
                "asset-ref",
                &request,
            ));
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        };
        if source.key().kind() != SourceDocumentKind::Prefab {
            self.context.diagnostics.push(with_asset_ref(
                diagnostic_error(
                    "scene.prefab-source-kind-invalid",
                    "Resolved source does not describe a Prefab",
                ),
                "asset-ref",
                &request,
            ));
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        if !source.matches_request(&request) {
            self.context.diagnostics.push(with_asset_ref(
                diagnostic_error(
                    "scene.prefab-source-identity-invalid",
                    "Primary product selection requires a matching admitted Prefab source association",
                ),
                "asset-ref",
                &request,
            ));
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        if !source.matches_authority(self.context.authority) {
            return Err(SceneCompilationFailureKind::OwnerAdmission.into());
        }
        if self
            .instance(&site)
            .unwrap()
            .overrides
            .as_ref()
            .is_some_and(|overlay| &overlay.base != source.key())
        {
            return Err(SceneCompilationFailureKind::SourceChanged.into());
        }
        if let crate::PrefabInstanceBinding::Root { base, .. } =
            &self.instance(&site).unwrap().binding
            && base != source.key()
        {
            return Err(SceneCompilationFailureKind::SourceChanged.into());
        }
        self.admit_input_arguments(site, &source)?;
        self.reserve_occurrence(node, &request, source.key())?;
        let state = self.state(node, &site, source.key(), reference)?;
        let mut ancestor = self.nodes[node].scope;
        while let Some(parent) = self.scopes[ancestor].anchor {
            let scope = &self.scopes[ancestor];
            if scope
                .source
                .as_ref()
                .is_some_and(|previous| previous.key() == source.key())
                && scope.state.as_ref() == Some(&state)
            {
                let mut diagnostic = with_asset_ref(
                    with_public_locator(
                        with_public_u64(
                            with_public_u64(
                                diagnostic_error(
                                    "scene.prefab-cycle",
                                    "Prefab source cycle was detected",
                                ),
                                "cycle-start-index",
                                usize_to_u64(scope.depth - 1),
                            ),
                            "cycle-depth",
                            usize_to_u64(depth - scope.depth + 1),
                        ),
                        "field-path",
                        "prefab.source",
                    ),
                    "cycle-to",
                    &request,
                );
                diagnostic = with_public_locator(diagnostic, "entity-id", anchor.as_str());
                if let Some(previous) = self.scopes[self.nodes[node].scope].request.as_ref() {
                    diagnostic = with_asset_ref(diagnostic, "cycle-from", previous);
                }
                self.context.diagnostics.push(diagnostic);
                return Err(SceneCompilationFailureKind::OwnerAdmission.into());
            }
            ancestor = self.nodes[parent].scope;
        }
        let scope = self.scopes.len();
        self.admit(
            site.segments
                .len()
                .checked_mul(size_of::<PrefabInstancePathSegment>())
                .and_then(|bytes| bytes.checked_add(size_of::<Site>())),
        )?;
        self.scopes.push(Scope {
            source: Some(source),
            request: Some(request),
            site: Some(site.clone()),
            anchor: Some(node),
            depth,
            nodes: ScopeNodes::default(),
            roots: Vec::new(),
            aliases: BTreeMap::new(),
            state: Some(state),
            input: BTreeSet::new(),
            output: BTreeSet::new(),
        });
        self.nodes[node].entered = Some(scope);
        self.nodes[node].opened = true;
        self.register_scope(scope)?;
        Ok(Some(scope))
    }

    fn resolve(
        &mut self,
        scope: ScopeId,
        target: &PrefabEntityTarget,
    ) -> Result<NodeId, SceneCompilationFailure> {
        let mut current = scope;
        for hop in &target.through {
            let anchor = *self.scopes[current]
                .nodes
                .get(&hop.anchor)
                .ok_or(SceneCompilationFailureKind::MissingTarget)?;
            current = self
                .open_slot(anchor)?
                .ok_or(SceneCompilationFailureKind::MissingTarget)?;
            if self.scopes[current].source.as_ref().unwrap().key() != &hop.expected_base {
                return Err(SceneCompilationFailureKind::SourceChanged.into());
            }
        }
        self.scopes[current]
            .nodes
            .get(&target.entity)
            .copied()
            .ok_or_else(|| SceneCompilationFailureKind::MissingTarget.into())
    }

    fn close_demands(&mut self, scanned: &mut usize) -> Result<(), SceneCompilationFailure> {
        while *scanned < self.scopes.len() {
            let scope = *scanned;
            *scanned += 1;
            // A direct missing root needs only borrowed layout lookup. A routed root also
            // demands the exact nested slots, even if no surviving payload refers to them.
            if let Some(target) = self
                .bound_root_target(scope)
                .filter(|target| !target.through.is_empty())
            {
                self.admit(target_clone_bytes(target))?;
                let target = self.bound_root_target(scope).unwrap().clone();
                self.resolve(scope, &target).map_err(|mut failure| {
                    if failure.reason == SceneCompilationFailureKind::MissingTarget {
                        failure.reason = SceneCompilationFailureKind::RootBinding;
                    }
                    self.scope_failure(scope, None, failure)
                })?;
            }
            let mut bytes = Some(0usize);
            placement_targets(self.source_placements(scope), |_, target| {
                bytes = bytes.and_then(|bytes| {
                    bytes
                        .checked_add(target_clone_bytes(target)?)?
                        .checked_add(size_of::<Option<PrefabOverlaySlot>>())
                });
            });
            if let Some(overlay) = self.overlay(scope) {
                structural_targets(overlay, |_, target| {
                    bytes = bytes.and_then(|bytes| {
                        bytes
                            .checked_add(target_clone_bytes(target)?)?
                            .checked_add(size_of::<Option<PrefabOverlaySlot>>())
                    });
                });
            }
            self.admit(bytes)?;
            let mut targets = Vec::new();
            placement_targets(self.source_placements(scope), |_, target| {
                targets.push((None, target.clone()))
            });
            if let Some(overlay) = self.overlay(scope) {
                structural_targets(overlay, |slot, target| {
                    targets.push((Some(slot), target.clone()))
                });
            }
            for (slot, target) in targets {
                self.resolve(scope, &target)
                    .map_err(|failure| self.scope_failure(scope, slot, failure))?;
            }
        }
        Ok(())
    }

    fn close_structure(
        &mut self,
        scanned: &mut usize,
    ) -> Result<SceneDraft, SceneCompilationFailure> {
        #[cfg(feature = "work-observation")]
        let _work_span =
            tracing::info_span!(target: "nara::scene::work", "scene_structure").entered();
        let document = loop {
            self.close_demands(scanned)?;
            self.postorder.clear();
            let document = self.layout_scope(0)?;
            self.admit(self.scopes[0].output.len().checked_mul(size_of::<NodeId>()))?;
            let pending = self.scopes[0]
                .output
                .iter()
                .copied()
                .filter(|&node| {
                    !self.nodes[node].opened
                        && (self.selected_anchor == Some(node) || self.selects_values(node))
                })
                .collect::<Vec<_>>();
            let scope_count = self.scopes.len();
            for node in pending {
                self.open_slot(node)?;
            }
            if self.scopes.len() == scope_count {
                break document;
            }
        };
        // Source facets and materialized entities differ under root binding. Their
        // skeletons have already paid working-storage admission; this public count
        // bounds the actual surviving hierarchy instead of inventing wrapper entities.
        let count = self
            .context
            .observe_item_budget(
                SceneCompilationBudgetKind::MaterializedEntities,
                0,
                document.entities.len(),
                self.context.options.limits.materialized_entities(),
                None,
                None,
            )
            .ok_or(SceneCompilationFailureKind::WorkLimit)?;
        let mut usage = self.context.usage;
        usage.materialized_entities = count;
        if !self.context.admit_working_usage(usage, None, None) {
            return Err(SceneCompilationFailureKind::WorkLimit.into());
        }
        self.context.usage = usage;
        Ok(document)
    }

    /// The declaring layer sees its own output, even if an outer layer later removes it.
    /// Callers admit the bounded target-index allocation before constructing this map.
    fn target_nodes(
        &self,
        scope: ScopeId,
    ) -> Result<BTreeMap<PrefabEntityTarget, SceneEntityId>, SceneCompilationFailure> {
        self.scopes[scope]
            .output
            .iter()
            .map(|&node| {
                self.relative(scope, node)
                    .map(|target| {
                        (
                            target,
                            self.nodes[self.canonical_node(scope, node)].locator.clone(),
                        )
                    })
                    .ok_or_else(|| SceneCompilationFailureKind::OwnerAdmission.into())
            })
            .collect()
    }

    fn compose_programs(&mut self) -> Result<(), SceneCompilationFailure> {
        let bytes = self
            .nodes
            .iter()
            .enumerate()
            .try_fold(0usize, |bytes, (id, node)| {
                if !self.selects_values(id) {
                    return Some(bytes);
                }
                bytes.checked_add(self.components(node.record).len().checked_mul(256)?)
            });
        self.admit(bytes)?;
        for node in 0..self.nodes.len() {
            if !self.selects_values(node) {
                continue;
            }
            let record = self.nodes[node].record;
            self.nodes[node].programs = self
                .components(record)
                .keys()
                .cloned()
                .map(|component| {
                    (
                        component,
                        Program {
                            record,
                            properties: Vec::new(),
                        },
                    )
                })
                .collect();
        }
        for ordinal in 0..self.postorder.len() {
            let scope = self.postorder[ordinal];
            self.inherit_bound_root_programs(scope)?;
            self.compose_scope_programs(scope)
                .map_err(|failure| self.scope_failure(scope, None, failure))?;
        }
        Ok(())
    }

    fn inherit_bound_root_programs(
        &mut self,
        scope: ScopeId,
    ) -> Result<(), SceneCompilationFailure> {
        self.admit(
            self.scopes[scope]
                .nodes
                .len()
                .checked_mul(size_of::<NodeId>()),
        )?;
        let local = self.scopes[scope]
            .nodes
            .values()
            .copied()
            .collect::<Vec<_>>();
        for anchor in local {
            let Some(child) = self.nodes[anchor].entered else {
                continue;
            };
            let Some(root) = self.bound_root_node(child)? else {
                continue;
            };
            if !self.selects_values(anchor) && !self.selects_values(root) {
                continue;
            }
            // The root's full inner program already includes its selected instance intent.
            // Containing-anchor records then replace complete components without changing the
            // lexical environment retained by either contributor's program.
            let local = std::mem::take(&mut self.nodes[anchor].programs);
            let mut inherited = std::mem::take(&mut self.nodes[root].programs);
            inherited.extend(local);
            self.nodes[anchor].programs = inherited;
        }
        Ok(())
    }

    fn compose_scope_programs(&mut self, scope: ScopeId) -> Result<(), SceneCompilationFailure> {
        // A scope opened only for a reference route or input environment contributes no
        // selected component values. Its layout has already passed structural composition.
        if !self.scopes[scope]
            .output
            .iter()
            .any(|&node| self.selects_values(node))
        {
            return Ok(());
        }
        let Some(overlay) = self.overlay(scope) else {
            return Ok(());
        };
        let count = overlay.components.len();
        let output = self.scopes[scope].output.len();
        self.admit(
            self.target_index_bytes(scope)
                .and_then(|bytes| bytes.checked_add(output.checked_mul(128)?))
                .and_then(|bytes| bytes.checked_add(count.checked_mul(256)?)),
        )?;
        let targets = self.target_nodes(scope)?;
        let locators = self.scopes[scope]
            .output
            .iter()
            .map(|&node| {
                let node = self.canonical_node(scope, node);
                (&self.nodes[node].locator, node)
            })
            .collect::<BTreeMap<_, _>>();
        let overlay = self.overlay(scope).unwrap();
        let mut touched = BTreeSet::new();
        let mut intents = Vec::with_capacity(count);
        for (index, intent) in overlay.components.iter().enumerate() {
            let (target, component) = match intent {
                PrefabComponentIntent::Set {
                    target, component, ..
                }
                | PrefabComponentIntent::Suppress { target, component }
                | PrefabComponentIntent::Properties {
                    target, component, ..
                } => (target, component),
            };
            if target.through.is_empty()
                && matches!(target.entity, PrefabEntityOrigin::InstanceAddition(_))
            {
                return Err(self.scope_failure(
                    scope,
                    Some(PrefabOverlaySlot::Component { index }),
                    SceneCompilationFailureKind::DuplicateIntent.into(),
                ));
            }
            let Some(node) = targets
                .get(target)
                .and_then(|locator| locators.get(locator))
                .copied()
            else {
                return Err(self.scope_failure(
                    scope,
                    Some(PrefabOverlaySlot::Component { index }),
                    SceneCompilationFailureKind::MissingTarget.into(),
                ));
            };
            if !self.selects_values(node) {
                continue;
            }
            if !touched.insert((node, component.clone())) {
                return Err(self.scope_failure(
                    scope,
                    Some(PrefabOverlaySlot::Component { index }),
                    SceneCompilationFailureKind::DuplicateIntent.into(),
                ));
            }
            intents.push((node, component.clone(), index));
        }
        for (node, component, index) in intents {
            let exists = self.nodes[node].programs.contains_key(&component);
            match &self.overlay(scope).unwrap().components[index] {
                PrefabComponentIntent::Set { mode, .. } => {
                    if (*mode == PrefabComponentSetMode::Add) == exists {
                        return Err(self.scope_failure(
                            scope,
                            Some(PrefabOverlaySlot::Component { index }),
                            SceneCompilationFailureKind::Component.into(),
                        ));
                    }
                    self.nodes[node].programs.insert(
                        component,
                        Program {
                            record: Record::Set { scope, index },
                            properties: Vec::new(),
                        },
                    );
                }
                PrefabComponentIntent::Suppress { .. } => {
                    if !exists {
                        return Err(self.scope_failure(
                            scope,
                            Some(PrefabOverlaySlot::Component { index }),
                            SceneCompilationFailureKind::Component.into(),
                        ));
                    }
                    self.nodes[node].programs.remove(&component);
                }
                PrefabComponentIntent::Properties { claims, .. } => {
                    self.admit(claims.len().checked_mul(size_of::<usize>()))?;
                    let PrefabComponentIntent::Properties { claims, .. } =
                        &self.overlay(scope).unwrap().components[index]
                    else {
                        unreachable!("the declaration kind does not change");
                    };
                    super::super::properties::validate_claim_paths(claims).map_err(|reason| {
                        self.scope_failure(
                            scope,
                            Some(PrefabOverlaySlot::Component { index }),
                            reason.into(),
                        )
                    })?;
                    if !self.nodes[node].programs.contains_key(&component) {
                        return Err(self.scope_failure(
                            scope,
                            Some(PrefabOverlaySlot::Component { index }),
                            SceneCompilationFailureKind::Component.into(),
                        ));
                    }
                    let program = self.nodes[node]
                        .programs
                        .get_mut(&component)
                        .expect("the component program was checked above");
                    program.properties.push((scope, index));
                }
            }
        }

        Ok(())
    }

    fn layout_scope(&mut self, scope: ScopeId) -> Result<SceneDraft, SceneCompilationFailure> {
        self.layout_scope_inner(scope)
            .map_err(|failure| self.scope_failure(scope, None, failure))
    }

    fn layout_failure(&mut self, mut failure: SceneCompilationFailure) -> SceneCompilationFailure {
        if let Some(entity) = failure.entity.as_ref() {
            let Some(&node) = self.locators.get(entity) else {
                return SceneCompilationFailureKind::OwnerAdmission.into();
            };
            if self
                .admit(provenance_string_bytes(
                    self.nodes[node].origin.entity().as_str(),
                ))
                .is_err()
            {
                return SceneCompilationFailureKind::WorkLimit.into();
            }
            failure.entity = Some(self.nodes[node].origin.entity().clone());
        }
        failure
    }

    fn layout_scope_inner(
        &mut self,
        scope: ScopeId,
    ) -> Result<SceneDraft, SceneCompilationFailure> {
        self.admit(self.scopes[scope].nodes.len().checked_mul(1024))?;
        let local = self.scopes[scope]
            .nodes
            .values()
            .copied()
            .collect::<Vec<_>>();
        self.scopes[scope].aliases.clear();
        self.scopes[scope].input.clear();
        let mut document = SceneDraft {
            roots: self.scopes[scope]
                .roots
                .iter()
                .map(|node| self.nodes[*node].locator.clone())
                .collect(),
            entities: Vec::new(),
        };
        for node in local {
            let mut record = ExpandedSceneEntity::new(self.nodes[node].locator.clone());
            self.scopes[scope].input.insert(node);
            if let Some(child) = self.nodes[node].entered {
                let mut fragment = self.layout_scope(child)?;
                self.admit(self.scopes[child].output.len().checked_mul(512))?;
                let bound_root = match self.bound_root_node(child)? {
                    None => None,
                    Some(root) => {
                        if fragment.roots.len() != 1
                            || fragment.roots.first() != Some(&self.nodes[root].locator)
                        {
                            return Err(self.scope_failure(
                                child,
                                None,
                                SceneCompilationFailureKind::RootBinding.into(),
                            ));
                        }
                        Some(root)
                    }
                };
                if let Some(root) = bound_root {
                    let index = fragment
                        .entities
                        .iter()
                        .position(|entity| entity.id == self.nodes[root].locator)
                        .ok_or(SceneCompilationFailureKind::RootBinding)?;
                    let root_record = fragment.entities.remove(index);
                    record.children = root_record.children;
                    self.scopes[scope].aliases.insert(root, node);
                } else {
                    record.children = fragment.roots;
                }
                // A chain of root bindings keeps every declaration address while normalizing
                // all materialized aliases to the outermost anchor in this source's layout.
                let facets = self.scopes[child]
                    .output
                    .iter()
                    .map(|&facet| (facet, self.canonical_node(child, facet)))
                    .collect::<Vec<_>>();
                for (facet, canonical) in facets {
                    let canonical = if Some(canonical) == bound_root {
                        node
                    } else {
                        canonical
                    };
                    if canonical != facet {
                        self.scopes[scope].aliases.insert(facet, canonical);
                    }
                    self.scopes[scope].input.insert(facet);
                }
                document.entities.extend(fragment.entities);
            }
            document.entities.push(record);
        }
        let bytes = self.scopes[scope]
            .input
            .iter()
            .try_fold(0usize, |bytes, &node| {
                bytes
                    .checked_add(self.relative_bytes(scope, node)?)?
                    .checked_add(provenance_string_bytes(
                        self.nodes[self.canonical_node(scope, node)]
                            .locator
                            .as_str(),
                    )?)?
                    .checked_add(1024)
            });
        self.admit(bytes)?;
        let mut targets = BTreeMap::new();
        for &node in &self.scopes[scope].input {
            if let Some(target) = self.relative(scope, node) {
                targets.insert(
                    target,
                    self.nodes[self.canonical_node(scope, node)].locator.clone(),
                );
            }
        }
        // Both owners use one desired-layout kernel. The source's own declarations can place
        // local children among inherited siblings before the containing instance's intent.
        layout::apply_layout(
            &mut document,
            &mut targets,
            self.source_placements(scope),
            None,
        )
        .map_err(|failure| self.layout_failure(failure))?;
        if let Some(overlay) = self.overlay(scope) {
            layout::apply_layout(
                &mut document,
                &mut targets,
                &overlay.placements,
                Some(overlay),
            )
            .map_err(|failure| self.layout_failure(failure))?;
        }
        let membership = document
            .entities
            .iter()
            .map(|record| &record.id)
            .collect::<BTreeSet<_>>();
        self.scopes[scope].output = self.scopes[scope]
            .input
            .iter()
            .copied()
            .filter(|node| {
                membership.contains(&self.nodes[self.canonical_node(scope, *node)].locator)
            })
            .collect();
        self.postorder.push(scope);
        Ok(document)
    }
}

fn structural_targets(
    overlay: &PrefabOverlay,
    mut visit: impl FnMut(PrefabOverlaySlot, &PrefabEntityTarget),
) {
    for (index, claim) in overlay.instances.iter().enumerate() {
        visit(PrefabOverlaySlot::InstanceClaim { index }, &claim.target);
    }
    for (index, target) in overlay.suppressions.iter().enumerate() {
        visit(PrefabOverlaySlot::Suppression { index }, target);
    }
    placement_targets(&overlay.placements, |index, target| {
        visit(PrefabOverlaySlot::Placement { index }, target)
    });
    for (index, component) in overlay.components.iter().enumerate() {
        match component {
            PrefabComponentIntent::Set { target, .. }
            | PrefabComponentIntent::Suppress { target, .. }
            | PrefabComponentIntent::Properties { target, .. } => {
                visit(PrefabOverlaySlot::Component { index }, target)
            }
        }
    }
}

fn placement_targets(
    placements: &[SceneEntityPlacement],
    mut visit: impl FnMut(usize, &PrefabEntityTarget),
) {
    for (index, declaration) in placements.iter().enumerate() {
        visit(index, &declaration.target);
        if let ScenePlacementParent::Entity(parent) = &declaration.placement.parent {
            visit(index, parent);
        }
        if let ScenePlacementSlot::Before(sibling) | ScenePlacementSlot::After(sibling) =
            &declaration.placement.slot
        {
            visit(index, sibling);
        }
    }
}

fn target_clone_bytes(target: &PrefabEntityTarget) -> Option<usize> {
    let bytes = size_of::<PrefabEntityTarget>()
        .checked_add(provenance_string_bytes(target.entity.entity().as_str())?)?;
    target.through.iter().try_fold(bytes, |bytes, hop| {
        bytes
            .checked_add(size_of::<PrefabInstanceHop>())?
            .checked_add(provenance_string_bytes(hop.anchor.entity().as_str())?)?
            .checked_add(hop.expected_base.checked_retained_bytes()?)
    })
}
