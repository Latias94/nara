use std::{error::Error, fs::File, io::Read, path::Path};

use nara::{
    prelude::*,
    reflect::{ComponentSchemaCatalog, component_registry},
    scene::{
        authoring::{
            SceneDocument, SceneEntity, builtin_component_schemas, prefab::expand_prefabs,
        },
        cook::cook_flattened_scene,
    },
};

use crate::{
    RunRules,
    scene_roles::{ExtractionGate, PatrolDrone, Pilot, PowerCell},
};

use super::{ArenaPlan, SceneRow};

#[cfg(all(feature = "desktop", feature = "hot-reload"))]
mod snapshot;
#[cfg(all(feature = "desktop", feature = "hot-reload"))]
pub(crate) use snapshot::SceneLayers;

const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) struct SourceProject {
    schemas: ComponentSchemaCatalog,
    catalog: ComponentSchemaCatalog,
}

#[cfg(test)]
pub(crate) fn test_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

impl SourceProject {
    pub fn new(app: &App) -> Result<Self, Box<dyn Error>> {
        let schemas = component_registry(app.world())
            .ok_or("could not read the Night Relay Schema catalog")?
            .clone();
        let catalog = ComponentSchemaCatalog::from_components(
            builtin_component_schemas()?
                .into_iter()
                .chain(schemas.components().iter().cloned()),
        )?;
        Ok(Self { schemas, catalog })
    }

    pub fn read_snapshot(
        &self,
        assets: &RuntimeAssetSource,
        path: &Path,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let read = || -> Result<SceneDocument, Box<dyn Error>> {
            let mut bytes = Vec::new();
            File::open(path)?
                .take(MAX_DOCUMENT_BYTES + 1)
                .read_to_end(&mut bytes)?;
            self.parse(&bytes)
        };
        let scene =
            read().map_err(|error| format!("could not load {}: {error}", path.display()))?;
        self.expand(assets, &scene)
    }

    pub fn read_scene(
        &self,
        assets: &RuntimeAssetSource,
        id: AssetId,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        self.expand(assets, &self.read_document(assets, id, "nara.scene")?)
    }

    pub fn cook(&self, scene: &SceneDocument) -> Result<SceneAsset, Box<dyn Error>> {
        ArenaPlan::prepare(scene.entities.iter().map(read_row))?;
        let cooked = cook_flattened_scene(scene, &self.schemas)?;
        Ok(SceneAsset::from_cooked(cooked.bytes)?)
    }

    fn expand(
        &self,
        assets: &RuntimeAssetSource,
        scene: &SceneDocument,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let mut read_error = None;
        let expanded = expand_prefabs(scene, &self.catalog, |id| {
            self.read_document(assets, id, "nara.prefab")
                .map_err(|error| {
                    let message = error.to_string();
                    read_error = Some(error);
                    message
                })
        });
        if let Some(error) = read_error {
            return Err(error);
        }
        Ok(expanded?.document)
    }

    fn read_document(
        &self,
        assets: &RuntimeAssetSource,
        id: AssetId,
        importer: &str,
    ) -> Result<SceneDocument, Box<dyn Error>> {
        let backend = assets.backend();
        if backend
            .meta(id)
            .is_none_or(|meta| meta.importer() != importer)
        {
            return Err(format!("asset {id} does not use the {importer} importer").into());
        }
        self.parse(&backend.read_asset(id, None)?)
            .map_err(|error| format!("could not load asset {id}: {error}").into())
    }

    fn parse(&self, bytes: &[u8]) -> Result<SceneDocument, Box<dyn Error>> {
        if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(format!("scene or prefab exceeds {MAX_DOCUMENT_BYTES} bytes").into());
        }
        Ok(SceneDocument::parse_with(
            std::str::from_utf8(bytes)?,
            self.schemas.components(),
        )?)
    }
}

fn read_row(source: &SceneEntity) -> Result<SceneRow<'_>, String> {
    if source.prefab.is_some() {
        return Err(format!("{} is an unresolved prefab instance", source.name));
    }
    Ok(SceneRow {
        name: &source.name,
        is_child: source.parent.is_some(),
        pose: component::<Transform2d>(source)?,
        visibility: component::<Visibility>(source)?,
        collider: component(source)?,
        pilot: component::<Pilot>(source)?.is_some(),
        cell_order: component::<PowerCell>(source)?.map(|cell| cell.order),
        drone_order: component::<PatrolDrone>(source)?.map(|drone| drone.order),
        gate: component::<ExtractionGate>(source)?.is_some(),
        rules: component::<RunRules>(source)?,
    })
}

fn component<T: Schema>(source: &SceneEntity) -> Result<Option<T>, String> {
    source.typed::<T>().map_err(|error| {
        format!(
            "{} ({}) {}: {error}",
            source.name,
            source.id,
            T::COMPONENT_ID
        )
    })
}
