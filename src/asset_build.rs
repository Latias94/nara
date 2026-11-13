//! Finite local artifact builds, separate from authored identity and Runtime acquisition.
//!
//! Compiled callers choose the concrete operations. Generated metadata only records and verifies
//! that selection. A verified group captures exact encoded bytes; it grants no native residency.

mod budget;
mod execute;
mod format;
mod images;
mod live_value;
mod model;
mod source_reload;
mod store;

pub use execute::{
    ArtifactBuildReason, ArtifactBuildResult, ArtifactBuildStatus, ArtifactSourceRequest,
    build_source,
};
pub use images::{build_image_import, cook_image_group};
pub use live_value::BuiltSourceAsset;
pub use model::{
    ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
    ArtifactBuildOutput, ArtifactBuildSeed, ArtifactContentDigest, ArtifactGroupCandidate,
    ArtifactGroupMember, ArtifactRecipeDigest, ArtifactSeedDigest,
};
pub use source_reload::{SourceAssetBuildPlugin, SourceAssetBuildStats};
pub use store::{
    ArtifactBuildStore, ArtifactCommitReceipt, ArtifactPublicationPoint, VerifiedArtifactGroup,
};
