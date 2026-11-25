use nara::{
    fs::DirectoryCapability,
    project_host::{ProjectCandidateError, open_product_project_root},
};

pub fn open_project_root() -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    open_product_project_root(env!("CARGO_MANIFEST_DIR"))
        .map_err(|error| Box::new(ProjectCandidateError::from_manifest_authority(error)))
}
