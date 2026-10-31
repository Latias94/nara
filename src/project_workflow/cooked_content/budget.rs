//! Logical costs for finite assembly, without an allocator or global budget service.

use std::mem::size_of;

use crate::runtime_content::{
    RuntimeContentDependency, RuntimeContentMember, RuntimeContentMemberKind,
};
use nara_asset::AssetProductRef;

use super::{CookedContentError, CookedContentRequest};

pub(super) fn add(left: usize, right: usize) -> Result<usize, CookedContentError> {
    left.checked_add(right).ok_or(CookedContentError::Overflow)
}

pub(super) fn mul(count: usize, bytes: usize) -> Result<usize, CookedContentError> {
    count.checked_mul(bytes).ok_or(CookedContentError::Overflow)
}

pub(super) fn check(
    kind: &'static str,
    required: usize,
    limit: usize,
) -> Result<(), CookedContentError> {
    if required > limit {
        Err(CookedContentError::Budget {
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
) -> Result<usize, CookedContentError> {
    let mut total = add(
        request.target.triple().len(),
        request.target.profile().len(),
    )?;
    total = add(total, request.configuration.window.title.len())?;
    total = add(
        total,
        mul(
            request.configuration.capabilities.requested.len(),
            size_of::<nara_project::ProductCapability>(),
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
) -> Result<usize, CookedContentError> {
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
                .ok_or(CookedContentError::Overflow)?,
            RuntimeContentMemberKind::Asset { format, .. } => format.as_str().len(),
        },
    )
}
