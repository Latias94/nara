use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use serde::{Deserialize, Serialize};

use crate::project_content::ProjectContentMemberReceipt;

use super::*;

mod source_observations;

const SHADER: &[u8] = b"fn surface() -> f32 { return 1.0; }\n";
const TEXTURE: &str = "fe612c0a-6a6b-4dd8-bb84-ad3a4dd678b8";
const AUXILIARY: &str = "d9f0bb6f-4fc8-435d-b37c-287dd0c015a8";

type SourceDependency = (AssetRef, AssetSourceKind, AssetReferenceDemand);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductSource {
    shader: AssetPath,
    dependencies: Vec<SourceDependency>,
    auxiliary: Vec<ImportedProductId>,
}

#[derive(Debug)]
struct Material {
    source: ProductSource,
    shader: Arc<[u8]>,
}

struct MaterialDomain;
struct PaletteDomain;

impl AssetDomain for MaterialDomain {
    type Product = Material;

    fn contains_product(product: &Material, id: ImportedProductId) -> bool {
        id == ImportedProductId::PRIMARY || product.source.auxiliary.contains(&id)
    }
}

impl SourceAssetImporter for MaterialDomain {
    type Settings = ();

    const SOURCE_KIND: &'static str = "test.product-dependencies.material";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(64 * 1024).unwrap(),
            ByteLimit::new(32 * 1024).unwrap(),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<Material, AssetImportError<E>> {
        source
            .metadata()
            .admit::<Self::Settings>()
            .map_err(|_| invalid_product_source())?;
        let body: ProductSource =
            serde_json::from_slice(source.bytes()).map_err(|_| invalid_product_source())?;
        if body.dependencies.len() > 16 || body.auxiliary.len() > 4 {
            return Err(invalid_product_source());
        }
        let shader = source.read_dependency(&body.shader, ByteLimit::new(1024).unwrap())?;
        Ok(Material {
            source: body,
            shader,
        })
    }

    fn visit_source_dependencies<E>(
        product: &Material,
        mut visitor: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        for (reference, kind, demand) in &product.source.dependencies {
            visitor(reference, kind, *demand)?;
        }
        Ok(())
    }
}

impl AssetDomain for PaletteDomain {
    type Product = Material;

    fn contains_product(product: &Material, id: ImportedProductId) -> bool {
        MaterialDomain::contains_product(product, id)
    }
}

impl SourceAssetImporter for PaletteDomain {
    type Settings = ();

    const SOURCE_KIND: &'static str = "test.product-dependencies.palette";

    fn limits() -> AssetImportLimits {
        MaterialDomain::limits()
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<Material, AssetImportError<E>> {
        MaterialDomain::prepare(source)
    }

    fn visit_source_dependencies<E>(
        product: &Material,
        visitor: impl FnMut(&AssetRef, &AssetSourceKind, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        MaterialDomain::visit_source_dependencies(product, visitor)
    }
}

fn invalid_product_source<E>() -> AssetImportError<E> {
    AssetImportError::domain(Diagnostic::error(
        DiagnosticCode::new("test.product-dependencies.source-invalid").unwrap(),
        SafeSummary::new("The material source or its settings are invalid").unwrap(),
    ))
}

fn dependency_project() -> LazyContentProject {
    let project = demand_project();
    fs::write(project.root.join("assets/shared.wgsl"), SHADER).unwrap();
    project
}

fn domain_kind<D: SourceAssetImporter>() -> AssetSourceKind {
    AssetSourceKind::Other(D::SOURCE_KIND.to_owned())
}

fn write_product<D: SourceAssetImporter<Settings = ()>>(
    project: &LazyContentProject,
    name: &str,
    stable: &str,
    dependencies: Vec<SourceDependency>,
    auxiliary: Vec<ImportedProductId>,
) {
    let body = ProductSource {
        shader: AssetPath::new("shared.wgsl").unwrap(),
        dependencies,
        auxiliary,
    };
    fs::write(
        project.root.join(format!("assets/{name}")),
        serde_json::to_vec(&body).unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join(format!("assets/{name}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable.parse().unwrap(),
                AssetPath::new(name).unwrap(),
                domain_kind::<D>(),
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
}

fn write_startup(project: &LazyContentProject, owner: &ComponentSchemaOwnerSnapshot) {
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        document(owner, &[(IMMEDIATE, AssetRef::Product(product(B)))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

fn assert_asset_sources(
    project: &LazyContentProject,
    members: &[ProjectContentMemberReceipt],
    expected: &[&str],
) {
    let mut actual = members
        .iter()
        .filter(|member| member.kind() == ProjectContentMemberKind::AssetSource)
        .map(|member| member.path().as_str())
        .collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
    for member in members {
        assert_eq!(
            member.digest(),
            project.digest(member.path().as_str()),
            "{}",
            member.path().as_str(),
        );
    }
}

fn assert_loader_idle(loader: &ProjectContentLoader) {
    let budget = loader.budget_snapshot();
    assert_eq!(budget.active_generations(), 0);
    assert_eq!(budget.active_reservations(), 0);
    assert_eq!(budget.active(ProjectContentBudgetKind::AggregateBytes), 0);
}

#[test]
fn material_product_prepares_its_texture_and_records_its_raw_shader_input() {
    let project = dependency_project();
    let owner = reference_owner();
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(
        &project,
        "surface.material",
        B,
        vec![(
            AssetRef::Product(product(TEXTURE)),
            AssetSourceKind::Image,
            AssetReferenceDemand::Immediate,
        )],
        vec![],
    );
    write_image(&project, "texture.png", TEXTURE);
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);

    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(snapshot.images()[0].path().as_str(), "assets/texture.png");
    assert_eq!(
        snapshot.images()[0].image().extent(),
        nara_render::image::ImageExtent::new(1, 1),
    );
    assert_eq!(snapshot.images()[0].image().pixels().len(), 4);
    assert!(snapshot.scene_dependencies().is_empty());
    let materials = snapshot
        .domain_assets()
        .collection::<MaterialDomain>()
        .unwrap();
    assert_eq!(materials.iter().len(), 1);
    assert_eq!(
        materials
            .get(&AssetRef::Product(product(B)))
            .unwrap()
            .shader
            .as_ref(),
        SHADER,
    );
    let closure = snapshot.content_closure();
    assert_asset_sources(
        &project,
        closure.members(),
        &[
            "assets/surface.material",
            "assets/texture.png",
            "assets/shared.wgsl",
        ],
    );
    for path in ["assets/surface.material.meta", "assets/texture.png.meta"] {
        assert!(closure.members().iter().any(|member| {
            member.kind() == ProjectContentMemberKind::AssetMetadata
                && member.path().as_str() == path
        }));
    }
    drop(closure);
    let retained = snapshot.domain_assets().clone();
    drop(snapshot);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    assert!(
        loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes)
            > 0
    );
    assert_eq!(
        retained
            .collection::<MaterialDomain>()
            .unwrap()
            .get(&AssetRef::Product(product(B)))
            .unwrap()
            .shader
            .as_ref(),
        SHADER,
    );
    drop(retained);
    assert_loader_idle(&loader);
}

#[test]
fn same_domain_multihop_cycle_and_aliases_import_each_source_once() {
    let project = dependency_project();
    let owner = reference_owner();
    let auxiliary = ImportedProductId::parse_subproduct(AUXILIARY).unwrap();
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(
        &project,
        "a.material",
        B,
        vec![
            (
                AssetRef::path("b.material").unwrap(),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Deferred,
            ),
            (
                AssetRef::StableId(C.parse().unwrap()),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
            (
                AssetRef::Product(product(C)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
        ],
        vec![auxiliary],
    );
    write_product::<MaterialDomain>(
        &project,
        "b.material",
        C,
        vec![
            (
                AssetRef::Product(product(D)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
            (
                AssetRef::Product(AssetProductRef::new(product(B).source(), auxiliary).unwrap()),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
        ],
        vec![],
    );
    write_product::<MaterialDomain>(
        &project,
        "c.material",
        D,
        vec![
            (
                AssetRef::Product(product(B)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
            (
                AssetRef::Product(product(C)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            ),
        ],
        vec![],
    );
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);

    let snapshot = loader.load(&candidate, &plan).unwrap();
    let materials = snapshot
        .domain_assets()
        .collection::<MaterialDomain>()
        .unwrap();
    assert_eq!(materials.iter().len(), 3);
    assert_eq!(materials.source_records().len(), 3);
    let a = materials.get(&AssetRef::Product(product(B))).unwrap();
    for (stable, path) in [(B, "a.material"), (C, "b.material"), (D, "c.material")] {
        let value = materials.get(&AssetRef::Product(product(stable))).unwrap();
        assert!(std::ptr::eq(
            value,
            materials
                .get(&AssetRef::StableId(stable.parse().unwrap()))
                .unwrap(),
        ));
        assert!(std::ptr::eq(
            value,
            materials.get(&AssetRef::path(path).unwrap()).unwrap(),
        ));
        assert!(Arc::ptr_eq(&a.shader, &value.shader));
    }
    assert!(std::ptr::eq(
        a,
        materials
            .get(&AssetRef::Product(
                AssetProductRef::new(product(B).source(), auxiliary).unwrap(),
            ))
            .unwrap(),
    ));
    let declared_edges = materials
        .iter()
        .map(|material| material.source.dependencies.len())
        .sum::<usize>();
    assert_eq!(
        loader
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::DependencyEdges),
        1 + declared_edges + materials.iter().len(),
        "one Scene reference, every declared edge and one raw read per source",
    );
    assert_asset_sources(
        &project,
        snapshot.content_closure().members(),
        &[
            "assets/a.material",
            "assets/b.material",
            "assets/c.material",
            "assets/shared.wgsl",
        ],
    );
    drop(snapshot);
    assert_loader_idle(&loader);
}

#[test]
fn cross_domain_discovery_revisits_the_first_provider_in_either_registration_order() {
    for material_first in [true, false] {
        let project = dependency_project();
        let owner = reference_owner();
        write_startup(&project, &owner);
        write_product::<MaterialDomain>(
            &project,
            "a.material",
            B,
            vec![(
                AssetRef::Product(product(C)),
                domain_kind::<PaletteDomain>(),
                AssetReferenceDemand::Immediate,
            )],
            vec![],
        );
        write_product::<PaletteDomain>(
            &project,
            "colors.palette",
            C,
            vec![(
                AssetRef::Product(product(D)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            )],
            vec![],
        );
        write_product::<MaterialDomain>(
            &project,
            "b.material",
            D,
            vec![(
                AssetRef::Product(product(B)),
                domain_kind::<MaterialDomain>(),
                AssetReferenceDemand::Immediate,
            )],
            vec![],
        );
        let recipe = if material_first {
            data_recipe(&owner)
                .add_asset_domain::<MaterialDomain>()
                .unwrap()
                .add_asset_domain::<PaletteDomain>()
                .unwrap()
        } else {
            data_recipe(&owner)
                .add_asset_domain::<PaletteDomain>()
                .unwrap()
                .add_asset_domain::<MaterialDomain>()
                .unwrap()
        };
        let (candidate, plan, loader) = demand_inputs(&project, recipe);

        let snapshot = loader.load(&candidate, &plan).unwrap();
        let materials = snapshot
            .domain_assets()
            .collection::<MaterialDomain>()
            .unwrap();
        let palettes = snapshot
            .domain_assets()
            .collection::<PaletteDomain>()
            .unwrap();
        assert_eq!(materials.iter().len(), 2);
        assert_eq!(materials.source_records().len(), 2);
        assert_eq!(palettes.iter().len(), 1);
        let a = materials.get(&AssetRef::Product(product(B))).unwrap();
        let b = materials.get(&AssetRef::Product(product(D))).unwrap();
        let palette = palettes.get(&AssetRef::Product(product(C))).unwrap();
        assert!(materials.get(&AssetRef::Product(product(C))).is_none());
        assert!(palettes.get(&AssetRef::Product(product(B))).is_none());
        assert!(Arc::ptr_eq(&a.shader, &b.shader));
        assert!(Arc::ptr_eq(&a.shader, &palette.shader));
        let import_edges = materials
            .iter()
            .chain(palettes.iter())
            .map(|material| material.source.dependencies.len() + 1)
            .sum::<usize>();
        assert_eq!(
            loader
                .budget_snapshot()
                .high_water(ProjectContentBudgetKind::DependencyEdges),
            1 + import_edges,
            "registration order must not repeat previous imports",
        );
        assert_asset_sources(
            &project,
            snapshot.content_closure().members(),
            &[
                "assets/a.material",
                "assets/b.material",
                "assets/colors.palette",
                "assets/shared.wgsl",
            ],
        );
        drop(snapshot);
        assert_loader_idle(&loader);
    }
}

#[test]
fn product_dependency_kind_is_checked_before_immediate_or_deferred_preparation() {
    for demand in [
        AssetReferenceDemand::Immediate,
        AssetReferenceDemand::Deferred,
    ] {
        let project = dependency_project();
        let owner = reference_owner();
        write_startup(&project, &owner);
        write_image(&project, "texture.png", TEXTURE);
        write_product::<MaterialDomain>(
            &project,
            "surface.material",
            B,
            vec![(
                AssetRef::Product(product(TEXTURE)),
                domain_kind::<MaterialDomain>(),
                demand,
            )],
            vec![],
        );
        let recipe = data_recipe(&owner)
            .add_asset_domain::<MaterialDomain>()
            .unwrap();
        let (candidate, plan, loader) = demand_inputs(&project, recipe);

        let failure = loader.load(&candidate, &plan).unwrap_err();
        assert_eq!(failure.kind(), ProjectContentErrorKind::AssetReference);
        assert!(failure.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.content.dependency-kind-mismatch"
        }));
        assert_loader_idle(&loader);
    }
}

#[test]
fn a_late_auxiliary_selection_requires_actual_membership_and_can_recover() {
    let project = dependency_project();
    let owner = reference_owner();
    let auxiliary = ImportedProductId::parse_subproduct(AUXILIARY).unwrap();
    let selection =
        AssetRef::Product(AssetProductRef::new(product(B).source(), auxiliary).unwrap());
    write_startup(&project, &owner);
    let dependencies = || {
        vec![(
            AssetRef::Product(product(C)),
            domain_kind::<MaterialDomain>(),
            AssetReferenceDemand::Immediate,
        )]
    };
    write_product::<MaterialDomain>(&project, "a.material", B, dependencies(), vec![]);
    write_product::<MaterialDomain>(
        &project,
        "b.material",
        C,
        vec![(
            selection.clone(),
            domain_kind::<MaterialDomain>(),
            AssetReferenceDemand::Immediate,
        )],
        vec![],
    );
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);

    let failure = loader.load(&candidate, &plan).unwrap_err();
    assert_eq!(failure.kind(), ProjectContentErrorKind::AssetReference);
    assert!(failure.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-product-missing"
    }));
    assert_loader_idle(&loader);

    write_product::<MaterialDomain>(&project, "a.material", B, dependencies(), vec![auxiliary]);
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let materials = snapshot
        .domain_assets()
        .collection::<MaterialDomain>()
        .unwrap();
    assert_eq!(materials.iter().len(), 2);
    assert!(std::ptr::eq(
        materials.get(&selection).unwrap(),
        materials.get(&AssetRef::Product(product(B))).unwrap(),
    ));
    drop(snapshot);
    assert_loader_idle(&loader);
}

#[test]
fn deferred_product_bodies_are_excluded_from_current_content_but_validated_for_delivery() {
    let project = dependency_project();
    let owner = reference_owner();
    let future = AssetRef::Product(product(C));
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(
        &project,
        "surface.material",
        B,
        vec![(
            future.clone(),
            domain_kind::<PaletteDomain>(),
            AssetReferenceDemand::Deferred,
        )],
        vec![],
    );
    write_product::<PaletteDomain>(&project, "future.palette", C, vec![], vec![]);
    write_image(&project, "future.png", TEXTURE);
    fs::write(
        project.root.join("assets/future.palette"),
        b"invalid future body",
    )
    .unwrap();
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap()
        .add_asset_domain::<PaletteDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);

    let current = loader.load(&candidate, &plan).unwrap();
    let original_digest = current.content_digest();
    let original_members = current.content_closure().members().to_vec();
    assert!(current.images().is_empty());
    assert!(
        current
            .domain_assets()
            .collection::<PaletteDomain>()
            .is_none()
    );
    assert!(current.asset_resolution().resolve_ref(&future).is_ok());
    assert!(original_members.iter().any(|member| {
        member.kind() == ProjectContentMemberKind::AssetMetadata
            && member.path().as_str() == "assets/future.palette.meta"
    }));
    assert_asset_sources(
        &project,
        &original_members,
        &["assets/surface.material", "assets/shared.wgsl"],
    );
    drop(current);
    assert_loader_idle(&loader);

    fs::write(
        project.root.join("assets/future.palette"),
        b"different invalid future body",
    )
    .unwrap();
    let current = loader.load(&candidate, &plan).unwrap();
    assert_eq!(current.content_digest(), original_digest);
    assert_eq!(current.content_closure().members(), original_members);
    let failure = loader.load_delivery(&candidate, &plan).unwrap_err();
    assert_eq!(failure.kind(), ProjectContentErrorKind::AssetDomain);
    assert!(failure.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "test.product-dependencies.source-invalid"
    }));
    assert_eq!(current.content_digest(), original_digest);
    assert!(
        current
            .domain_assets()
            .collection::<PaletteDomain>()
            .is_none()
    );
    drop(current);
    assert_loader_idle(&loader);

    write_product::<PaletteDomain>(
        &project,
        "future.palette",
        C,
        vec![(
            AssetRef::Product(product(TEXTURE)),
            AssetSourceKind::Image,
            AssetReferenceDemand::Immediate,
        )],
        vec![],
    );
    let delivery = loader.load_delivery(&candidate, &plan).unwrap();
    assert_eq!(delivery.startup_content().content_digest(), original_digest);
    let ordinary_startup_members = delivery
        .startup_content()
        .members()
        .iter()
        .filter(|member| member.kind() != ProjectContentMemberKind::AuthoringSchemaBundle)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(ordinary_startup_members, original_members);
    assert_eq!(delivery.scene_roots().len(), 1);
    assert_asset_sources(
        &project,
        delivery.members(),
        &[
            "assets/surface.material",
            "assets/future.palette",
            "assets/future.png",
            "assets/shared.wgsl",
        ],
    );
    drop(delivery);
    assert_loader_idle(&loader);
}

#[test]
fn product_cycles_share_the_dependency_edge_budget_and_release_failed_work() {
    let project = dependency_project();
    let owner = reference_owner();
    write_startup(&project, &owner);
    write_product::<MaterialDomain>(
        &project,
        "a.material",
        B,
        vec![(
            AssetRef::Product(product(C)),
            domain_kind::<MaterialDomain>(),
            AssetReferenceDemand::Immediate,
        )],
        vec![],
    );
    write_product::<MaterialDomain>(
        &project,
        "b.material",
        C,
        vec![(
            AssetRef::Product(product(B)),
            domain_kind::<MaterialDomain>(),
            AssetReferenceDemand::Immediate,
        )],
        vec![],
    );
    let recipe = data_recipe(&owner)
        .add_asset_domain::<MaterialDomain>()
        .unwrap();
    let (candidate, plan, loader) = demand_inputs(&project, recipe);
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let digest = snapshot.content_digest();
    let required = loader
        .budget_snapshot()
        .high_water(ProjectContentBudgetKind::DependencyEdges);
    drop(snapshot);
    assert_loader_idle(&loader);

    let exact_limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::DependencyEdges, required)
        .unwrap();
    let exact = ProjectContentLoader::with_limits(loader.project_root(), exact_limits).unwrap();
    let snapshot = exact.load(&candidate, &plan).unwrap();
    assert_eq!(snapshot.content_digest(), digest);
    drop(snapshot);
    assert_loader_idle(&exact);

    let short_limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::DependencyEdges, required - 1)
        .unwrap();
    let short = ProjectContentLoader::with_limits(loader.project_root(), short_limits).unwrap();
    let failure = short.load(&candidate, &plan).unwrap_err();
    assert_eq!(failure.kind(), ProjectContentErrorKind::BudgetExceeded);
    assert_loader_idle(&short);

    write_product::<MaterialDomain>(&project, "b.material", C, vec![], vec![]);
    let snapshot = short.load(&candidate, &plan).unwrap();
    assert_eq!(
        snapshot
            .domain_assets()
            .collection::<MaterialDomain>()
            .unwrap()
            .iter()
            .len(),
        2,
    );
    drop(snapshot);
    assert_loader_idle(&short);
}
