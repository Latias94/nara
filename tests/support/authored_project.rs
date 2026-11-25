use std::path::{Path, PathBuf};

use nara::{
    fs::{RelativeComponent, RelativePath},
    project_tooling::host_fs,
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, ProjectStarter, prepare_desktop_2d_project,
    },
};

pub(crate) fn create_authored_project(
    checkout: &Path,
    parent: &Path,
    slug: &str,
    starter: ProjectStarter,
) -> PathBuf {
    let checkout = checkout.canonicalize().unwrap();
    let parent = parent.canonicalize().unwrap();
    let ancestor = if parent.starts_with(&checkout) {
        checkout.as_path()
    } else {
        checkout.parent().unwrap()
    };
    let checkout_relative = (ancestor != checkout.as_path())
        .then(|| RelativePath::new(checkout.file_name().unwrap()).unwrap());
    let parent_relative = RelativePath::from_components(
        parent
            .strip_prefix(ancestor)
            .unwrap()
            .iter()
            .map(|name| RelativeComponent::new(name).unwrap()),
    )
    .unwrap();
    let shared = host_fs::writable_directory(ancestor).unwrap();
    let publication_parent = shared.open_directory(&parent_relative).unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        starter,
        publication_parent,
        ProjectSlug::new(slug).unwrap(),
        NaraDependencySource::local(LocalDependencyRelation::new(
            shared,
            checkout_relative,
            Some(parent_relative),
        )),
    ))
    .unwrap();
    let outcome = prepared.publish();
    let ProjectCreationOutcome::Created(receipt) = outcome else {
        panic!("authored starter publication failed: {outcome:?}");
    };
    assert_eq!(receipt.plan().starter(), starter);
    let root = parent.join(slug).canonicalize().unwrap();
    assert!(
        receipt
            .project_root()
            .same_object(&host_fs::readable_directory(&root).unwrap())
            .unwrap()
    );
    root
}
