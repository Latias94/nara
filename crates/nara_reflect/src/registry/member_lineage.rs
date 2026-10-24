use std::ops::Bound::{Included, Unbounded};

use super::{
    BTreeSet, ComponentFieldSchema, ComponentMemberPath, ComponentRegistryError, ComponentTypeId,
};

fn active_paths(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
) -> Result<BTreeSet<ComponentMemberPath>, ComponentRegistryError> {
    field
        .active_member_paths()
        .map(|paths| paths.into_iter().collect())
        .map_err(
            |(field_id, source)| ComponentRegistryError::InvalidComponentFieldShape {
                component_id: component.clone(),
                field_id,
                source,
            },
        )
}

fn covers(retired: &ComponentMemberPath, path: &ComponentMemberPath) -> bool {
    path.segments().starts_with(retired.segments())
}

fn has_active_scope(active: &BTreeSet<ComponentMemberPath>, path: &ComponentMemberPath) -> bool {
    active
        .range::<ComponentMemberPath, _>((Included(path), Unbounded))
        .next()
        .is_some_and(|candidate| covers(path, candidate))
}

fn retired_scopes(field: &ComponentFieldSchema) -> Vec<&ComponentMemberPath> {
    let mut paths = field.member_tombstones.iter().collect::<Vec<_>>();
    paths.sort();
    let mut scopes: Vec<&ComponentMemberPath> = Vec::new();
    for path in paths {
        if scopes.last().is_none_or(|parent| !covers(parent, path)) {
            scopes.push(path);
        }
    }
    scopes
}

fn is_retired(scopes: &[&ComponentMemberPath], path: &ComponentMemberPath) -> bool {
    let end = scopes.partition_point(|candidate| *candidate <= path);
    end.checked_sub(1)
        .is_some_and(|index| covers(scopes[index], path))
}

pub(super) fn validate(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
) -> Result<(), ComponentRegistryError> {
    let active = active_paths(component, field)?;
    let mut retired = BTreeSet::new();
    for path in &field.member_tombstones {
        if !retired.insert(path) {
            return Err(ComponentRegistryError::DuplicateMemberTombstone {
                component_id: component.clone(),
                field_id: field.id.clone(),
                path: path.clone(),
            });
        }
        if has_active_scope(&active, path) {
            return Err(ComponentRegistryError::ActiveMemberIsTombstoned {
                component_id: component.clone(),
                field_id: field.id.clone(),
                path: path.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn validate_successor(
    component: &ComponentTypeId,
    current: &ComponentFieldSchema,
    previous: &ComponentFieldSchema,
) -> Result<(), ComponentRegistryError> {
    let active = active_paths(component, current)?;
    let retired = retired_scopes(current);
    for path in &previous.member_tombstones {
        if has_active_scope(&active, path) {
            return Err(ComponentRegistryError::ReactivatedMember {
                component_id: component.clone(),
                field_id: current.id.clone(),
                path: path.clone(),
            });
        }
        if !is_retired(&retired, path) {
            return Err(ComponentRegistryError::MissingMemberTombstone {
                component_id: component.clone(),
                field_id: current.id.clone(),
                path: path.clone(),
            });
        }
    }
    for path in active_paths(component, previous)? {
        if !active.contains(&path) && !is_retired(&retired, &path) {
            return Err(ComponentRegistryError::MissingMemberTombstone {
                component_id: component.clone(),
                field_id: current.id.clone(),
                path,
            });
        }
    }
    Ok(())
}
