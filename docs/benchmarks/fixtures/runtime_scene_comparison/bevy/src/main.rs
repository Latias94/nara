//! CPU-only measurements against Bevy 0.20.0-dev at the pinned local source revision.
//!
//! The new `Scene` / `ScenePatch` / `ResolvedScene` / `Template` APIs do not provide
//! a native RON serializer or decoder at this revision. See `bevy_scene/src/lib.rs`,
//! section ".bsn Asset Format", and `bevy_scene/src/scene_patch.rs`. Consequently,
//! this consumer reports native Scene file phases as unavailable. It measures real
//! Scene resolution and instantiation from complete, typed Rust patches instead.
//! `Clone` supplies the ordinary `Template` implementation for these components.
//!
//! The optional `native-world-ron` feature additionally measures the *separate*
//! official `.scn.ron` workflow: `DynamicWorld::serialize`, `WorldDeserializer`, and
//! `DynamicWorld::write_to_world_with`. These APIs live in
//! `bevy_world_serialization`; this is neither an obsolete `DynamicScene` consumer
//! nor a decoded-world-to-Scene adapter. The native World format has no Scene
//! template resolution phase. `WorldMapVisitor::visit_map` already performs
//! `ReflectFromReflect` conversion inside decode; write-to-world retains its normal
//! reflection, entity mapping, component registration, and insertion work.
//!
//! There are no plugins, schedules, renderer, asset dependencies, or worker pools.
//! An inert in-memory AssetServer supplies the context required by the Scene APIs.
//! Each phase has three warmups and nine retained samples. Input construction,
//! dependency enumeration, registry setup, validation, and full World destruction
//! are outside timing. Instantiation includes `World::new` and the actual public
//! spawn/write operation. Scene spawning includes Bevy's native `Ready` triggers.
//! File reads are warm OS-cache reads; no cold-storage claim is made.
//!
//! Run an optimized build, optionally passing an existing owned directory for
//! generated native World RON files. Stdout is newline-delimited JSON: one metadata
//! record followed by measurements. Source files are retained for inspection.
//! The default build avoids the camera/image/mesh dependencies that the pinned
//! `bevy_world_serialization` crate unconditionally requires. No reference manifest
//! or lockfile needs modification; this package has its own workspace and lockfile.

use std::{
    hint::black_box,
    io::{self, BufWriter, Write},
    path::Path,
    process::Command,
    sync::Arc,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use bevy_asset::{
    AssetServer, AssetServerMode, Assets, UnapprovedPathMode,
    io::{AssetSourceBuilder, AssetSourceBuilders, AssetSourceId, memory::MemoryAssetReader},
};
use bevy_ecs::{component::Component, reflect::ReflectComponent, world::World};
use bevy_reflect::Reflect;
use bevy_scene::{PatchFromTemplate, ScenePatch};
use serde_json::{Value, json};

#[cfg(feature = "native-world-ron")]
use {
    bevy_ecs::entity::{Entity, EntityHashMap},
    bevy_reflect::TypeRegistry,
    bevy_world_serialization::{DynamicWorld, DynamicWorldBuilder, serde::WorldDeserializer},
    serde::de::DeserializeSeed,
    std::fs,
};

const BEVY_VERSION: &str = "0.20.0-dev";
const BEVY_REVISION: &str = "03ed5a27c92715c8fa2188f31f04e454ec39942f";
const WARMUP: usize = 3;
const SAMPLES: usize = 9;
const POPULATIONS: [usize; 3] = [64, 4096, 16384];

#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TileRow {
    x: i32,
    y: i32,
    tile: u32,
    flags: u32,
}

#[derive(Component, Reflect, Clone, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
struct TileRows(Vec<TileRow>);

fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let start = Instant::now();
    let output = black_box(operation());
    let elapsed = start.elapsed().as_nanos();
    (output, elapsed)
}

fn rows(population: usize) -> Vec<TileRow> {
    let side = population.isqrt();
    assert_eq!(side * side, population);
    (0..population)
        .map(|index| TileRow {
            x: i32::try_from(index % side).expect("bounded tile coordinate"),
            y: i32::try_from(index / side).expect("bounded tile coordinate"),
            tile: (index % 64) as u32,
            flags: (index % 4) as u32,
        })
        .collect()
}

fn checksum(rows: &[TileRow]) -> i64 {
    rows.iter()
        .map(|row| i64::from(row.x) + i64::from(row.y) + i64::from(row.tile) + i64::from(row.flags))
        .sum()
}

fn world_counts(world: &World) -> (usize, usize) {
    world.archetypes().iter().fold((0, 0), |counts, archetype| {
        let entities = archetype.len() as usize;
        (
            counts.0 + entities,
            counts.1 + entities * archetype.components().len(),
        )
    })
}

fn validate(world: &World, expected: &[TileRow], baseline: (usize, usize)) -> Result<()> {
    let mut components = 0;
    for entity in world.iter_entities() {
        if let Some(rows) = entity.get::<TileRows>() {
            ensure!(
                entity.archetype().components().len() == 1,
                "unexpected payload components"
            );
            ensure!(rows.0 == expected, "exact row values or count differ");
            ensure!(
                checksum(&rows.0) == checksum(expected),
                "row checksum differs"
            );
            components += 1;
            black_box(&rows.0);
        }
    }
    ensure!(components == 1, "expected exactly one TileRows component");
    ensure!(
        world_counts(world) == (baseline.0 + 1, baseline.1 + 1),
        "unexpected World entity/component count"
    );
    Ok(())
}

fn emit(output: &mut impl Write, record: Value) -> Result<()> {
    serde_json::to_writer(&mut *output, &record)?;
    writeln!(output)?;
    Ok(())
}

fn measure_scene(
    output: &mut impl Write,
    expected: &[TileRow],
    server: &AssetServer,
    cache: &Assets<ScenePatch>,
    baseline: (usize, usize),
) -> Result<()> {
    let mut resolves = Vec::with_capacity(SAMPLES);
    let mut instantiates = Vec::with_capacity(SAMPLES);
    for sample in 0..WARMUP + SAMPLES {
        let rows = TileRows(expected.to_vec());
        let mut scene =
            ScenePatch::load(server, TileRows::patch(move |template, _| *template = rows));
        let (resolved, resolve_ns) = timed(|| scene.resolve(black_box(server), black_box(cache)));
        resolved.context("resolve complete typed Scene templates")?;
        let (spawned, instantiate_ns) = timed(|| instantiate_scene(black_box(&scene)));
        let world = spawned.context("instantiate typed Scene in fresh World")?;
        validate(&world, expected, baseline)?;
        if sample >= WARMUP {
            resolves.push(resolve_ns);
            instantiates.push(instantiate_ns);
        }
        black_box((&world, &scene));
    }
    emit(
        output,
        json!({
            "engine": "bevy", "version": BEVY_VERSION, "revision": BEVY_REVISION,
            "format": "typed_rust_scene", "mode": "rows_one_entity", "rows": expected.len(),
            "entities": 1, "components": 1, "source_bytes": null,
            "logical_payload_bytes": expected.len() * 16, "checksum": checksum(expected),
            "world_entities": baseline.0 + 1, "world_components": baseline.1 + 1,
            "warmup": WARMUP, "samples": SAMPLES,
            "warm_read_ns": null, "decode_ns": null, "compile_ns": resolves,
            "instantiate_ns": instantiates, "state_verified": true,
            "resolve_api": "ScenePatch::resolve -> ResolvedSceneRoot",
            "compile_scope": "complete typed Template patches; Rust input assembly and dependency enumeration excluded",
            "instantiate_scope": "World::new + ScenePatch::spawn; Template::build_template, clone, component registration/insertion, Ready triggers included; World destruction excluded",
            "file_pipeline_status": "unavailable: this Scene API has no native file serializer/decoder at the pinned revision"
        }),
    )
}

fn instantiate_scene(scene: &ScenePatch) -> Result<World> {
    let mut world = World::new();
    black_box(scene.spawn(black_box(&mut world))?.id());
    Ok(world)
}

#[cfg(feature = "native-world-ron")]
fn decode_native_world(
    bytes: &[u8],
    registry: &TypeRegistry,
    server: &AssetServer,
) -> Result<DynamicWorld> {
    let mut deserializer = ron::de::Deserializer::from_bytes(black_box(bytes))?;
    let mut load_from_path = server;
    let decoded = WorldDeserializer {
        type_registry: registry,
        load_from_path: &mut load_from_path,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| deserializer.span_error(error))?;
    deserializer
        .end()
        .map_err(|error| deserializer.span_error(error))?;
    Ok(decoded)
}

#[cfg(feature = "native-world-ron")]
fn instantiate_native_world(
    source: &DynamicWorld,
    registry: &TypeRegistry,
) -> Result<(World, EntityHashMap<Entity>)> {
    let mut world = World::new();
    let mut entity_map = EntityHashMap::default();
    source.write_to_world_with(
        black_box(&mut world),
        black_box(&mut entity_map),
        black_box(registry),
    )?;
    Ok((world, entity_map))
}

#[cfg(feature = "native-world-ron")]
fn measure_native_world(
    output: &mut impl Write,
    directory: &Path,
    expected: &[TileRow],
    server: &AssetServer,
    registry: &TypeRegistry,
    baseline: (usize, usize),
) -> Result<()> {
    let mut authored = World::new();
    let entity = authored.spawn(TileRows(expected.to_vec())).id();
    validate(&authored, expected, baseline)?;
    let source = DynamicWorldBuilder::from_world(&authored, registry)
        .deny_all()
        .allow_component::<TileRows>()
        .extract_entity(entity)
        .build();
    let text = source.serialize(registry)?;
    let path = directory.join(format!("bevy-world-rows-{}.scn.ron", expected.len()));
    fs::write(&path, text.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    let mut serializes = Vec::with_capacity(SAMPLES);
    let mut reads = Vec::with_capacity(SAMPLES);
    let mut decodes = Vec::with_capacity(SAMPLES);
    let mut instantiates = Vec::with_capacity(SAMPLES);
    for sample in 0..WARMUP + SAMPLES {
        let (serialized, serialize_ns) =
            timed(|| black_box(&source).serialize(black_box(registry)));
        let serialized = serialized?;
        ensure!(serialized == text, "serialized native World bytes differ");
        let (bytes, read_ns) = timed(|| fs::read(black_box(&path)));
        let bytes = bytes?;
        ensure!(bytes == text.as_bytes(), "native World file bytes differ");
        let (decoded, decode_ns) = timed(|| decode_native_world(&bytes, registry, server));
        let decoded = decoded.context("decode native World RON")?;
        ensure!(
            decoded.resources.is_empty(),
            "unexpected serialized resources"
        );
        ensure!(decoded.entities.len() == 1, "native entity count differs");
        ensure!(
            decoded
                .entities
                .iter()
                .all(|entity| entity.components.len() == 1),
            "native component count differs"
        );
        let (spawned, instantiate_ns) =
            timed(|| instantiate_native_world(black_box(&decoded), registry));
        let (world, entity_map) = spawned.context("write native World into fresh World")?;
        ensure!(
            entity_map.len() == decoded.entities.len(),
            "entity map count differs"
        );
        validate(&world, expected, baseline)?;
        if sample >= WARMUP {
            serializes.push(serialize_ns);
            reads.push(read_ns);
            decodes.push(decode_ns);
            instantiates.push(instantiate_ns);
        }
        black_box((&world, &entity_map, &decoded, &bytes, &serialized));
    }
    emit(
        output,
        json!({
            "engine": "bevy", "version": BEVY_VERSION, "revision": BEVY_REVISION,
            "format": "native_world_scn_ron", "mode": "rows_one_entity", "rows": expected.len(),
            "entities": 1, "components": 1, "source_bytes": text.len(),
            "logical_payload_bytes": expected.len() * 16, "checksum": checksum(expected),
            "source_file": path, "world_entities": baseline.0 + 1,
            "world_components": baseline.1 + 1, "warmup": WARMUP, "samples": SAMPLES,
            "serialize_ns": serializes, "warm_read_ns": reads, "decode_ns": decodes,
            "compile_ns": null, "instantiate_ns": instantiates, "state_verified": true,
            "serialize_scope": "DynamicWorld::serialize; reflection extraction from authored World excluded",
            "decode_scope": "RON bytes -> WorldDeserializer -> DynamicWorld, including native ReflectFromReflect conversion and trailing-input validation; resident bytes, no Scene template resolution",
            "instantiate_scope": "World::new + empty EntityHashMap + DynamicWorld::write_to_world_with; registration, reflection, mapping and insertion included; World/map destruction excluded",
            "comparison_limit": "official serialized World workflow, separate from the typed Scene/ScenePatch/Template workflow; no native Scene RON claim"
        }),
    )
}

fn verify_revision(reference: &Path) -> Result<()> {
    let revision = Command::new("git")
        .arg("-C")
        .arg(reference)
        .args(["rev-parse", "HEAD"])
        .output()?;
    ensure!(revision.status.success(), "cannot read reference revision");
    ensure!(
        String::from_utf8(revision.stdout)?.trim() == BEVY_REVISION,
        "reference revision differs from benchmark pin"
    );
    let status = Command::new("git")
        .arg("-C")
        .arg(reference)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()?;
    ensure!(
        status.status.success() && status.stdout.is_empty(),
        "reference has tracked modifications or cannot be inspected"
    );
    Ok(())
}

fn main() -> Result<()> {
    ensure!(
        std::mem::size_of::<TileRow>() == 16,
        "expected a 16-byte native row"
    );
    ensure!(
        !cfg!(debug_assertions),
        "use an optimized --release build for measurements"
    );
    let reference = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../../repo-ref/bevy");
    verify_revision(&reference)?;
    let directory = std::env::args_os().nth(1).unwrap_or_else(|| ".".into());
    let directory = Path::new(&directory);
    ensure!(
        directory.is_dir(),
        "supply an existing owned output directory"
    );
    let mut sources = AssetSourceBuilders::default();
    sources.insert(
        AssetSourceId::Default,
        AssetSourceBuilder::new(|| Box::new(MemoryAssetReader::default())),
    );
    let server = AssetServer::new(
        Arc::new(sources.build_sources(false, false)),
        AssetServerMode::Unprocessed,
        false,
        UnapprovedPathMode::Forbid,
    );
    let cache = Assets::<ScenePatch>::default();
    let baseline = world_counts(&World::new());
    let mut output = BufWriter::new(io::stdout().lock());
    emit(
        &mut output,
        json!({
            "kind": "metadata", "engine": "bevy", "version": BEVY_VERSION,
            "revision": BEVY_REVISION, "revision_verified": true, "rows": POPULATIONS,
            "warmup": WARMUP, "samples": SAMPLES, "clock": "std::time::Instant", "unit": "ns",
            "native_world_ron_enabled": cfg!(feature = "native-world-ron"),
            "baseline_world_entities": baseline.0, "baseline_world_components": baseline.1,
            "payload": "x:i32, y:i32, tile:u32, flags:u32; one reflected TileRows(Vec<TileRow>) component on one entity",
            "row_values": "side=isqrt(rows); x=i%side; y=i/side; tile=i%64; flags=i%4",
            "native_scene_file_pipeline": {
                "status": "unavailable", "source_bytes": null, "serialize_ns": null,
                "warm_read_ns": null, "decode_ns": null, "compile_ns": null,
                "reason": "pinned Scene/ScenePatch/ResolvedScene/Template APIs have no native RON encoder or decoder; .bsn asset support is not shipped",
                "source": "crates/bevy_scene/src/lib.rs: .bsn Asset Format; crates/bevy_scene/src/scene_patch.rs"
            },
            "phase_limits": [
                "typed Scene resolve and native World RON are independent workloads; their phase arrays must not be combined into a file-to-Scene pipeline",
                "instantiate includes fresh World construction; baseline counts include Bevy internal resource entities/components",
                "native World decode includes ReflectFromReflect materialization; it does not resolve Scene templates",
                "warm reads include open/read/allocation/close; decode consumes resident bytes; no cold-cache, asset-I/O scheduling or end-to-end latency claim",
                "three warmups then nine raw samples per phase and case; no percentile or frame-budget claim",
                "exact counts, every row field, and additive checksum verified outside every timed iteration; setup and full output destruction excluded",
                "no plugins, schedules, rendering, hierarchy, external asset loads, or hot reload"
            ],
            "source_owners": [
                "crates/bevy_scene/src/scene_patch.rs",
                "crates/bevy_scene/src/resolved_scene.rs",
                "crates/bevy_ecs/src/template.rs",
                "crates/bevy_world_serialization/src/dynamic_world.rs",
                "crates/bevy_world_serialization/src/serde.rs",
                "crates/bevy_world_serialization/src/world_asset_loader.rs"
            ]
        }),
    )?;
    #[cfg(feature = "native-world-ron")]
    let registry = {
        let mut registry = TypeRegistry::default();
        registry.register::<TileRow>();
        registry.register::<TileRows>();
        registry
    };
    for population in POPULATIONS {
        let expected = rows(population);
        measure_scene(&mut output, &expected, &server, &cache, baseline)?;
        #[cfg(feature = "native-world-ron")]
        measure_native_world(
            &mut output,
            directory,
            &expected,
            &server,
            &registry,
            baseline,
        )?;
    }
    output.flush()?;
    Ok(())
}
