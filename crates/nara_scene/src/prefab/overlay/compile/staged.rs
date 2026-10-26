//! Source skeletons precede layered layout and selected component interpretation.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_reflect::ComponentTypeId;

use super::*;
use crate::SceneComponentRecord;
use crate::prefab::PrefabExpansionWorkingSet;
use crate::provenance::{provenance_asset_ref_bytes, provenance_string_bytes};

mod output;
mod values;

type NodeId = usize;
type ScopeId = usize;

#[derive(Debug, Clone, Copy)]
enum Segment {
    Addition(usize),
    Claim(usize),
}

/// Locates a declaration in an immutable input, never in a growing arena allocation.
#[derive(Debug, Clone)]
struct Site {
    source: ScopeId,
    entity: usize,
    segments: Vec<Segment>,
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
    overlay: Option<DeclarationAddress>,
    demands: Vec<(DeclarationAddress, usize, usize)>,
}

struct Node {
    scope: ScopeId,
    origin: PrefabEntityOrigin,
    record: Record,
    locator: SceneEntityId,
    children: Vec<NodeId>,
    slot: Option<Site>,
    entered: Option<ScopeId>,
    opened: bool,
    programs: BTreeMap<ComponentTypeId, Program>,
}

struct Scope {
    source: Option<ResolvedPrefabSource>,
    request: Option<AssetRef>,
    site: Option<Site>,
    anchor: Option<NodeId>,
    depth: usize,
    nodes: BTreeMap<PrefabEntityOrigin, NodeId>,
    roots: Vec<NodeId>,
    state: Option<State>,
    input: BTreeSet<NodeId>,
    output: BTreeSet<NodeId>,
}

struct Compiler<'root, 'context, 'owner, F, E> {
    root: &'root SceneDocument,
    context: &'context mut PrefabExpansionContext<'owner, F, E>,
    scopes: Vec<Scope>,
    nodes: Vec<Node>,
    locators: BTreeMap<SceneEntityId, NodeId>,
    value_ids: BTreeMap<SceneEntityId, SceneEntityId>,
    postorder: Vec<ScopeId>,
}

pub(super) fn expand<F, E>(
    context: &mut PrefabExpansionContext<'_, F, E>,
    document: &SceneDocument,
) -> Result<SceneDocument, PrefabOverlayConflict>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let mut compiler = Compiler {
        root: document,
        context,
        scopes: Vec::new(),
        nodes: Vec::new(),
        locators: BTreeMap::new(),
        value_ids: BTreeMap::new(),
        postorder: Vec::new(),
    };
    compiler.admit(Some(size_of::<Scope>()))?;
    compiler.scopes.push(Scope {
        source: None,
        request: None,
        site: None,
        anchor: None,
        depth: 0,
        nodes: BTreeMap::new(),
        roots: Vec::new(),
        state: None,
        input: BTreeSet::new(),
        output: BTreeSet::new(),
    });
    compiler.register_scope(0)?;
    let mut scanned = 0;
    let mut layout = compiler.close_structure(&mut scanned)?;
    // A retained value's bindings may enter a source; discarded programs never do.
    // Structural demands for that new scope close before selecting its final values.
    loop {
        compiler.compose_programs()?;
        let mut binding_bytes = Some(0usize);
        for &node in &compiler.scopes[0].output {
            for (component, program) in &compiler.nodes[node].programs {
                compiler.program_bindings(component, program, |_, target| {
                    binding_bytes = binding_bytes.and_then(|bytes| {
                        bytes
                            .checked_add(target_clone_bytes(target)?)?
                            .checked_add(64)
                    });
                });
            }
        }
        compiler.admit(binding_bytes)?;
        let mut demanded = Vec::new();
        for &node in &compiler.scopes[0].output {
            for (component, program) in &compiler.nodes[node].programs {
                compiler.program_bindings(component, program, |scope, target| {
                    demanded.push((scope, target.clone()))
                });
            }
        }
        let previous = compiler.scopes.len();
        for (scope, target) in demanded {
            compiler.resolve(scope, &target)?;
        }
        if previous == compiler.scopes.len() {
            break;
        }
        layout = compiler.close_structure(&mut scanned)?;
    }
    compiler.materialize(layout)
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn admit(&mut self, bytes: Option<usize>) -> Result<(), PrefabOverlayConflict> {
        self.context
            .reserve_scratch_bytes(bytes, None, None)
            .then_some(())
            .ok_or(PrefabOverlayConflict::WorkLimit)
    }

    fn source_entities(&self, scope: ScopeId) -> &[SceneEntityRecord] {
        self.scopes[scope]
            .source
            .as_ref()
            .map_or(&self.root.entities, |source| &source.document().entities)
    }

    fn source_roots(&self, scope: ScopeId) -> &[SceneEntityId] {
        self.scopes[scope]
            .source
            .as_ref()
            .map_or(&self.root.roots, |source| &source.document().roots)
    }

    fn instance(&self, site: &Site) -> Option<&PrefabInstance> {
        let mut instance = self
            .source_entities(site.source)
            .get(site.entity)?
            .prefab
            .as_ref()?;
        for segment in &site.segments {
            let overlay = instance.overrides.as_ref()?;
            instance = match *segment {
                Segment::Addition(index) => overlay.additions.get(index)?.prefab.as_ref()?,
                Segment::Claim(index) => overlay.instances.get(index)?.value.as_ref()?,
            };
        }
        Some(instance)
    }

    fn overlay(&self, scope: ScopeId) -> Option<&PrefabOverlay> {
        self.scopes[scope]
            .site
            .as_ref()
            .and_then(|site| self.instance(site))?
            .overrides
            .as_ref()
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

    fn stable_site(&self, site: &Site) -> DeclarationAddress {
        let entity = self.source_entities(site.source)[site.entity].id.clone();
        let root = self.scopes[site.source]
            .source
            .as_ref()
            .map_or(DeclarationRoot::Scene(entity.clone()), |source| {
                DeclarationRoot::Prefab(source.key().clone(), entity)
            });
        DeclarationAddress {
            root,
            segments: site
                .segments
                .iter()
                .map(|segment| match *segment {
                    Segment::Addition(index) => DeclarationSegment::Addition(index),
                    Segment::Claim(index) => DeclarationSegment::InstanceClaim(index),
                })
                .collect(),
        }
    }

    fn site_bytes(&self, site: &Site) -> Option<usize> {
        size_of::<DeclarationAddress>()
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
                    .checked_mul(size_of::<DeclarationSegment>())?,
            )
    }

    fn register_scope(&mut self, scope: ScopeId) -> Result<(), PrefabOverlayConflict> {
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
                    .checked_add(entity.children.iter().try_fold(0usize, |bytes, child| {
                        bytes
                            .checked_add(size_of::<NodeId>())?
                            .checked_add(provenance_string_bytes(child.as_str())?.checked_mul(2)?)
                    })?)?
                    .checked_add(entity.components.keys().try_fold(
                        0usize,
                        |bytes, component| {
                            bytes
                                .checked_add(128)?
                                .checked_add(provenance_string_bytes(component.as_str())?)
                        },
                    )?)
            })
            .and_then(|bytes| bytes.checked_add(additions.checked_mul(512)?));
        self.admit(bytes)?;
        let entities = self
            .context
            .observe_item_budget(
                PrefabExpansionBudgetKind::MaterializedEntities,
                self.context.usage.materialized_entities,
                count.saturating_add(additions),
                self.context.options.limits.materialized_entities(),
                None,
                None,
            )
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
        let mut usage = self.context.usage;
        usage.materialized_entities = entities;
        if !self.context.admit_working_usage(usage, None, None) {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        self.context.usage = usage;
        let entities = self.source_entities(scope);
        let document = SceneDocument {
            roots: self.source_roots(scope).to_vec(),
            entities: entities
                .iter()
                .map(|entity| {
                    SceneEntityRecord::new(entity.id.clone())
                        .with_children(entity.children.iter().cloned())
                })
                .collect(),
        };
        let structure = validate_scene_structure(&document);
        if structure.has_errors() {
            self.context.diagnostics.extend(structure);
            return Err(PrefabOverlayConflict::Placement);
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
                            PrefabExpansionBudgetKind::OverlayDeclarations
                        } else {
                            PrefabExpansionBudgetKind::WorkingBytes
                        };
                    let (observed, maximum) =
                        if kind == PrefabExpansionBudgetKind::OverlayDeclarations {
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
                    PrefabOverlayConflict::WorkLimit
                })?;
            let declarations = self
                .context
                .observe_item_budget(
                    PrefabExpansionBudgetKind::OverlayDeclarations,
                    self.context.usage.overlay_declarations,
                    cost.declarations,
                    self.context.options.limits.overlay_declarations(),
                    anchor.as_ref(),
                    None,
                )
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            self.context.usage.overlay_declarations = declarations;
            self.admit(
                cost.nodes
                    .checked_mul(128)
                    .and_then(|bytes| bytes.checked_add(cost.structural_logical_bytes)),
            )?;
            let overlay = self.overlay(scope).unwrap();
            if overlay.additions.iter().any(|addition| {
                addition
                    .bindings
                    .keys()
                    .any(|component| !addition.components.contains_key(component))
            }) {
                return Err(PrefabOverlayConflict::Reference);
            }
            let mut claimed = BTreeSet::new();
            for claim in &overlay.instances {
                if !claimed.insert(&claim.target)
                    || (claim.target.through.is_empty()
                        && matches!(claim.target.entity, PrefabEntityOrigin::InstanceAddition(_)))
                {
                    return Err(PrefabOverlayConflict::DuplicateIntent);
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
                    site.segments.push(Segment::Addition(index));
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
                return Err(PrefabOverlayConflict::DuplicateIntent);
            }
            self.locators.insert(locator.clone(), node);
            self.nodes.push(Node {
                scope,
                origin,
                record,
                locator,
                children: Vec::new(),
                slot,
                entered: None,
                opened: false,
                programs: BTreeMap::new(),
            });
        }
        self.scopes[scope].roots = self
            .source_roots(scope)
            .iter()
            .map(|id| self.scopes[scope].nodes[&PrefabEntityOrigin::SourceEntity(id.clone())])
            .collect();
        for index in 0..count {
            let entity = &self.source_entities(scope)[index];
            let node =
                self.scopes[scope].nodes[&PrefabEntityOrigin::SourceEntity(entity.id.clone())];
            self.nodes[node].children = entity
                .children
                .iter()
                .map(|id| self.scopes[scope].nodes[&PrefabEntityOrigin::SourceEntity(id.clone())])
                .collect();
        }
        for index in 0..additions {
            let id = &self.overlay(scope).unwrap().additions[index].id;
            let node = self.scopes[scope].nodes[&PrefabEntityOrigin::InstanceAddition(id.clone())];
            self.scopes[scope].roots.push(node);
        }
        if scope == 0 {
            for entity in &self.root.entities {
                if !self.context.reserve_provenance_bytes(
                    provenance_entry_bytes(&entity.id, &entity.id),
                    Some(&entity.id),
                    None,
                ) {
                    return Err(PrefabOverlayConflict::WorkLimit);
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
    ) -> Result<State, PrefabOverlayConflict> {
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
            if let Some(layer) = self.overlay(current) {
                let mut count = 0usize;
                structural_targets(layer, |_| count = count.saturating_add(1));
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
                    .ok_or(PrefabOverlayConflict::OwnerAdmission)?;
                let prefix = relative
                    .through
                    .iter()
                    .map(|hop| &hop.anchor)
                    .chain(std::iter::once(&relative.entity))
                    .collect::<Vec<_>>();
                let declaration = self.stable_site(self.scopes[current].site.as_ref().unwrap());
                let mut index = 0;
                structural_targets(layer, |target| {
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
                    index += 1;
                });
            }
            let Some(anchor) = self.scopes[current].anchor else {
                break;
            };
            current = self.nodes[anchor].scope;
        }
        Ok(State { overlay, demands })
    }

    fn reserve_occurrence(
        &mut self,
        node: NodeId,
        request: &AssetRef,
        key: &SourceDocumentKey,
    ) -> Result<(), PrefabOverlayConflict> {
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
            .ok_or(PrefabOverlayConflict::WorkLimit)
    }

    fn select_site(&self, node: NodeId) -> Result<Option<Site>, PrefabOverlayConflict> {
        let mut site = self.nodes[node].slot.clone();
        let mut current = self.nodes[node].scope;
        loop {
            if let Some(overlay) = self
                .overlay(current)
                .filter(|overlay| !overlay.instances.is_empty())
            {
                let target = self
                    .relative(current, node)
                    .ok_or(PrefabOverlayConflict::OwnerAdmission)?;
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
                            return Err(PrefabOverlayConflict::DuplicateIntent);
                        }
                        if claim.target != target {
                            return Err(PrefabOverlayConflict::SourceChanged);
                        }
                        matched = true;
                        site = claim.value.as_ref().map(|_| {
                            let mut site = self.scopes[current].site.as_ref().unwrap().clone();
                            site.segments.push(Segment::Claim(index));
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

    fn open_slot(&mut self, node: NodeId) -> Result<Option<ScopeId>, PrefabOverlayConflict> {
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
        self.admit(Some(512))?;
        let instance = self
            .instance(&site)
            .ok_or(PrefabOverlayConflict::OwnerAdmission)?;
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
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        if !self.context.reserve_resolved_instance(&anchor, &request) {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        let mut working = self.context.working_set();
        working.source_depth = depth;
        let source = match (self.context.resolver)(&request, &mut working) {
            Ok(source) => source,
            Err(error) => {
                self.context.load_error = Some(error);
                return Err(PrefabOverlayConflict::OwnerAdmission);
            }
        };
        self.context.options.limits = self
            .context
            .options
            .limits
            .with_working_bytes(working.working_limit);
        if !self
            .context
            .admit_working_usage(self.context.usage, Some(&anchor), Some(&request))
        {
            return Err(PrefabOverlayConflict::WorkLimit);
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
            return Err(PrefabOverlayConflict::OwnerAdmission);
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
            return Err(PrefabOverlayConflict::SourceChanged);
        }
        if !source.matches_authority(self.context.authority)
            || self
                .instance(&site)
                .unwrap()
                .overrides
                .as_ref()
                .is_some_and(|overlay| &overlay.base != source.key())
        {
            return Err(PrefabOverlayConflict::SourceChanged);
        }
        self.reserve_occurrence(node, &request, source.key())?;
        let state = self.state(node, &site, source.key())?;
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
                return Err(PrefabOverlayConflict::OwnerAdmission);
            }
            ancestor = self.nodes[parent].scope;
        }
        let scope = self.scopes.len();
        self.scopes.push(Scope {
            source: Some(source),
            request: Some(request),
            site: Some(site),
            anchor: Some(node),
            depth,
            nodes: BTreeMap::new(),
            roots: Vec::new(),
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
    ) -> Result<NodeId, PrefabOverlayConflict> {
        let mut current = scope;
        for hop in &target.through {
            let anchor = *self.scopes[current]
                .nodes
                .get(&hop.anchor)
                .ok_or_else(|| PrefabOverlayConflict::MissingTarget(target.clone()))?;
            current = self
                .open_slot(anchor)?
                .ok_or_else(|| PrefabOverlayConflict::MissingTarget(target.clone()))?;
            if self.scopes[current].source.as_ref().unwrap().key() != &hop.expected_base {
                return Err(PrefabOverlayConflict::SourceChanged);
            }
        }
        self.scopes[current]
            .nodes
            .get(&target.entity)
            .copied()
            .ok_or_else(|| PrefabOverlayConflict::MissingTarget(target.clone()))
    }

    fn close_demands(&mut self, scanned: &mut usize) -> Result<(), PrefabOverlayConflict> {
        while *scanned < self.scopes.len() {
            let scope = *scanned;
            *scanned += 1;
            let mut bytes = Some(0usize);
            if let Some(overlay) = self.overlay(scope) {
                structural_targets(overlay, |target| {
                    bytes = bytes.and_then(|bytes| bytes.checked_add(target_clone_bytes(target)?));
                });
            }
            self.admit(bytes)?;
            let mut targets = Vec::new();
            if let Some(overlay) = self.overlay(scope) {
                structural_targets(overlay, |target| targets.push(target.clone()));
            }
            for target in targets {
                self.resolve(scope, &target)?;
            }
        }
        Ok(())
    }

    fn close_structure(
        &mut self,
        scanned: &mut usize,
    ) -> Result<SceneDocument, PrefabOverlayConflict> {
        loop {
            self.close_demands(scanned)?;
            self.postorder.clear();
            let document = self.layout_scope(0, &mut BTreeMap::new())?;
            self.admit(self.scopes[0].output.len().checked_mul(size_of::<NodeId>()))?;
            let pending = self.scopes[0]
                .output
                .iter()
                .copied()
                .filter(|&node| !self.nodes[node].opened)
                .collect::<Vec<_>>();
            if pending.is_empty() {
                return Ok(document);
            }
            for node in pending {
                self.open_slot(node)?;
            }
        }
    }

    /// The declaring layer sees its own output, even if an outer layer later removes it.
    /// Callers admit the bounded target-index allocation before constructing this map.
    fn target_nodes(
        &self,
        scope: ScopeId,
    ) -> Result<BTreeMap<PrefabEntityTarget, SceneEntityId>, PrefabOverlayConflict> {
        self.scopes[scope]
            .output
            .iter()
            .map(|&node| {
                self.relative(scope, node)
                    .map(|target| (target, self.nodes[node].locator.clone()))
                    .ok_or(PrefabOverlayConflict::OwnerAdmission)
            })
            .collect()
    }

    fn compose_programs(&mut self) -> Result<(), PrefabOverlayConflict> {
        let bytes = self.nodes.iter().try_fold(0usize, |bytes, node| {
            bytes.checked_add(self.components(node.record).len().checked_mul(256)?)
        });
        self.admit(bytes)?;
        for node in 0..self.nodes.len() {
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
            let Some(overlay) = self.overlay(scope) else {
                continue;
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
                .map(|&node| (&self.nodes[node].locator, node))
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
                    return Err(PrefabOverlayConflict::DuplicateIntent);
                }
                let node = targets
                    .get(target)
                    .and_then(|locator| locators.get(locator))
                    .copied()
                    .ok_or_else(|| PrefabOverlayConflict::MissingTarget(target.clone()))?;
                if !touched.insert((node, component.clone())) {
                    return Err(PrefabOverlayConflict::DuplicateIntent);
                }
                intents.push((node, component.clone(), index));
            }
            for (node, component, index) in intents {
                let exists = self.nodes[node].programs.contains_key(&component);
                match &self.overlay(scope).unwrap().components[index] {
                    PrefabComponentIntent::Set { mode, .. } => {
                        if (*mode == PrefabComponentSetMode::Add) == exists {
                            return Err(PrefabOverlayConflict::Component);
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
                            return Err(PrefabOverlayConflict::Component);
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
                        super::super::properties::validate_claim_paths(claims)?;
                        let program = self.nodes[node]
                            .programs
                            .get_mut(&component)
                            .ok_or(PrefabOverlayConflict::Component)?;
                        program.properties.push((scope, index));
                    }
                }
            }
        }
        Ok(())
    }

    fn program_bindings(
        &self,
        component: &ComponentTypeId,
        program: &Program,
        mut visit: impl FnMut(ScopeId, &PrefabEntityTarget),
    ) {
        match program.record {
            Record::Source { .. } => {}
            Record::Addition { scope, index } => {
                for binding in self.overlay(scope).unwrap().additions[index]
                    .bindings
                    .get(component)
                    .into_iter()
                    .flatten()
                {
                    visit(scope, &binding.target);
                }
            }
            Record::Set { scope, index } => {
                if let PrefabComponentIntent::Set { bindings, .. } =
                    &self.overlay(scope).unwrap().components[index]
                {
                    for binding in bindings {
                        visit(scope, &binding.target);
                    }
                }
            }
        }
        for &(scope, index) in &program.properties {
            if let PrefabComponentIntent::Properties { claims, .. } =
                &self.overlay(scope).unwrap().components[index]
            {
                for binding in claims.iter().flat_map(|claim| &claim.bindings) {
                    visit(scope, &binding.target);
                }
            }
        }
    }

    fn layout_scope(
        &mut self,
        scope: ScopeId,
        nested: &mut BTreeMap<SceneEntityId, Vec<SceneEntityId>>,
    ) -> Result<SceneDocument, PrefabOverlayConflict> {
        let local = self.scopes[scope]
            .nodes
            .values()
            .copied()
            .collect::<Vec<_>>();
        self.admit(local.len().checked_mul(1024))?;
        let mut document = SceneDocument {
            roots: self.scopes[scope]
                .roots
                .iter()
                .map(|node| self.nodes[*node].locator.clone())
                .collect(),
            entities: Vec::new(),
        };
        for node in local {
            let mut record = SceneEntityRecord::new(self.nodes[node].locator.clone());
            if let Some(child) = self.nodes[node].entered {
                let child = self.layout_scope(child, nested)?;
                record.children.extend(child.roots.iter().cloned());
                nested.insert(record.id.clone(), child.roots);
                document.entities.extend(child.entities);
            }
            record.children.extend(
                self.nodes[node]
                    .children
                    .iter()
                    .map(|child| self.nodes[*child].locator.clone()),
            );
            document.entities.push(record);
        }
        let input = document
            .entities
            .iter()
            .map(|record| self.locators[&record.id])
            .collect::<BTreeSet<_>>();
        self.admit(input.len().checked_mul(256))?;
        self.scopes[scope].input = input;
        if let Some(overlay) = self.overlay(scope) {
            let _ = overlay;
            let bytes = self.scopes[scope]
                .input
                .iter()
                .try_fold(0usize, |bytes, &node| {
                    bytes
                        .checked_add(self.relative_bytes(scope, node)?)?
                        .checked_add(256)
                });
            self.admit(bytes)?;
            let overlay = self.overlay(scope).unwrap();
            let mut targets = BTreeMap::new();
            for &node in &self.scopes[scope].input {
                if let Some(target) = self.relative(scope, node) {
                    targets.insert(target, self.nodes[node].locator.clone());
                }
            }
            layout::apply_layout(&mut document, &mut targets, overlay, nested)?;
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
            .filter(|node| membership.contains(&self.nodes[*node].locator))
            .collect();
        self.postorder.push(scope);
        Ok(document)
    }
}

fn structural_targets(overlay: &PrefabOverlay, mut visit: impl FnMut(&PrefabEntityTarget)) {
    for claim in &overlay.instances {
        visit(&claim.target);
    }
    for target in &overlay.suppressions {
        visit(target);
    }
    for placement in &overlay.placements {
        visit(&placement.target);
        if let PrefabPlacementParent::Entity(target) = &placement.parent {
            visit(target);
        }
        if let PrefabPlacementSlot::Before(target) | PrefabPlacementSlot::After(target) =
            &placement.slot
        {
            visit(target);
        }
    }
    for component in &overlay.components {
        match component {
            PrefabComponentIntent::Set { target, .. }
            | PrefabComponentIntent::Suppress { target, .. }
            | PrefabComponentIntent::Properties { target, .. } => visit(target),
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
