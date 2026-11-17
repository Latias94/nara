//! Warm, CPU-only public Scene measurements with explicit phase and ownership boundaries.

use std::{fs, hint::black_box, path::Path, time::Instant};

use nara_core::{ByteLimit, SourceUniverseId};
use nara_ecs::{Component, World};
use nara_identity::SceneEntityId;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryBuilder, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueRef,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, SceneComponentBindings, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityRecord, SceneFileLimits, SceneFormatError,
    SceneOccurrenceKey, SourceDocumentKey, SourceDocumentKind,
};
use serde_json::json;

const KIND: &str = "comparison.TileRows";
const WARMUP: usize = 3;
const SAMPLES: usize = 9;

#[derive(Component)]
struct TileRows(Vec<TileRow>);

#[derive(Debug, PartialEq, Eq)]
struct TileRow {
    x: i32,
    y: i32,
    tile: u32,
    flags: u32,
}

impl TileRow {
    fn fields(&self) -> [i64; 4] {
        [
            i64::from(self.x),
            i64::from(self.y),
            i64::from(self.tile),
            i64::from(self.flags),
        ]
    }
}

fn decode_rows(value: ComponentValueRef<'_>) -> Result<TileRows, ComponentCodecError> {
    let rows = value
        .field("rows")?
        .as_list()
        .ok_or_else(|| ComponentCodecError::invalid_field("rows", "list"))?;
    let rows = rows
        .iter()
        .map(|row| {
            let mut result = [0; 4];
            for (index, key) in ["x", "y", "tile", "flags"].into_iter().enumerate() {
                let value = row
                    .field(key)?
                    .as_i64()
                    .ok_or_else(|| ComponentCodecError::invalid_field(key, "signed integer"))?;
                if (index < 2 && i32::try_from(value).is_err())
                    || (index >= 2 && u32::try_from(value).is_err())
                {
                    return Err(ComponentCodecError::invalid_field(key, "32-bit integer"));
                }
                result[index] = value;
            }
            Ok(TileRow {
                x: result[0] as i32,
                y: result[1] as i32,
                tile: result[2] as u32,
                flags: result[3] as u32,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(TileRows(rows))
}

fn encode_rows(rows: &TileRows) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([(
        "rows",
        ComponentValue::List(
            rows.0
                .iter()
                .map(|row| {
                    ComponentValue::map(
                        ["x", "y", "tile", "flags"]
                            .into_iter()
                            .zip(row.fields().into_iter().map(ComponentValue::I64)),
                    )
                })
                .collect(),
        ),
    )]))
}

fn bindings() -> SceneComponentBindings {
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec(
            ComponentSchema::new(
                ComponentTypeId::new(KIND),
                "Tile rows",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("rows"),
                "Rows",
                ComponentFieldPath::from_fields(["rows"]),
                ComponentValueKind::List,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            decode_rows,
            encode_rows,
        )
        .unwrap();
    SceneComponentBindings::new(registry.freeze().unwrap(), []).unwrap()
}

fn run(directory: &Path, population: usize, encoding: &str) {
    let ron = encoding == "ron";
    let side = (population as f64).sqrt() as usize;
    assert_eq!(side * side, population);
    let original = TileRows(
        (0..population)
            .map(|i| TileRow {
                x: (i % side) as i32,
                y: (i / side) as i32,
                tile: (i % 64) as u32,
                flags: (i % 4) as u32,
            })
            .collect(),
    );
    assert_eq!(std::mem::size_of::<TileRow>(), 16);
    let expected_checksum: i64 = original.0.iter().flat_map(TileRow::fields).sum();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("terrain").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(KIND),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, encode_rows(&original).unwrap()),
    )]);
    let extension = encoding;
    let path = directory.join(format!("nara-{population}.{extension}"));
    let limits = if ron && population == 16384 {
        assert!(matches!(
            document.to_ron_string(),
            Err(SceneFormatError::EncodedBytesExceeded {
                maximum: 16_777_216,
                ..
            })
        ));
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(32 * 1024 * 1024).unwrap())
    } else {
        SceneFileLimits::default()
    };
    let text = if ron {
        document.to_ron_string_with_limits(limits).unwrap()
    } else if encoding == "json_compact" {
        let pretty = document.to_json_string().unwrap();
        let value: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        serde_json::to_string(&value).unwrap()
    } else {
        document.to_json_string().unwrap()
    };
    fs::write(&path, text.as_bytes()).unwrap();
    let bindings = bindings();
    let universe: SourceUniverseId = "d930fd8c-bbbe-4db7-8e9e-4f143c75fed1".parse().unwrap();
    let source =
        SourceDocumentKey::named(universe, "comparison", SourceDocumentKind::Scene).unwrap();
    let occurrence = SceneOccurrenceKey::new("comparison", "terrain").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new();
    let mut reads = Vec::new();
    let mut decodes = Vec::new();
    let mut compiles = Vec::new();
    let mut instantiates = Vec::new();
    let mut native_counts = None;
    for sample in 0..WARMUP + SAMPLES {
        let start = Instant::now();
        let bytes = fs::read(&path).unwrap();
        let read_ns = start.elapsed().as_nanos();
        black_box(&bytes);
        let start = Instant::now();
        let candidate = if ron {
            SceneDocumentCandidate::decode_ron_bytes_with_limits(&bytes, limits)
        } else {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        }
        .unwrap();
        let decoded = candidate.into_composition_source().unwrap();
        let decode_ns = start.elapsed().as_nanos();
        black_box(&decoded);
        let start = Instant::now();
        let report = bindings.compile_scene(&decoded, &resolver);
        let compiled = report.candidate.unwrap().bind_source(source.clone());
        let compile_ns = start.elapsed().as_nanos();
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        black_box(&compiled);
        let start = Instant::now();
        let mut world = World::new();
        let pending = bindings
            .prepare_scene(&mut world, &compiled, &occurrence, None)
            .unwrap();
        let spawned = pending.commit(&mut world);
        let instantiate_ns = start.elapsed().as_nanos();
        assert!(!spawned.diagnostics.has_errors(), "{spawned:?}");
        assert!(spawned.instance.is_some());
        let rows = world.query::<&TileRows>().single(&world).unwrap();
        assert_eq!(rows.0, original.0);
        assert_eq!(
            rows.0.iter().flat_map(TileRow::fields).sum::<i64>(),
            expected_checksum
        );
        let counts = world.archetypes().iter().fold((0, 0), |counts, archetype| {
            let entities = archetype.len() as usize;
            (
                counts.0 + entities,
                counts.1 + entities * archetype.components().len(),
            )
        });
        if let Some(previous) = native_counts {
            assert_eq!(counts, previous);
        }
        native_counts = Some(counts);
        if sample >= WARMUP {
            reads.push(read_ns);
            decodes.push(decode_ns);
            compiles.push(compile_ns);
            instantiates.push(instantiate_ns);
        }
        black_box(&world);
    }
    println!(
        "{}",
        json!({
            "engine":"nara", "format": extension, "mode":"generic_rows_one_entity",
            "revision":"9bd3a54f016992c10cb42d105ea338f4e35e64b0", "rows":population,
            "entities":1, "components":1, "source_bytes":text.len(), "source_file":path,
            "runtime_world_entities":native_counts.unwrap().0, "runtime_world_components":native_counts.unwrap().1,
            "logical_payload_bytes":population * 16, "checksum":expected_checksum,
            "encoded_limit_bytes":limits.encoded_bytes().get(),
            "default_encoded_limit_bytes":SceneFileLimits::default().encoded_bytes().get(),
            "warmup":WARMUP, "samples":SAMPLES,
            "warm_read_ns":reads, "decode_ns":decodes,
            "compile_ns":compiles, "instantiate_ns":instantiates,
            "decode_scope":"structural preflight + strict envelope + typed generic payload + source finish",
            "writer_scope":if encoding == "json_compact" { "public JSON wire, externally minified outside timing; not the default writer" } else { "unchanged public pretty writer" },
            "instantiate_scope":"fresh World + native preparation + original commit; no rendering/assets/Prefab",
            "state_verified":true
        })
    );
}

fn main() {
    let directory = std::env::args_os()
        .nth(1)
        .expect("supply an existing owned output directory");
    let directory = Path::new(&directory);
    assert!(directory.is_dir());
    for population in [64, 4096, 16384] {
        for encoding in ["json", "json_compact", "ron"] {
            run(directory, population, encoding);
        }
    }
}
