use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::AssetRef,
    ecs::{Component, Res, ResMut, Resource},
    fs::RelativePath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectDomainAssets,
        ingest_project_manifest, resolve_product_recipe,
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
};
use nara_asset_domain_consumer::TextDomain;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.editor.Caption",
    version = 1,
    alias = "Caption",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct Caption {
    #[nara(id = "text", alias = "Text", capabilities(scene, asset_ref))]
    pub text: AssetRef,
}
nara::persistent_components! { pub mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("consumer.editor.caption");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.editor"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.editor.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct CaptionPlugin;
#[derive(Clone, Default, Debug, Resource)]
struct TextResult(String);

impl Plugin for CaptionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.editor.caption"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<TextResult>()?.add_systems(
            CoreStage::Update,
            |assets: Res<ProjectDomainAssets>, mut result: ResMut<TextResult>| {
                let text = assets
                    .collection::<TextDomain>()
                    .unwrap()
                    .get(&AssetRef::path("caption.txt").unwrap())
                    .unwrap();
                result.0.clone_from(&text.text);
            },
        )?;
        Ok(())
    }
}

pub fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<CaptionPlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_asset_domain::<TextDomain>()
        .unwrap()
}

pub struct TextProject {
    pub root: PathBuf,
}

impl TextProject {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-editor-text-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for path in ["assets", "prefabs", "scenes", "schema", "scripts"] {
            fs::create_dir(root.join(path)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '24ed5e36-1b37-5fa3-9c42-232066b17916'\nnative_document_identity = 'project_location'\nname = \"Editor text\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(root.join("assets/caption.txt"), b"suffix.txt\nHello").unwrap();
        fs::write(root.join("assets/suffix.txt"), b" world").unwrap();
        let project = Self { root };
        let directory = project.capability();
        let manifest = directory
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    pub fn capability(&self) -> nara::fs::DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = fs::File::open(&self.root).unwrap();
        nara::fs::DirectoryCapability::from_host_handle(
            handle,
            nara::fs::HostCapabilityOptions::new(
                nara::fs::CapabilityRights::ReadWrite,
                nara::fs::TrustMode::TrustedLocal,
            ),
        )
        .unwrap()
    }

    #[allow(
        dead_code,
        reason = "Shared by integration binaries that do not all exercise source-backed Play."
    )]
    pub fn reference_text(&self) {
        let caption = Caption {
            text: AssetRef::path("caption.txt").unwrap(),
        };
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("caption").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("consumer.editor.Caption"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                caption.__encode_persistent_component().unwrap(),
            ),
        )]);
        fs::write(
            self.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
    }

    #[allow(
        dead_code,
        reason = "Shared by integration binaries that do not all exercise source-backed Play."
    )]
    pub fn assert_check_and_play(&self) {
        self.reference_text();
        let receipt = nara::project_workflow::check_project(
            nara::project_workflow::ProjectCheckRequest::new(self.capability(), recipe()),
        )
        .unwrap();
        assert!(
            receipt
                .members()
                .iter()
                .any(|member| member.path().as_str() == "assets/suffix.txt")
        );
        let mut run = HeadlessRun::<TextResult>::new(
            self.capability(),
            HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::ZERO)
                .with_recipe(recipe()),
            Vec::new(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let report = run.execute_bounded();
            match report.outcome() {
                HeadlessRunOutcome::Completed(text) => {
                    assert_eq!(text.0, "Hello world");
                    break;
                }
                HeadlessRunOutcome::Failed => panic!("Play failed: {:?}", report.diagnostics()),
                HeadlessRunOutcome::CleanupIncomplete => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

impl Drop for TextProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
