use std::fmt::{self, Debug, Formatter};

use nara_fs::{ContentDigest, PathValidationError, RelativeComponent};
use nara_project::CURRENT_PROJECT_SCHEMA_VERSION;
use nara_reflect::{AuthoringSchemaBundle, ComponentSchemaProviderDefinition};
use thiserror::Error;

use super::dependency::NaraDependencyReceipt;

mod product_contract;

pub(crate) use product_contract::product_recipe as desktop_2d_v1_product_recipe;
use product_contract::{ProductContractSources, render_cargo_manifest, render_project_manifest};

const MAX_PROJECT_SLUG_BYTES: usize = 64;
const MAX_STARTER_MEMBER_BYTES: usize = 256 * 1024;
const DESKTOP_2D_V1_SCENE_FORMAT_VERSION: u32 = 1;

const AUTHOR_GUIDE_BODY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/starters/desktop-2d/v1/README.md"
));
const LIBRARY_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/starters/desktop-2d/v1/src/lib.rs"
));
const HEADLESS_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/starters/desktop-2d/v1/src/bin/headless.rs"
));
const DESKTOP_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/starters/desktop-2d/v1/src/bin/desktop.rs"
));
const STARTUP_SCENE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/starters/desktop-2d/v1/scenes/startup.scene.json"
));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectStarter {
    Desktop2dV1,
}

impl ProjectStarter {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Desktop2dV1 => "desktop-2d/v1",
        }
    }

    #[must_use]
    pub const fn manifest_schema_version(self) -> u32 {
        match self {
            Self::Desktop2dV1 => CURRENT_PROJECT_SCHEMA_VERSION,
        }
    }

    #[must_use]
    pub const fn scene_format_version(self) -> u32 {
        match self {
            Self::Desktop2dV1 => DESKTOP_2D_V1_SCENE_FORMAT_VERSION,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProjectSlug(String);

impl ProjectSlug {
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectSlugError> {
        let value = value.into();
        validate_project_slug(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn destination_component(&self) -> RelativeComponent {
        RelativeComponent::new(&self.0)
            .expect("a validated project slug is a valid relative component")
    }
}

impl Debug for ProjectSlug {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ProjectSlug").field(&self.0).finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSlugErrorKind {
    Empty,
    TooLong,
    InvalidStart,
    InvalidEnd,
    InvalidCharacter,
    RepeatedSeparator,
    InvalidFilesystemName,
}

#[derive(Debug, Error)]
#[error("project slug is invalid: {kind:?}")]
pub struct ProjectSlugError {
    kind: ProjectSlugErrorKind,
}

impl ProjectSlugError {
    #[must_use]
    pub const fn kind(&self) -> ProjectSlugErrorKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectDirectory {
    Assets,
    Prefabs,
    Scenes,
    Schema,
    Source,
    SourceBinaries,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectMember {
    CargoManifest,
    ProjectManifest,
    AuthorGuide,
    LibrarySource,
    HeadlessSource,
    DesktopSource,
    StartupScene,
    AuthoringSchemaBundle,
}

pub struct ProjectMemberPlan {
    member: ProjectMember,
    bytes: Box<[u8]>,
    digest: ContentDigest,
}

impl ProjectMemberPlan {
    #[must_use]
    pub const fn member(&self) -> ProjectMember {
        self.member
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

impl Debug for ProjectMemberPlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectMemberPlan")
            .field("member", &self.member)
            .field("digest", &self.digest)
            .finish()
    }
}

pub struct ProjectCreationPlan {
    slug: ProjectSlug,
    dependency: super::dependency::NaraDependencyReceipt,
    members: Box<[ProjectMemberPlan]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectCreationPlanReceipt {
    starter: ProjectStarter,
    slug: ProjectSlug,
    dependency: super::dependency::NaraDependencyReceipt,
    manifest_schema_version: u32,
    scene_format_version: u32,
    directories: Box<[ProjectDirectory]>,
    members: Box<[ProjectMemberReceipt]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectMemberReceipt {
    member: ProjectMember,
    digest: ContentDigest,
}

impl ProjectMemberReceipt {
    #[must_use]
    pub const fn member(self) -> ProjectMember {
        self.member
    }

    #[must_use]
    pub const fn digest(self) -> ContentDigest {
        self.digest
    }
}

impl ProjectCreationPlanReceipt {
    #[must_use]
    pub const fn starter(&self) -> ProjectStarter {
        self.starter
    }

    #[must_use]
    pub const fn slug(&self) -> &ProjectSlug {
        &self.slug
    }

    #[must_use]
    pub const fn dependency(&self) -> &super::dependency::NaraDependencyReceipt {
        &self.dependency
    }

    #[must_use]
    pub const fn manifest_schema_version(&self) -> u32 {
        self.manifest_schema_version
    }

    #[must_use]
    pub const fn scene_format_version(&self) -> u32 {
        self.scene_format_version
    }

    #[must_use]
    pub fn directories(&self) -> &[ProjectDirectory] {
        &self.directories
    }

    #[must_use]
    pub fn members(&self) -> &[ProjectMemberReceipt] {
        &self.members
    }

    #[must_use]
    pub fn member(&self, member: ProjectMember) -> Option<ProjectMemberReceipt> {
        self.members
            .iter()
            .copied()
            .find(|receipt| receipt.member == member)
    }
}

impl ProjectCreationPlan {
    #[must_use]
    pub const fn starter(&self) -> ProjectStarter {
        ProjectStarter::Desktop2dV1
    }

    #[must_use]
    pub const fn slug(&self) -> &ProjectSlug {
        &self.slug
    }

    #[must_use]
    pub const fn dependency(&self) -> &super::dependency::NaraDependencyReceipt {
        &self.dependency
    }

    #[must_use]
    pub const fn directories(&self) -> &'static [ProjectDirectory] {
        &PROJECT_DIRECTORIES
    }

    #[must_use]
    pub fn members(&self) -> &[ProjectMemberPlan] {
        &self.members
    }

    #[must_use]
    pub fn member(&self, member: ProjectMember) -> Option<&ProjectMemberPlan> {
        self.members.iter().find(|plan| plan.member == member)
    }

    #[must_use]
    pub(crate) fn receipt(&self) -> ProjectCreationPlanReceipt {
        let starter = self.starter();
        ProjectCreationPlanReceipt {
            starter,
            slug: self.slug.clone(),
            dependency: self.dependency.clone(),
            manifest_schema_version: starter.manifest_schema_version(),
            scene_format_version: starter.scene_format_version(),
            directories: self.directories().to_vec().into_boxed_slice(),
            members: self
                .members
                .iter()
                .map(|member| ProjectMemberReceipt {
                    member: member.member(),
                    digest: member.digest(),
                })
                .collect(),
        }
    }
}

impl Debug for ProjectCreationPlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationPlan")
            .field("starter", &self.starter())
            .field("slug", &self.slug)
            .field("dependency", &self.dependency)
            .field("member_count", &self.members.len())
            .finish()
    }
}

const PROJECT_DIRECTORIES: [ProjectDirectory; 6] = [
    ProjectDirectory::Assets,
    ProjectDirectory::Prefabs,
    ProjectDirectory::Scenes,
    ProjectDirectory::Schema,
    ProjectDirectory::Source,
    ProjectDirectory::SourceBinaries,
];

const DESKTOP_2D_V1_AUTHORING_SCHEMA_PROVIDERS: [ComponentSchemaProviderDefinition; 5] = [
    nara_scene::SCENE_COMPONENTS_SCHEMA_PROVIDER,
    nara_transform::TRANSFORM_SCHEMA_PROVIDER,
    nara_render::RENDER_SCHEMA_PROVIDER,
    nara_sprite::SPRITE_SCHEMA_PROVIDER,
    nara_tilemap::TILEMAP_SCHEMA_PROVIDER,
];

pub(crate) fn build_desktop_2d_v1_plan(
    slug: ProjectSlug,
    dependency_manifest_fragment: &str,
    creation_command_arguments: &str,
    dependency_receipt: NaraDependencyReceipt,
) -> Result<ProjectCreationPlan, &'static str> {
    let cargo_manifest = render_cargo_manifest(&slug, dependency_manifest_fragment);
    let project_manifest = render_project_manifest(&slug);
    let authoring_schema_bundle = desktop_2d_v1_authoring_schema_bundle()?;
    let authoring_schema_bundle_json = authoring_schema_bundle
        .to_json_string()
        .map_err(|_| "starter authoring schema bundle could not be encoded")?;
    let author_guide = render_author_guide(
        &slug,
        dependency_manifest_fragment,
        creation_command_arguments,
    );
    product_contract::validate(
        &slug,
        &dependency_receipt,
        ProductContractSources {
            cargo_manifest: cargo_manifest.as_bytes(),
            project_manifest: project_manifest.as_bytes(),
            authoring_schema_bundle: authoring_schema_bundle_json.as_bytes(),
            expected_authoring_schema_bundle: &authoring_schema_bundle,
            dependency_manifest_fragment,
            creation_command_arguments,
            author_guide: &author_guide,
            library: LIBRARY_SOURCE,
            headless: HEADLESS_SOURCE,
            desktop: DESKTOP_SOURCE,
            startup_scene: STARTUP_SCENE,
        },
    )?;

    let members = [
        member(ProjectMember::CargoManifest, cargo_manifest.as_bytes())?,
        member(ProjectMember::ProjectManifest, project_manifest.as_bytes())?,
        member(ProjectMember::AuthorGuide, author_guide.as_bytes())?,
        member(ProjectMember::LibrarySource, LIBRARY_SOURCE)?,
        member(ProjectMember::HeadlessSource, HEADLESS_SOURCE)?,
        member(ProjectMember::DesktopSource, DESKTOP_SOURCE)?,
        member(ProjectMember::StartupScene, STARTUP_SCENE)?,
        member(
            ProjectMember::AuthoringSchemaBundle,
            authoring_schema_bundle_json.as_bytes(),
        )?,
    ];
    if members.iter().any(|member| {
        member.bytes().windows(2).any(|window| window == b"{{")
            || member
                .bytes()
                .windows(b"reference-game".len())
                .any(|window| window == b"reference-game")
    }) {
        return Err("starter members contain a forbidden placeholder or fixture identity");
    }

    Ok(ProjectCreationPlan {
        slug,
        dependency: dependency_receipt,
        members: Box::new(members),
    })
}

fn desktop_2d_v1_authoring_schema_bundle() -> Result<AuthoringSchemaBundle, &'static str> {
    AuthoringSchemaBundle::from_trusted_provider_definitions(
        DESKTOP_2D_V1_AUTHORING_SCHEMA_PROVIDERS,
    )
    .map_err(|_| "starter authoring schema bundle could not be composed")
}

fn render_author_guide(
    slug: &ProjectSlug,
    dependency_manifest_fragment: &str,
    creation_command_arguments: &str,
) -> String {
    format!(
        "# {} author guide\n\n## Pinned identity\n\n- Starter: `desktop-2d/v1`\n- Nara dependency: `{dependency_manifest_fragment}`\n\n## Creation command\n\nRun this PowerShell command from the authorized destination parent. Local paths use literal quoting:\n\n```powershell\nnara new desktop-2d/v1 {} --parent . {creation_command_arguments}\n```\n\n{body}",
        slug.as_str(),
        slug.as_str(),
        body = AUTHOR_GUIDE_BODY,
    )
}

fn member(member: ProjectMember, bytes: &[u8]) -> Result<ProjectMemberPlan, &'static str> {
    if bytes.len() > MAX_STARTER_MEMBER_BYTES {
        return Err("starter member exceeds its byte limit");
    }
    Ok(ProjectMemberPlan {
        member,
        bytes: bytes.into(),
        digest: ContentDigest::of_bytes(bytes),
    })
}

fn validate_project_slug(value: &str) -> Result<(), ProjectSlugError> {
    let error = |kind| ProjectSlugError { kind };
    if value.is_empty() {
        return Err(error(ProjectSlugErrorKind::Empty));
    }
    if value.len() > MAX_PROJECT_SLUG_BYTES {
        return Err(error(ProjectSlugErrorKind::TooLong));
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(error(ProjectSlugErrorKind::InvalidStart));
    }
    if !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return Err(error(ProjectSlugErrorKind::InvalidEnd));
    }
    if bytes
        .iter()
        .any(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && *byte != b'-')
    {
        return Err(error(ProjectSlugErrorKind::InvalidCharacter));
    }
    if bytes.windows(2).any(|window| window == b"--") {
        return Err(error(ProjectSlugErrorKind::RepeatedSeparator));
    }
    RelativeComponent::new(value).map_err(|source| match source {
        PathValidationError::ComponentTooLong => error(ProjectSlugErrorKind::TooLong),
        _ => error(ProjectSlugErrorKind::InvalidFilesystemName),
    })?;
    Ok(())
}
