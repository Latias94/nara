//! Private copied authoring sources for the ordinary game's finite workload populations.

use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    ops::Range,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        AssetSourcePath, ImportedProductId, StableAssetId,
    },
    core::{EngineVersion, FormatGenerator, FormatKind, PersistentFileEnvelope},
    fs::ContentDigest,
    material::SamplerDescriptor,
    prelude::Vec2,
    reflect::{
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, encode_asset_reference_value,
    },
    scene::{
        PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance,
        SceneComponentRecord, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
        SceneFileLimits, SourceDocumentKey, SourceDocumentKind,
    },
    tilemap::{TileAtlasLayout, TileSetSource},
};
use serde_json::{Map, Value, json};

const PREFIX: &str = "nara-runtime-workload-source-";
const OWNER_FILE: &str = ".runtime-workload-owner";
const MAX_COPY_ENTRIES: usize = 256;
const MAX_COPY_DEPTH: usize = 16;
const MAX_COPY_BYTES: usize = 64 * 1024 * 1024;
const TILEMAP: &str = "nara.tilemap.Tilemap";
const PREFAB_ROOT: &str = "terrain-half";
const LEVELS: [(&str, &str); 2] = [
    ("meadow", "f516527c-4e94-45b3-9941-13e659ccfb07"),
    ("cavern", "a64c4823-8686-41a7-ae87-9a483931d91b"),
];

pub struct Project {
    temp_parent: PathBuf,
    root: PathBuf,
    owner: String,
    population: usize,
    report: Value,
    cleaned: bool,
}

impl Project {
    pub fn new(population: usize) -> Result<Self, String> {
        let side = match population {
            64 => 8,
            4096 => 64,
            16384 => 128,
            _ => {
                return Err(format!(
                    "unsupported source workload population: {population}"
                ));
            }
        };
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        require_directory(source)?;
        let source = source.canonicalize().map_err(|error| error.to_string())?;
        let temp_parent = std::env::temp_dir()
            .canonicalize()
            .map_err(|error| format!("resolve workload temp parent: {error}"))?;
        require_directory(&temp_parent)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let owner = format!(
            "{PREFIX}{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        );
        let root = temp_parent.join(&owner);
        fs::create_dir(&root)
            .map_err(|error| format!("create workload source {}: {error}", root.display()))?;
        let mut project = Self {
            temp_parent,
            root,
            owner,
            population,
            report: Value::Null,
            cleaned: false,
        };
        write_file(
            &project.root.join(OWNER_FILE),
            project.owner.as_bytes(),
            false,
        )?;
        project.prepare(&source, side)?;
        Ok(project)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source_report(&self) -> Value {
        self.report.clone()
    }

    /// Call only after the original Runtime and every retained GPU replay have retired.
    pub fn cleanup(&mut self) -> Result<(), String> {
        if self.cleaned {
            return Ok(());
        }
        require_directory(&self.temp_parent)?;
        require_directory(&self.root)?;
        let parent = self
            .temp_parent
            .canonicalize()
            .map_err(|error| format!("resolve original temp parent: {error}"))?;
        let root = self
            .root
            .canonicalize()
            .map_err(|error| format!("resolve owned source directory: {error}"))?;
        if parent != self.temp_parent
            || root != self.root
            || root.parent() != Some(parent.as_path())
            || root.file_name().and_then(|name| name.to_str()) != Some(self.owner.as_str())
            || !self.owner.starts_with(PREFIX)
        {
            return Err("refusing cleanup: owned source directory or temp parent changed".into());
        }
        let owner = read_regular(&root.join(OWNER_FILE), 1024)?;
        if owner != self.owner.as_bytes() {
            return Err("refusing cleanup: source directory ownership marker changed".into());
        }
        fs::remove_dir_all(&root)
            .map_err(|error| format!("remove owned source {}: {error}", root.display()))?;
        self.cleaned = true;
        Ok(())
    }

    fn prepare(&mut self, source: &Path, side: i32) -> Result<(), String> {
        let mut budget = CopyBudget::default();
        let mut files = BTreeMap::new();
        for name in ["assets", "scenes", "schema", "prefabs", "nara.toml"] {
            copy_source(
                source,
                &source.join(name),
                &self.root.join(name),
                0,
                &mut budget,
                &mut files,
            )?;
        }
        let copied_files = files.values().cloned().collect::<Vec<_>>();
        let tile_size = 256.0 / side as f32;
        let mut products = Vec::new();
        let mut prefabs = Vec::new();
        fs::create_dir(self.root.join("assets/tilesets"))
            .map_err(|error| format!("create copied TileSet directory: {error}"))?;
        if self.population == 16384 {
            fs::create_dir(self.root.join("prefabs/workload"))
                .map_err(|error| format!("create copied Prefab directory: {error}"))?;
        }
        for (level, stable_id) in LEVELS {
            let tileset = format!("tilesets/{level}.tileset.ron");
            let id = StableAssetId::parse_str(stable_id).map_err(|error| error.to_string())?;
            let product = AssetProductRef::new(id, ImportedProductId::PRIMARY)
                .map_err(|error| error.to_string())?;
            products.push(json!({"level": level, "product": product, "source": tileset}));
            let source = TileSetSource::new(
                AssetRef::path(format!("textures/{level}.png"))
                    .map_err(|error| error.to_string())?,
                TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 1, 1),
            )
            .with_sampler(SamplerDescriptor::NEAREST_CLAMP);
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    id,
                    AssetPath::new(&tileset).map_err(|error| error.to_string())?,
                    AssetSourceKind::Other("tileset".into()),
                ),
                (),
            );
            let tileset_bytes = source
                .encode_ron_bytes()
                .map_err(|error| error.to_string())?;
            let metadata_bytes = metadata
                .to_json_string()
                .map_err(|error| error.to_string())?;
            self.write_asset(
                &format!("assets/{tileset}"),
                tileset_bytes.as_bytes(),
                &mut files,
            )?;
            self.write_asset(
                &format!("assets/{tileset}.meta"),
                metadata_bytes.as_bytes(),
                &mut files,
            )?;

            let scene_path = format!("scenes/{level}.scene.json");
            let bytes = read_regular(
                &self.root.join(&scene_path),
                SceneFileLimits::default().encoded_bytes().get(),
            )?;
            let mut scene: Value = serde_json::from_slice(&bytes)
                .map_err(|error| format!("read copied {scene_path}: {error}"))?;
            let entities = scene
                .pointer("/payload/entities")
                .and_then(Value::as_array)
                .ok_or("copied Scene lacks entities")?;
            if entities.len() != 6 {
                return Err(format!("{scene_path} must retain six authored members"));
            }
            let columns = if side == 128 { 64 } else { side };
            let record = tilemap_record(side, tile_size, 0..columns, &tileset)?;
            let components = entity_components(&mut scene, "checkpoint")?;
            if components.contains_key(TILEMAP) {
                return Err("copied checkpoint already has a Tilemap".into());
            }
            components.insert(
                TILEMAP.into(),
                serde_json::to_value(record).map_err(|error| error.to_string())?,
            );
            if side == 128 {
                let alias = format!("workload/{level}-half.prefab.json");
                let prefab_path = format!("prefabs/{alias}");
                let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
                    SceneEntityId::new(PREFAB_ROOT).map_err(|error| error.to_string())?,
                )
                .with_component(
                    ComponentTypeId::new(TILEMAP),
                    tilemap_record(side, tile_size, -96..-32, &tileset)?,
                )]);
                let envelope = PersistentFileEnvelope::canonical_v1(
                    FormatKind::new("prefab").map_err(|error| error.to_string())?,
                    EngineVersion::parse("0.1.0").map_err(|error| error.to_string())?,
                    FormatGenerator::new(
                        "nara",
                        EngineVersion::parse(env!("CARGO_PKG_VERSION"))
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                    prefab,
                );
                self.write_document(
                    &prefab_path,
                    &serde_json::to_vec(&envelope).map_err(|error| error.to_string())?,
                    true,
                    &mut files,
                )?;
                let universe = scene_universe(&self.root)?;
                let instance = PrefabInstance::root(
                    AssetRef::path(&alias).map_err(|error| error.to_string())?,
                    SourceDocumentKey::project_location(
                        universe,
                        AssetSourcePath::new(&prefab_path).map_err(|error| error.to_string())?,
                        SourceDocumentKind::Prefab,
                    ),
                    PrefabEntityTarget::source(
                        SceneEntityId::new(PREFAB_ROOT).map_err(|error| error.to_string())?,
                    ),
                );
                let portal = scene
                    .pointer_mut("/payload/entities")
                    .and_then(Value::as_array_mut)
                    .and_then(|entities| {
                        entities.iter_mut().find(|entity| entity["id"] == "portal")
                    })
                    .and_then(Value::as_object_mut)
                    .ok_or("copied Scene lacks its portal root")?;
                if portal.get("prefab").is_some_and(|value| !value.is_null()) {
                    return Err("copied portal already has a Prefab declaration".into());
                }
                portal.insert(
                    "prefab".into(),
                    serde_json::to_value(instance).map_err(|error| error.to_string())?,
                );
                prefabs.push(prefab_path);
            }
            self.write_document(
                &scene_path,
                &serde_json::to_vec(&scene).map_err(|error| error.to_string())?,
                false,
                &mut files,
            )?;
        }
        let limits = SceneFileLimits::default();
        let shape = limits.shape();
        self.report = json!({
            "format": "two-level-workload-source/v1",
            "population": self.population,
            "cells_per_active_scene": self.population,
            "tile_size": [tile_size, tile_size],
            "world_bounds": {"min": [-120, -128], "max": [136, 128]},
            "authored_scene_members": 6,
            "host_root_ids": if side == 128 {
                vec!["checkpoint", "portal"]
            } else {
                vec!["checkpoint"]
            },
            "tileset_products": products,
            "prefabs": prefabs,
            "prefab_role": if side == 128 {
                "root binding adds Tilemap to portal; no additional member or child"
            } else {
                "unused"
            },
            "source_scope": "source copy; no Runtime admission or startup-input permission",
            "copied_source_files": copied_files,
            "source_files": files.into_values().collect::<Vec<_>>(),
            "copied_entries": budget.entries,
            "copied_bytes": budget.bytes,
            "bounds": {
                "copy_entries": MAX_COPY_ENTRIES,
                "copy_depth": MAX_COPY_DEPTH,
                "copy_bytes": MAX_COPY_BYTES,
                "scene_file_bytes": limits.encoded_bytes().get(),
                "shape_nodes": shape.nodes().get(),
                "shape_container_items": shape.container_items().get(),
                "shape_depth": shape.depth().get(),
                "shape_string_bytes": shape.string_bytes().get(),
                "shape_total_string_bytes": shape.total_string_bytes().get(),
            },
            "document_preflight": "written Scene/Prefab decoded with default SceneFileLimits",
        });
        Ok(())
    }

    fn write_asset(
        &self,
        relative: &str,
        bytes: &[u8],
        files: &mut BTreeMap<String, Value>,
    ) -> Result<(), String> {
        write_file(&self.root.join(relative), bytes, false)?;
        let written = read_regular(
            &self.root.join(relative),
            SceneFileLimits::default().encoded_bytes().get(),
        )?;
        files.insert(relative.into(), file_report(relative, &written));
        Ok(())
    }

    fn write_document(
        &self,
        relative: &str,
        bytes: &[u8],
        prefab: bool,
        files: &mut BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let limits = SceneFileLimits::default();
        if bytes.len() > limits.encoded_bytes().get() {
            return Err(format!(
                "{relative} exceeds the original Scene file byte limit"
            ));
        }
        write_file(&self.root.join(relative), bytes, !prefab)?;
        let written = read_regular(&self.root.join(relative), limits.encoded_bytes().get())?;
        if prefab {
            PrefabDocumentCandidate::decode_json_bytes_with_limits(&written, limits)
                .map_err(|error| format!("default Prefab preflight {relative}: {error}"))?
                .into_composition_source()
                .map_err(|error| format!("Prefab structure {relative}: {error}"))?;
        } else {
            SceneDocumentCandidate::decode_json_bytes_with_limits(&written, limits)
                .map_err(|error| format!("default Scene preflight {relative}: {error}"))?
                .into_composition_source()
                .map_err(|error| format!("Scene structure {relative}: {error}"))?;
        }
        files.insert(relative.into(), file_report(relative, &written));
        Ok(())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!(
                "runtime_workload: source cleanup failed; retaining {}: {error}",
                self.root.display(),
            );
        }
    }
}

fn tilemap_record(
    side: i32,
    tile_size: f32,
    columns: Range<i32>,
    tileset: &str,
) -> Result<SceneComponentRecord, String> {
    let number = |value| ComponentValue::f64(value).map_err(|error| error.to_string());
    let color = ComponentValue::map([
        ("r", number(1.0)?),
        ("g", number(1.0)?),
        ("b", number(1.0)?),
        ("a", number(1.0)?),
    ]);
    let mut cells = Vec::with_capacity((columns.end - columns.start) as usize * side as usize);
    for y in -side / 2..side / 2 {
        for x in columns.clone() {
            cells.push(ComponentValue::map([
                (
                    "coord",
                    ComponentValue::map([
                        ("x", ComponentValue::I64(i64::from(x))),
                        ("y", ComponentValue::I64(i64::from(y))),
                    ]),
                ),
                (
                    "cell",
                    ComponentValue::map([
                        ("tile", ComponentValue::U64(0)),
                        ("color", color.clone()),
                    ]),
                ),
            ]));
        }
    }
    Ok(SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "tile_size",
                ComponentValue::map([
                    ("x", number(f64::from(tile_size))?),
                    ("y", number(f64::from(tile_size))?),
                ]),
            ),
            ("layer", ComponentValue::I64(-10)),
            (
                "tileset",
                encode_asset_reference_value(
                    &AssetRef::path(tileset).map_err(|error| error.to_string())?,
                ),
            ),
            ("cells", ComponentValue::List(cells)),
        ]),
    ))
}

fn entity_components<'a>(
    scene: &'a mut Value,
    id: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    scene
        .pointer_mut("/payload/entities")
        .and_then(Value::as_array_mut)
        .and_then(|entities| entities.iter_mut().find(|entity| entity["id"] == id))
        .and_then(|entity| entity.get_mut("components"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("copied Scene lacks components for {id}"))
}

fn scene_universe(root: &Path) -> Result<nara::core::SourceUniverseId, String> {
    let capability =
        two_level_game::open_explicit_project_root(root).map_err(|error| error.to_string())?;
    let manifest = capability
        .open_file(&nara::fs::RelativePath::new("nara.toml").map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let candidate = nara::project_host::ingest_project_manifest(&manifest, None)
        .map_err(|error| error.to_string())?;
    Ok(candidate.settings().project.source_universe)
}

#[derive(Default)]
struct CopyBudget {
    entries: usize,
    bytes: usize,
}

fn copy_source(
    source_root: &Path,
    source: &Path,
    destination: &Path,
    depth: usize,
    budget: &mut CopyBudget,
    files: &mut BTreeMap<String, Value>,
) -> Result<(), String> {
    budget.entries += 1;
    if depth > MAX_COPY_DEPTH || budget.entries > MAX_COPY_ENTRIES {
        return Err("copied source exceeds the fixture's entry/depth bounds".into());
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    let canonical = source.canonicalize().map_err(|error| error.to_string())?;
    if is_link(&metadata) || !canonical.starts_with(source_root) {
        return Err(format!("refusing linked source {}", source.display()));
    }
    if metadata.is_dir() {
        fs::create_dir(destination).map_err(|error| error.to_string())?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            if entries.len() >= MAX_COPY_ENTRIES {
                return Err("copied source directory exceeds the fixture's entry bound".into());
            }
            entries.push(entry.map_err(|error| error.to_string())?.path());
        }
        entries.sort();
        for entry in entries {
            let name = entry.file_name().ok_or("source entry lacks a file name")?;
            copy_source(
                source_root,
                &entry,
                &destination.join(name),
                depth + 1,
                budget,
                files,
            )?;
        }
    } else if metadata.is_file() {
        let maximum =
            (MAX_COPY_BYTES - budget.bytes).min(SceneFileLimits::default().encoded_bytes().get());
        let bytes = read_regular(source, maximum)?;
        write_file(destination, &bytes, false)?;
        budget.bytes += bytes.len();
        let relative = source
            .strip_prefix(source_root)
            .map_err(|error| error.to_string())?
            .to_str()
            .ok_or("source file name is not UTF-8")?
            .replace('\\', "/");
        files.insert(relative.clone(), file_report(&relative, &bytes));
    } else {
        return Err(format!("refusing nonregular source {}", source.display()));
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect directory {}: {error}", path.display()))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(format!(
            "refusing linked or non-directory path {}",
            path.display()
        ));
    }
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn read_regular(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || is_link(&metadata) || metadata.len() > maximum as u64 {
        return Err(format!(
            "refusing linked, nonregular or oversized file {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum {
        return Err(format!("file grew beyond copy bound: {}", path.display()));
    }
    Ok(bytes)
}

fn write_file(path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    if replace {
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || is_link(&metadata) {
            return Err(format!(
                "refusing to replace nonregular file {}",
                path.display()
            ));
        }
    }
    OpenOptions::new()
        .write(true)
        .create_new(!replace)
        .truncate(replace)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| format!("write copied source {}: {error}", path.display()))
}

fn file_report(relative: &str, bytes: &[u8]) -> Value {
    json!({
        "path": relative,
        "bytes": bytes.len(),
        "content_digest": format!("{:?}", ContentDigest::of_bytes(bytes)),
    })
}

#[cfg(test)]
#[path = "project/tests.rs"]
mod tests;
