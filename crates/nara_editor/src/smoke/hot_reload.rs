use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

use anyhow::{Context as _, Result, ensure};
use nara_asset::{
    AssetScanMode, AssetSourcePath, ProjectAssetDatabase,
    image::{ImageAsset, ImageColorSpace, ImageContentHash},
};
use nara_core::{
    AssetId, EntityUuid, Vec2,
    vfs::{AssetPath, AssetSource as _, DirAssetSource, write_atomic},
};
use nara_project::{ProjectManifest, workflow::SCHEMA_CACHE_PATH};
use nara_scene::{authoring::SpriteSource, spatial::Transform2d};
use open_gpui::{App, AsyncApp};

use super::{NATIVE, OpenEditor, recent_console, wait};
use crate::workspace::Workspace;

const TEXTURE: &str = "power-cell.png";
const MOVED_TEXTURE: &str = "hot-reload-probe/power-cell.png";
const SCENE: &str = "night-relay.scene.ron";
const PREFAB: &str = "power-cell.prefab.ron";
const MATERIAL: &str = "gate.material.ron";
const SHADER: &str = "gate.wgsl";
const ALTERNATE_TEXTURE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0xf3, 0xff,
    0x61, 0x00, 0x00, 0x00, 0x1a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x90, 0x7b, 0xf8, 0xe0,
    0x3f, 0x25, 0x98, 0x61, 0xd4, 0x80, 0x51, 0x03, 0x46, 0x0d, 0x18, 0x2e, 0x06, 0x00, 0x00, 0xfa,
    0x4f, 0xde, 0x1f, 0xea, 0x32, 0x64, 0xe4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

pub(crate) fn copy_fixture(root: &Path) -> Result<tempfile::TempDir> {
    let source = DirAssetSource::new(root)?;
    let copy = tempfile::tempdir()?;
    let manifest_source = AssetPath::new("nara.toml")?;
    let manifest = String::from_utf8(source.read_bytes(&manifest_source)?)?;
    let asset_root = AssetSourcePath::new(ProjectManifest::parse_toml(&manifest)?.paths.assets)?;
    let index = ProjectAssetDatabase::scan(source.root(), &[asset_root], AssetScanMode::ReadOnly)?;
    fs::write(copy.path().join("nara.toml"), manifest)?;
    for optional in [".nara.ron", SCHEMA_CACHE_PATH] {
        let path = AssetPath::new(optional)?;
        if source.exists(&path) {
            copy_file(&source, copy.path(), &path)?;
        }
    }
    for record in index.records() {
        copy_file(
            &source,
            copy.path(),
            &AssetPath::new(record.path().as_str())?,
        )?;
        copy_file(
            &source,
            copy.path(),
            &AssetPath::new(format!("{}.meta", record.path()))?,
        )?;
    }
    Ok(copy)
}

fn copy_file(source: &DirAssetSource, root: &Path, path: &AssetPath) -> Result<()> {
    let target = root.join(path.as_str());
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    std::io::copy(&mut source.open(path)?, &mut fs::File::create(target)?)?;
    Ok(())
}

pub(crate) fn start(
    editor: OpenEditor,
    count: u32,
    failure: Rc<RefCell<Option<anyhow::Error>>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        match measure(&editor, count, cx).await {
            Ok(()) => println!("HOT_RELOAD_MEASUREMENT_OK"),
            Err(error) => {
                eprintln!("Hot reload measurement failed: {error:#}");
                *failure.borrow_mut() = Some(error);
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

struct Edit {
    case: &'static str,
    path: PathBuf,
    variants: [(Vec<u8>, Expected); 2],
}

enum Expected {
    Texture(AssetId, ImageContentHash),
    Scene(EntityUuid, Vec2),
    Prefab(EntityUuid, Vec2),
    Material(AssetId, f32),
    Shader(AssetId, String),
}

impl Expected {
    fn applied(&self, workspace: &Workspace) -> bool {
        let row = |id| {
            workspace
                .session
                .preview_scene()
                .entities
                .iter()
                .find(|row| row.id == id)
        };
        match self {
            Self::Texture(id, hash) => workspace.preview_assets().image_hash(*id) == Some(*hash),
            Self::Scene(id, translation) => row(*id)
                .and_then(|row| row.typed::<Transform2d>().ok().flatten())
                .is_some_and(|transform| &transform.translation == translation),
            Self::Prefab(id, size) => row(*id)
                .and_then(|row| row.typed::<SpriteSource>().ok().flatten())
                .is_some_and(|sprite| &sprite.size == size),
            Self::Material(id, first_channel) => workspace
                .preview_assets()
                .material(*id)
                .is_some_and(|material| {
                    material
                        .uniform_bytes()
                        .starts_with(&first_channel.to_le_bytes())
                }),
            Self::Shader(id, source) => workspace
                .preview_assets()
                .material(*id)
                .is_some_and(|material| material.generation().shader().user_source() == source),
        }
    }
}

async fn measure(editor: &OpenEditor, count: u32, cx: &mut AsyncApp) -> Result<()> {
    let (root, texture, material) = cx.update(|cx| -> Result<_> {
        let workspace = editor.workspace.read(cx);
        let id = |name| -> Result<AssetId> {
            workspace
                .project
                .asset_reference(&AssetPath::new(name)?)
                .map(|asset| asset.id)
                .with_context(|| format!("No sidecar for '{name}'"))
        };
        Ok((
            workspace.project.asset_directory().to_path_buf(),
            id(TEXTURE)?,
            id(MATERIAL)?,
        ))
    })?;
    wait(cx, NATIVE, "present the initial hot reload preview", |cx| {
        let workspace = editor.workspace.read(cx);
        let (revision, _) = editor.viewport.read(cx).presented_preview()?;
        (workspace.preview_revision() == revision
            && workspace.preview_assets().image_hash(texture).is_some()
            && workspace.preview_assets().material(material).is_some())
        .then_some(())
    })
    .await
    .with_context(|| recent_console(editor, cx))?;

    let moved = AssetPath::new(MOVED_TEXTURE)?;
    let fixtures = cx
        .background_executor()
        .spawn(async move {
            let edits = prepare_edits(&root, texture, material)?;
            fs::create_dir(root.join("hot-reload-probe"))?;
            for suffix in ["", ".meta"] {
                fs::rename(
                    root.join(format!("{TEXTURE}{suffix}")),
                    root.join(format!("{MOVED_TEXTURE}{suffix}")),
                )?;
            }
            Ok::<_, anyhow::Error>(edits)
        })
        .await?;
    wait(
        cx,
        NATIVE,
        "resolve the image at its moved sidecar ID",
        |cx| {
            (editor
                .workspace
                .read(cx)
                .project
                .asset_hint(texture)
                .as_ref()
                == Some(&moved))
            .then_some(())
        },
    )
    .await
    .with_context(|| recent_console(editor, cx))?;
    println!("hot-reload rename asset_id={texture} path={moved}");

    for Edit {
        case,
        path,
        variants,
    } in fixtures
    {
        let mut samples = Vec::with_capacity(count as usize);
        for index in 0..count + 2 {
            let (source, expected) = &variants[index as usize % variants.len()];
            let baseline = cx.update(|cx| editor.workspace.read(cx).preview_revision());
            let path = path.clone();
            let bytes = source.clone();
            let started = cx
                .background_executor()
                .spawn(async move {
                    let started = Instant::now();
                    write_atomic(&path, &bytes)?;
                    Ok::<_, std::io::Error>(started)
                })
                .await?;
            let presented = wait(cx, NATIVE, case, |cx| {
                let workspace = editor.workspace.read(cx);
                let (revision, presented) = editor.viewport.read(cx).presented_preview()?;
                (revision != baseline
                    && revision == workspace.preview_revision()
                    && presented >= started
                    && expected.applied(workspace))
                .then_some(presented)
            })
            .await
            .with_context(|| recent_console(editor, cx))?;
            if index >= 2 {
                samples.push(presented.duration_since(started));
            }
        }
        samples.sort_unstable();
        let percentile = |percent: u32| {
            let rank = (count * percent).div_ceil(100) as usize;
            samples[rank - 1].as_secs_f64() * 1_000.0
        };
        println!(
            "hot-reload {case} samples={count} p50_ms={:.3} p95_ms={:.3}",
            percentile(50_u32),
            percentile(95_u32)
        );
    }
    Ok(())
}

fn prepare_edits(root: &Path, texture: AssetId, material: AssetId) -> Result<[Edit; 5]> {
    let image_hash = |source| {
        Ok::<_, anyhow::Error>(ImageAsset::from_png(source, ImageColorSpace::Srgb)?.content_hash())
    };
    let original = fs::read(root.join(TEXTURE))?;
    let textures = [
        (
            ALTERNATE_TEXTURE.to_vec(),
            Expected::Texture(texture, image_hash(ALTERNATE_TEXTURE)?),
        ),
        (
            original.clone(),
            Expected::Texture(texture, image_hash(&original)?),
        ),
    ];
    let pilot = "4f99b200-2a5e-4872-8d3d-000000000001".parse()?;
    let cell = "4f99b200-2a5e-4872-8d3d-000000000002".parse()?;
    let scene = fs::read_to_string(root.join(SCENE))?;
    let prefab = fs::read_to_string(root.join(PREFAB))?;
    let authored_material = fs::read_to_string(root.join(MATERIAL))?;
    let shader = fs::read_to_string(root.join(SHADER))?;
    let scenes = [
        (
            replace_one(&scene, "(-256.0, 96.0)", "(-232.0, 96.0)")?,
            Expected::Scene(pilot, Vec2::new(-232.0, 96.0)),
        ),
        (
            scene.into_bytes(),
            Expected::Scene(pilot, Vec2::new(-256.0, 96.0)),
        ),
    ];
    let prefabs = [
        (
            replace_one(&prefab, "size: (15.0, 15.0)", "size: (21.0, 21.0)")?,
            Expected::Prefab(cell, Vec2::splat(21.0)),
        ),
        (
            prefab.into_bytes(),
            Expected::Prefab(cell, Vec2::splat(15.0)),
        ),
    ];
    let materials = [
        (
            replace_one(
                &authored_material,
                "SpriteMaterial(",
                "SpriteMaterial(color: (0.4, 1.0, 1.0, 1.0),",
            )?,
            Expected::Material(material, 0.4),
        ),
        (
            authored_material.into_bytes(),
            Expected::Material(material, 1.0),
        ),
    ];
    let changed_shader = String::from_utf8(replace_one(
        &shader,
        "sampled * tint * material_parameters.color",
        "sampled * tint * material_parameters.color * vec4<f32>(1.0, 0.4, 1.0, 1.0)",
    )?)?;
    let shaders = [
        (
            changed_shader.as_bytes().to_vec(),
            Expected::Shader(material, changed_shader),
        ),
        (
            shader.as_bytes().to_vec(),
            Expected::Shader(material, shader),
        ),
    ];
    Ok([
        Edit {
            case: "texture",
            path: root.join(MOVED_TEXTURE),
            variants: textures,
        },
        Edit {
            case: "scene",
            path: root.join(SCENE),
            variants: scenes,
        },
        Edit {
            case: "prefab",
            path: root.join(PREFAB),
            variants: prefabs,
        },
        Edit {
            case: "material",
            path: root.join(MATERIAL),
            variants: materials,
        },
        Edit {
            case: "shader",
            path: root.join(SHADER),
            variants: shaders,
        },
    ])
}

fn replace_one(source: &str, old: &str, new: &str) -> Result<Vec<u8>> {
    ensure!(
        source.matches(old).count() == 1,
        "Night Relay fixture mismatch for '{old}'"
    );
    Ok(source.replacen(old, new, 1).into_bytes())
}
