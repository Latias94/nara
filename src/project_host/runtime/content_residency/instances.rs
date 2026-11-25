//! Inspection and placement of dependencies retained by the current hosted Scene.

#[cfg(feature = "project-source")]
use std::sync::Arc;

use nara_asset::{AssetProductRef, ImportedProductId};
use nara_ecs::World;
use nara_ecs::identity::{SpawnedSceneInstance, WorldIdentityDomain};
#[cfg(feature = "project-source")]
use nara_scene::CompiledSceneCandidate;
use nara_scene::{
    CompiledSceneDocument, SceneComponentBindings, SceneOccurrenceKey, SceneSpawnReport,
};

use super::{ProjectSceneResidency, ResidencyAuthority, failure, rejected};
#[cfg(feature = "project-source")]
use crate::project_content::ProjectSourceContent;
use crate::{
    runtime_content::{RuntimeContentMemberKind, RuntimeContentScene},
    startup_scene::{CurrentScene, current_scene_bindings, current_scene_inspection_bindings},
};

/// Inspects one Immediate Scene dependency retained by the current Host content generation.
///
/// `expected` must be the complete current root receipt in this World. The callback borrows the
/// exact admitted flat document without acquiring, decoding or compiling content. Inspection is
/// also available during startup activation after `CurrentScene` has been installed.
///
/// Only primary Scene products in the original retained dependency closure are accepted. The
/// current root, Deferred targets, unrelated catalogue members and independently managed sources
/// are rejected. This operation neither instantiates a Scene nor grants travel authority.
pub fn inspect_current_scene_dependency<R>(
    world: &World,
    expected: &SpawnedSceneInstance,
    product: AssetProductRef,
    inspect: impl FnOnce(&CompiledSceneDocument) -> R,
) -> Result<R, SceneSpawnReport> {
    let bindings = current_scene_inspection_bindings(world, expected)?;
    let dependency = current_dependency(world, expected, product, &bindings)?;
    Ok(inspect(dependency.document()))
}

/// Instantiates one retained Immediate Scene dependency at the caller's logical occurrence.
///
/// Uses the original component bindings' preparation and commit, including receiving-World
/// asset reservations and occurrence admission. The current root source and receipt stay owned
/// by their existing Host. Success returns a separate activation receipt for the caller to retire.
///
/// The containing Scene retains dependency data and resource custody. The caller must retire
/// these spawned activations before or in the same product transaction as that Scene's
/// replacement. This function performs no content
/// acquisition, resource publication or implicit travel, and requires finalized startup.
#[must_use]
pub fn instantiate_current_scene_dependency(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    product: AssetProductRef,
    occurrence: &SceneOccurrenceKey,
) -> SceneSpawnReport {
    nara_scene::hierarchy::prepare_mutation_baseline(world);
    let bindings = match current_scene_bindings(world, expected) {
        Ok(bindings) => bindings,
        Err(report) => return report,
    };
    let dependency = match current_dependency(world, expected, product, &bindings) {
        Ok(dependency) => dependency,
        Err(report) => return report,
    };
    let prepared = match &dependency {
        #[cfg(feature = "project-source")]
        SceneDependency::Source { sources, candidate } => {
            bindings.prepare_scene(world, candidate, occurrence, Some(sources.resolution()))
        }
        SceneDependency::Processed(scene) => {
            bindings.prepare_runtime_scene(world, scene.candidate(), occurrence)
        }
    };
    match prepared {
        Ok(prepared) => prepared.commit(world),
        Err(report) => report,
    }
}

enum SceneDependency {
    #[cfg(feature = "project-source")]
    Source {
        sources: Arc<ProjectSourceContent>,
        candidate: Arc<CompiledSceneCandidate>,
    },
    Processed(RuntimeContentScene),
}

impl SceneDependency {
    fn document(&self) -> &CompiledSceneDocument {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source { candidate, .. } => candidate.document(),
            Self::Processed(scene) => scene.candidate().document(),
        }
    }
}

fn current_dependency(
    world: &World,
    expected: &SpawnedSceneInstance,
    product: AssetProductRef,
    bindings: &SceneComponentBindings,
) -> Result<SceneDependency, SceneSpawnReport> {
    if !world
        .get_resource::<WorldIdentityDomain>()
        .is_some_and(|domain| {
            domain.id() == expected.domain_id() && domain.registered_locators(world).is_ok()
        })
    {
        return Err(rejected(failure(
            "scene.current.world-mismatch",
            "Current Scene receipt does not belong to this World's identity domain",
        )));
    }
    let current = world.resource::<CurrentScene>();
    if !current.matches_component_bindings(bindings) {
        return Err(rejected(failure(
            "scene.current.component-bindings-mismatch",
            "Current Scene source does not match the installed component definitions",
        )));
    }
    if product.product() != ImportedProductId::PRIMARY {
        return Err(dependency_unavailable());
    }
    let residency = world
        .get_resource::<ProjectSceneResidency>()
        .ok_or_else(|| {
            rejected(failure(
                "project.scene.residency-unavailable",
                "Project Scene residency resources are unavailable",
            ))
        })?;
    match &residency.authority {
        #[cfg(feature = "project-source")]
        ResidencyAuthority::Source { .. } => {
            let sources = current.project_sources().ok_or_else(authority_mismatch)?;
            let candidate = sources
                .scene_dependency(product)
                .filter(|candidate| candidate.source_key() != current.source_key())
                .ok_or_else(dependency_unavailable)?;
            Ok(SceneDependency::Source {
                sources: Arc::clone(sources),
                candidate: Arc::clone(candidate),
            })
        }
        ResidencyAuthority::Processed { scenes, base, .. } => {
            let current_base = current
                .hosted_runtime_base()
                .ok_or_else(authority_mismatch)?;
            if !base.same_base(current_base) {
                return Err(authority_mismatch());
            }
            let scene = scenes
                .iter()
                .find(|scene| scene.product() == product)
                .ok_or_else(dependency_unavailable)?;
            if !base.same_base(scene.base()) {
                return Err(authority_mismatch());
            }
            let member = base.member(product).ok_or_else(dependency_unavailable)?;
            if !matches!(
                member.kind(),
                RuntimeContentMemberKind::Scene { source }
                    if source == scene.candidate().source_key()
            ) || scene.candidate().source_key() == current.source_key()
            {
                return Err(dependency_unavailable());
            }
            if !scene.candidate().definitions().matches(bindings) {
                return Err(authority_mismatch());
            }
            Ok(SceneDependency::Processed(scene.clone()))
        }
    }
}

fn dependency_unavailable() -> SceneSpawnReport {
    rejected(failure(
        "project.scene.dependency-not-resident",
        "Selected product is not a retained primary Immediate Scene dependency of the current Scene",
    ))
}

fn authority_mismatch() -> SceneSpawnReport {
    rejected(failure(
        "project.scene.content-authority-mismatch",
        "Current Scene dependency does not retain the installed Host content authority",
    ))
}
