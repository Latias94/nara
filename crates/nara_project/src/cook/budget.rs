//! Logical costs for finite assembly, without an allocator or global budget service.

use std::mem::size_of;

use nara_asset::AssetProductRef;
use nara_scene::runtime_catalog::{
    RuntimeContentDependency, RuntimeContentMember, RuntimeContentMemberKind,
};

use super::{CookedContentAssemblyError, CookedContentRequest};

pub(super) fn add(left: usize, right: usize) -> Result<usize, CookedContentAssemblyError> {
    left.checked_add(right)
        .ok_or(CookedContentAssemblyError::Overflow)
}

pub(super) fn mul(count: usize, bytes: usize) -> Result<usize, CookedContentAssemblyError> {
    count
        .checked_mul(bytes)
        .ok_or(CookedContentAssemblyError::Overflow)
}

pub(super) fn check(
    kind: &'static str,
    required: usize,
    limit: usize,
) -> Result<(), CookedContentAssemblyError> {
    if required > limit {
        Err(CookedContentAssemblyError::Budget {
            kind,
            required,
            limit,
        })
    } else {
        Ok(())
    }
}

pub(super) fn request_heap_bytes(
    request: &CookedContentRequest,
) -> Result<usize, CookedContentAssemblyError> {
    let mut total = add(
        request.target.triple().len(),
        request.target.profile().len(),
    )?;
    total = add(total, request.configuration.window.title.len())?;
    total = add(
        total,
        mul(
            request.configuration.capabilities.requested.len(),
            size_of::<nara_scene::runtime_configuration::ProductCapability>(),
        )?,
    )?;
    add(
        total,
        mul(
            add(request.scenes.len(), request.addressable_roots.len())?,
            size_of::<AssetProductRef>(),
        )?,
    )
}

pub(super) fn member_heap_bytes(
    member: &RuntimeContentMember,
) -> Result<usize, CookedContentAssemblyError> {
    let mut total = add(
        member.path().len(),
        mul(member.products().len(), size_of::<AssetProductRef>())?,
    )?;
    total = add(
        total,
        mul(
            member.dependencies().len(),
            size_of::<RuntimeContentDependency>(),
        )?,
    )?;
    add(
        total,
        match member.kind() {
            RuntimeContentMemberKind::Scene { source } => source
                .retained_bytes()
                .checked_sub(size_of::<nara_scene::SourceDocumentKey>())
                .ok_or(CookedContentAssemblyError::Overflow)?,
            RuntimeContentMemberKind::Asset { format, .. } => format.as_str().len(),
        },
    )
}
