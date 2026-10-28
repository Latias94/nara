use super::*;

fn entry(id: u8, package: &str, requirements: &[(u8, &str)]) -> Entry {
    Entry {
        installed: EditorSourceToolInstalledSelection {
            package: package.into(),
            directory: format!("tools/install-{id}").into(),
            installation: [id; 32],
            receipt: ContentDigest::from_parts(32, [2; 32]),
            manifest: ContentDigest::from_parts(64, [3; 32]),
        },
        requirements: requirements
            .iter()
            .map(|(id, package)| Binding {
                package: (*package).into(),
                installation: [*id; 32],
            })
            .collect(),
    }
}

fn graph() -> Graph {
    let installed = entry(1, "test.stats", &[]);
    Graph {
        desired: Some(Desired {
            installed: installed.installed.clone(),
            enabled: true,
        }),
        entries: vec![installed],
    }
}

fn record() -> String {
    String::from_utf8(encode(&graph()).unwrap()).unwrap()
}

fn required_graph() -> Graph {
    Graph::default()
        .register(&[
            entry(3, "test.consumer", &[(2, "test.other"), (1, "test.stats")]),
            entry(2, "test.other", &[]),
            entry(1, "test.stats", &[]),
        ])
        .unwrap()
}

#[test]
fn canonical_graph_records_roundtrip_with_enabled_disabled_and_absent_desired() {
    for enabled in [true, false] {
        let mut graph = graph();
        graph.desired.as_mut().unwrap().enabled = enabled;
        let expected = format!(
            "{{\"format_version\":1,\"installed\":[{{\"package\":\"test.stats\",\"directory\":\"tools/install-1\",\"installation\":{},\"receipt\":{{\"length\":32,\"hash\":{}}},\"manifest\":{{\"length\":64,\"hash\":{}}},\"requires\":[]}}],\"desired\":{{\"installation\":{},\"enabled\":{enabled}}}}}",
            serde_json::to_string(&[1_u8; 32]).unwrap(),
            serde_json::to_string(&[2_u8; 32]).unwrap(),
            serde_json::to_string(&[3_u8; 32]).unwrap(),
            serde_json::to_string(&[1_u8; 32]).unwrap(),
        );
        let bytes = encode(&graph).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded, graph);
        assert_eq!(encode(&decoded).unwrap(), bytes);
    }
    let empty = b"{\"format_version\":1,\"installed\":[],\"desired\":null}";
    assert_eq!(encode(&Graph::default()).unwrap(), empty);
    assert_eq!(decode(empty).unwrap(), Graph::default());
    assert_eq!(
        decode(b"{\"format_version\":1,\"installed\":[]}").unwrap(),
        Graph::default()
    );
    let graph = required_graph();
    assert!(graph.desired.is_none());
    assert_eq!(decode(&encode(&graph).unwrap()).unwrap(), graph);
}

#[test]
fn encode_and_decode_canonicalize_member_and_requirement_order_without_changing_the_input() {
    let canonical = required_graph();
    let mut shuffled = canonical.clone();
    shuffled.entries.reverse();
    shuffled.entries[0].requirements.reverse();
    let before = shuffled.clone();
    let bytes = encode(&canonical).unwrap();
    assert_eq!(encode(&shuffled).unwrap(), bytes);
    assert_eq!(shuffled, before);

    let input = String::from_utf8(bytes.clone()).unwrap();
    let mut dto: SelectionRecord = serde_json::from_str(&input).unwrap();
    dto.installed.reverse();
    dto.installed[0].requires.reverse();
    let shuffled_bytes = serde_json::to_vec(&dto).unwrap();
    assert_ne!(shuffled_bytes, bytes);
    let decoded = decode(&shuffled_bytes).unwrap();
    assert_eq!(decoded, canonical);
    assert_eq!(encode(&decoded).unwrap(), bytes);
    assert_eq!(decoded.register(&shuffled.entries).unwrap(), canonical);
}

#[test]
fn rejects_malformed_missing_unknown_and_previous_single_member_records() {
    let valid = record();
    for input in [
        String::new(),
        "null".to_owned(),
        "{}".to_owned(),
        "[]".to_owned(),
        valid[..valid.len() - 1].to_owned(),
        format!("{valid} {{}}"),
        valid.replace("\"format_version\":1,", ""),
        valid.replace("\"package\":\"test.stats\",", ""),
        valid.replace("\"directory\":\"tools/install-1\",", ""),
        valid.replace(",\"requires\":[]", ""),
        valid.replace("\"enabled\":true", "\"enabled\":null"),
        valid.replace("\"enabled\":true", "\"enabled\":1"),
        valid.replace("\"enabled\":true", "\"desired_enabled\":true"),
        valid.replace("\"length\":32,", ""),
        valid.replace("\"length\":32", "\"length\":-1"),
        valid.replace("\"length\":32", "\"length\":32.0"),
        valid.replacen('{', "{\"unknown\":true,", 1),
        valid.replace("\"installed\":[{", "\"installed\":[{\"unknown\":true,"),
        valid.replace("\"receipt\":{", "\"receipt\":{\"unknown\":true,"),
        valid.replace("\"manifest\":{", "\"manifest\":{\"unknown\":true,"),
        valid.replace("\"desired\":{", "\"desired\":{\"unknown\":true,"),
    ] {
        assert_eq!(
            decode(input.as_bytes()),
            Err(Error::InvalidRecord),
            "accepted {input}"
        );
    }
    let dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
    let member = serde_json::to_string(&dto.installed[0]).unwrap();
    let old = format!("{{\"format_version\":1,\"installed\":{member},\"desired_enabled\":true}}");
    assert_eq!(decode(old.as_bytes()), Err(Error::InvalidRecord));
    let required = String::from_utf8(encode(&required_graph()).unwrap()).unwrap();
    let unknown_binding = required.replace("\"requires\":[{", "\"requires\":[{\"unknown\":true,");
    assert_eq!(
        decode(unknown_binding.as_bytes()),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn rejects_duplicate_and_escaped_duplicate_fields_at_every_object_level() {
    let valid = record();
    let dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
    for field in [
        "format_version",
        "installed",
        "desired",
        "package",
        "directory",
        "installation",
        "receipt",
        "manifest",
        "length",
        "hash",
        "requires",
        "enabled",
    ] {
        let input = valid.replace(
            &format!("\"{field}\":"),
            &format!("\"{field}\":null,\"{field}\":"),
        );
        assert_ne!(input, valid);
        assert_eq!(
            decode(input.as_bytes()),
            Err(Error::InvalidRecord),
            "{field}"
        );
    }
    let duplicated_valid = valid.replace("\"enabled\":true", "\"enabled\":true,\"enabled\":true");
    assert_eq!(
        decode(duplicated_valid.as_bytes()),
        Err(Error::InvalidRecord)
    );
    let escaped = valid.replace(
        "\"package\":",
        "\"p\\u0061ckage\":\"test.stats\",\"package\":",
    );
    assert_eq!(decode(escaped.as_bytes()), Err(Error::InvalidRecord));
    let member = serde_json::to_string(&dto.installed[0]).unwrap();
    let mut dto = dto;
    dto.installed.push(serde_json::from_str(&member).unwrap());
    assert_eq!(
        decode(&serde_json::to_vec(&dto).unwrap()),
        Err(Error::Graph)
    );

    let required = String::from_utf8(encode(&required_graph()).unwrap()).unwrap();
    let mut dto: SelectionRecord = serde_json::from_str(&required).unwrap();
    dto.installed[2].requires.push(RequirementRecord {
        package: Cow::Borrowed("test.stats"),
        installation: [1; 32],
    });
    assert_eq!(
        decode(&serde_json::to_vec(&dto).unwrap()),
        Err(Error::Graph)
    );
}

#[test]
fn rejects_unsupported_versions_and_noninteger_versions() {
    let valid = record();
    for version in ["0", "2", "4294967295"] {
        let input = valid.replace(
            "\"format_version\":1",
            &format!("\"format_version\":{version}"),
        );
        assert_eq!(decode(input.as_bytes()), Err(Error::UnsupportedFormat));
    }
    for version in ["-1", "1.0", "\"1\"", "null", "4294967296"] {
        let input = valid.replace(
            "\"format_version\":1",
            &format!("\"format_version\":{version}"),
        );
        assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    }
}

#[test]
fn validates_package_nonzero_installation_and_complete_desired_facts() {
    for package in [
        "",
        "Stats",
        "test/stats",
        "test+stats",
        "test stats",
        "stаts",
    ] {
        let mut graph = graph();
        graph.desired = None;
        graph.entries[0].installed.package = package.into();
        assert_eq!(encode(&graph), Err(Error::InvalidRecord));
        assert_eq!(
            decode(record().replace("test.stats", package).as_bytes()),
            Err(Error::InvalidRecord)
        );
    }
    for (length, accepted) in [(1, true), (128, true), (129, false)] {
        let mut graph = graph();
        graph.desired = None;
        graph.entries[0].installed.package = "a".repeat(length).into();
        assert_eq!(encode(&graph).is_ok(), accepted);
        assert_eq!(
            decode(
                record()
                    .replace("test.stats", &"a".repeat(length))
                    .as_bytes()
            )
            .is_ok(),
            accepted
        );
    }
    let input = record().replace(
        &serde_json::to_string(&[1_u8; 32]).unwrap(),
        &serde_json::to_string(&[0_u8; 32]).unwrap(),
    );
    assert_eq!(decode(input.as_bytes()), Err(Error::InvalidRecord));
    let mut graph = graph();
    graph.desired.as_mut().unwrap().installed.manifest = ContentDigest::from_parts(64, [9; 32]);
    assert_eq!(encode(&graph), Err(Error::Graph));
    let valid = record();
    let mut dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
    dto.desired.as_mut().unwrap().installation = [9; 32];
    assert_eq!(
        decode(&serde_json::to_vec(&dto).unwrap()),
        Err(Error::Graph)
    );
}

#[test]
fn validates_fixed_width_byte_arrays_and_digest_lengths() {
    let valid = record();
    let arrays = [
        "[]".to_owned(),
        serde_json::to_string(&vec![1_u8; 31]).unwrap(),
        serde_json::to_string(&vec![1_u8; 33]).unwrap(),
        format!("[-1,{}]", vec!["1"; 31].join(",")),
        format!("[256,{}]", vec!["1"; 31].join(",")),
        format!("[1.0,{}]", vec!["1"; 31].join(",")),
        "\"hex\"".to_owned(),
        "null".to_owned(),
    ];
    for original in [[1_u8; 32], [2; 32], [3; 32]] {
        let original = serde_json::to_string(&original).unwrap();
        for replacement in &arrays {
            assert_eq!(
                decode(valid.replace(&original, replacement).as_bytes()),
                Err(Error::InvalidRecord)
            );
        }
    }
    for (field, original, limit) in [("receipt", 32, 4096), ("manifest", 64, 65_536)] {
        for length in [0, 1, limit, limit + 1, u64::MAX] {
            let mut graph = graph();
            graph.desired = None;
            let digest = ContentDigest::from_parts(length, [0; 32]);
            if field == "receipt" {
                graph.entries[0].installed.receipt = digest;
            } else {
                graph.entries[0].installed.manifest = digest;
            }
            let accepted = (1..=limit).contains(&length);
            assert_eq!(encode(&graph).is_ok(), accepted, "{field}: {length}");
            let input = valid.replace(
                &format!("\"length\":{original}"),
                &format!("\"length\":{length}"),
            );
            assert_eq!(
                decode(input.as_bytes()).is_ok(),
                accepted,
                "{field}: {length}"
            );
        }
    }
}

#[test]
fn validates_relative_paths_and_selection_parent_exclusion_before_publication() {
    for directory in [
        "",
        "/tools",
        "C:/tools",
        "tools\\stats",
        "tools//stats",
        "tools/",
        ".",
        "..",
        "./tools",
        "tools/./stats",
        "tools/../stats",
        "tools/stats.",
        "tools/stats ",
        "tools/NUL",
        "tools/con.txt",
        "tools/a:b",
        "tools/a?b",
        "tools/a\nb",
        ".nara",
        ".NARA",
        ".nara/source-tools",
        ".NARA/Source-Tools",
    ] {
        let mut graph = graph();
        graph.desired = None;
        graph.entries[0].installed.directory = directory.into();
        assert_eq!(encode(&graph), Err(Error::InvalidPath), "{directory}");
        let valid = record();
        let mut dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
        dto.installed[0].directory = Cow::Borrowed(directory);
        assert_eq!(
            decode(&serde_json::to_vec(&dto).unwrap()),
            Err(Error::InvalidPath),
            "{directory}"
        );
    }
    for (directory, accepted) in [
        (vec!["a"; 16].join("/"), true),
        (vec!["a"; 17].join("/"), false),
        (format!("{}/{}/c", "a".repeat(255), "b".repeat(254)), true),
        (format!("{}/{}/c", "a".repeat(255), "b".repeat(255)), false),
        ("a".repeat(256), false),
        ("tools/工具".to_owned(), true),
        (".nara/source-tools/install-1".to_owned(), true),
    ] {
        let mut graph = graph();
        graph.desired = None;
        graph.entries[0].installed.directory = directory.clone().into();
        assert_eq!(encode(&graph).is_ok(), accepted, "{directory}");
        let valid = record();
        let mut dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
        dto.installed[0].directory = Cow::Owned(directory);
        assert_eq!(decode(&serde_json::to_vec(&dto).unwrap()).is_ok(), accepted);
    }
}

#[test]
fn decode_rejects_directory_aliases_overlaps_missing_targets_package_mismatches_and_cycles() {
    let original = required_graph();
    let valid = String::from_utf8(encode(&original).unwrap()).unwrap();
    for directory in [
        "tools/install-1",
        "TOOLS/INSTALL-1",
        "tools/INSTALL-1/child",
        "TOOLS",
    ] {
        let mut dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
        dto.installed[1].directory = Cow::Borrowed(directory);
        assert_eq!(
            decode(&serde_json::to_vec(&dto).unwrap()),
            Err(Error::Graph)
        );
    }
    for case in 0..5 {
        let mut dto: SelectionRecord = serde_json::from_str(&valid).unwrap();
        match case {
            0 => dto.installed[2].requires[0].installation = [9; 32],
            1 => dto.installed[2].requires[0].package = Cow::Borrowed("wrong.package"),
            2 => {
                dto.installed[2].requires[0] = RequirementRecord {
                    package: Cow::Borrowed("test.consumer"),
                    installation: [3; 32],
                }
            }
            3 => dto.installed[0].requires.push(RequirementRecord {
                package: Cow::Borrowed("test.consumer"),
                installation: [3; 32],
            }),
            _ => dto.installed[2].requires[0].installation = [0; 32],
        }
        assert_eq!(
            decode(&serde_json::to_vec(&dto).unwrap()),
            Err(Error::Graph)
        );
    }
}

#[test]
fn checks_byte_limit_before_parsing_and_structural_limits_before_domain_decode() {
    for (length, accepted) in [(65_535, true), (65_536, true), (65_537, false)] {
        let mut bytes = encode(&graph()).unwrap();
        bytes.resize(length, b' ');
        let result = decode(&bytes);
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert_eq!(result, Err(Error::TooLarge));
        }
    }
    assert_eq!(
        decode(&vec![b'!'; BYTE_LIMIT as usize + 1]),
        Err(Error::TooLarge)
    );
    let deep = format!("{}0{}", "[".repeat(9), "]".repeat(9));
    assert_eq!(decode(deep.as_bytes()), Err(Error::InvalidRecord));
    let oversized_string = format!("\"{}\"", "a".repeat(MAX_DIRECTORY_BYTES + 1));
    assert_eq!(
        decode(oversized_string.as_bytes()),
        Err(Error::InvalidRecord)
    );
    let excessive_container = format!("[{}]", vec!["null"; MAX_ENTRIES + 1].join(","));
    assert_eq!(
        decode(excessive_container.as_bytes()),
        Err(Error::InvalidRecord)
    );
}

#[test]
fn admits_exact_graph_bounds_and_rejects_excess_outgoing_and_aggregate_edges() {
    let mut entries: Vec<_> = (1..=32)
        .map(|id| entry(id, &format!("test.p{id}"), &[]))
        .collect();
    for index in 16..24 {
        let requirements = entries[..16]
            .iter()
            .map(|target| Binding {
                package: target.installed.package.clone(),
                installation: target.installed.installation,
            })
            .collect();
        entries[index].requirements = requirements;
    }
    let graph = Graph::default().register(&entries).unwrap();
    let bytes = encode(&graph).unwrap();
    assert_eq!(decode(&bytes).unwrap(), graph);
    let text = String::from_utf8(bytes).unwrap();
    for case in 0..3 {
        let mut dto: SelectionRecord = serde_json::from_str(&text).unwrap();
        match case {
            0 => dto.installed[16].requires.push(RequirementRecord {
                package: Cow::Borrowed("test.p32"),
                installation: [32; 32],
            }),
            1 => dto.installed[24].requires.push(RequirementRecord {
                package: Cow::Borrowed("test.p1"),
                installation: [1; 32],
            }),
            _ => dto.installed.push(InstalledRecord {
                package: Cow::Borrowed("test.extra"),
                directory: Cow::Borrowed("tools/extra"),
                installation: [33; 32],
                receipt: DigestRecord {
                    length: 32,
                    hash: [2; 32],
                },
                manifest: DigestRecord {
                    length: 64,
                    hash: [3; 32],
                },
                requires: Vec::new(),
            }),
        }
        assert!(decode(&serde_json::to_vec(&dto).unwrap()).is_err());
    }
}

#[test]
fn output_buffer_stops_at_byte_limit_without_appending_a_partial_chunk() {
    let mut output = RecordBytes {
        bytes: vec![0; BYTE_LIMIT as usize - 1],
        oversized: false,
    };
    assert!(output.write_all(&[1, 2]).is_err());
    assert!(output.oversized);
    assert_eq!(output.bytes.len(), BYTE_LIMIT as usize - 1);
    output.write_all(&[1]).unwrap();
    assert_eq!(output.bytes.len(), BYTE_LIMIT as usize);
    assert!(output.write_all(&[2]).is_err());
    assert_eq!(output.bytes.len(), BYTE_LIMIT as usize);
}

#[test]
fn encode_rejects_a_valid_graph_whose_canonical_json_exceeds_the_publication_limit() {
    let mut entries: Vec<_> = (1_u8..=32)
        .map(|id| {
            let mut entry = entry(id, &format!("p{id:02}{}", "a".repeat(125)), &[]);
            entry.installed.installation = [id + 223; 32];
            entry.installed.directory =
                format!("{}/{}/{id:02}", "a".repeat(255), "b".repeat(253)).into();
            entry.installed.receipt = ContentDigest::from_parts(4096, [255; 32]);
            entry.installed.manifest = ContentDigest::from_parts(65_536, [255; 32]);
            entry
        })
        .collect();
    for index in 16..24 {
        let requirements = entries[..16]
            .iter()
            .map(|target| Binding {
                package: target.installed.package.clone(),
                installation: target.installed.installation,
            })
            .collect();
        entries[index].requirements = requirements;
    }
    let graph = Graph::default().register(&entries).unwrap();
    graph.validate().unwrap();
    assert_eq!(encode(&graph), Err(Error::TooLarge));
}

#[cfg(any(windows, target_os = "linux"))]
mod filesystem {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use nara_fs::{HostCapabilityOptions, TrustMode};

    use crate::project_host::file_publication::{
        FilePublicationBinding, FilePublicationOutcome, FilePublicationRejection,
    };

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
    const MOVED_PATH: &str = ".nara/source-tools/original.json";

    struct Fixture {
        directory: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "nara-selection-store-{}-{stamp}-{sequence}",
                std::process::id(),
            ));
            fs::create_dir(&directory).unwrap();
            Self { directory }
        }

        fn capability(&self, rights: CapabilityRights, trust: TrustMode) -> DirectoryCapability {
            #[cfg(windows)]
            let file = {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                    FILE_SHARE_READ, FILE_SHARE_WRITE,
                };

                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(&self.directory)
                    .unwrap()
            };
            #[cfg(target_os = "linux")]
            let file = fs::File::open(&self.directory).unwrap();
            DirectoryCapability::from_host_handle(file, HostCapabilityOptions::new(rights, trust))
                .unwrap()
        }

        fn create_parents(&self) {
            fs::create_dir(self.directory.join(METADATA_DIRECTORY)).unwrap();
            fs::create_dir(self.directory.join(".nara/source-tools")).unwrap();
        }

        fn write_record(&self, bytes: &[u8]) {
            fs::write(self.directory.join(SELECTION_PATH), bytes).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Every path is an exclusively created, known fixture object; never recurse.
            for path in [SELECTION_PATH, MOVED_PATH, ".nara", ".nara/source-tools"] {
                let path = self.directory.join(path);
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if !metadata.is_dir() || metadata.is_symlink() => {
                        fs::remove_file(path).unwrap();
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("fixture leaf inspection failed: {error}"),
                }
            }
            for path in [SELECTION_PATH, ".nara/source-tools", ".nara"] {
                match fs::remove_dir(self.directory.join(path)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("fixture directory cleanup failed: {error}"),
                }
            }
            fs::remove_dir(&self.directory).unwrap();
        }
    }

    #[test]
    fn loading_missing_directories_is_read_only_and_does_not_create_them() {
        let fixture = Fixture::new();
        let root = fixture.capability(CapabilityRights::ReadOnly, TrustMode::Untrusted);
        let loaded = load(&root).unwrap();
        assert_eq!(loaded.graph, Graph::default());
        assert!(loaded.host.is_none());
        assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 0);

        fs::create_dir(fixture.directory.join(METADATA_DIRECTORY)).unwrap();
        let loaded = load(&root).unwrap();
        assert_eq!(loaded.graph, Graph::default());
        assert!(loaded.host.is_none());
        assert_eq!(
            fs::read_dir(fixture.directory.join(METADATA_DIRECTORY))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn existing_parent_and_missing_leaf_bind_missing_without_any_write() {
        let fixture = Fixture::new();
        fixture.create_parents();
        let root = fixture.capability(CapabilityRights::ReadOnly, TrustMode::Untrusted);
        let loaded = load(&root).unwrap();
        assert_eq!(loaded.graph, Graph::default());
        let host = loaded.host.unwrap();
        assert_eq!(host.binding(), FilePublicationBinding::Missing);
        assert_eq!(host.accepted_source_observation(), None);
        assert_eq!(
            fs::read_dir(fixture.directory.join(".nara/source-tools"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn loaded_host_pins_the_actual_decoded_leaf_after_rename_and_unlink() {
        let fixture = Fixture::new();
        fixture.create_parents();
        let bytes = encode(&graph()).unwrap();
        fixture.write_record(&bytes);
        let root = fixture.capability(CapabilityRights::ReadOnly, TrustMode::Untrusted);
        let observed = root.open_file_exact(&fixed_path(SELECTION_PATH)).unwrap();
        let identity = observed.identity();
        let loaded = load(&root).unwrap();
        assert_eq!(loaded.graph, graph());
        let host = loaded.host.unwrap();
        drop(observed);
        let digest = ContentDigest::of_bytes(&bytes);
        assert_eq!(
            host.binding(),
            FilePublicationBinding::Existing { identity, digest }
        );
        let observation = host.accepted_source_observation().unwrap();
        assert_eq!(observation.0, identity);
        assert_eq!(observation.2, digest);

        fs::rename(
            fixture.directory.join(SELECTION_PATH),
            fixture.directory.join(MOVED_PATH),
        )
        .unwrap();
        fixture.write_record(b"a different current leaf");
        assert_eq!(host.test_accepted_source_bytes().unwrap(), bytes);
        fs::remove_file(fixture.directory.join(MOVED_PATH)).unwrap();
        assert_eq!(host.test_accepted_source_bytes().unwrap(), bytes);
        assert_eq!(host.accepted_source_observation(), Some(observation));
        assert_eq!(
            fs::read(fixture.directory.join(SELECTION_PATH)).unwrap(),
            b"a different current leaf"
        );
    }

    #[test]
    fn corrupt_unknown_and_oversized_records_never_become_empty_or_get_overwritten() {
        let fixture = Fixture::new();
        fixture.create_parents();
        let root = fixture.capability(CapabilityRights::ReadWrite, TrustMode::Untrusted);
        for (bytes, expected) in [
            (b"not JSON".to_vec(), Error::InvalidRecord),
            (
                record().replacen('{', "{\"unknown\":true,", 1).into_bytes(),
                Error::InvalidRecord,
            ),
            (
                record()
                    .replace("\"format_version\":1", "\"format_version\":2")
                    .into_bytes(),
                Error::UnsupportedFormat,
            ),
            (vec![b'!'; BYTE_LIMIT as usize + 1], Error::TooLarge),
        ] {
            fixture.write_record(&bytes);
            assert_eq!(load(&root).unwrap_err(), expected);
            assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
            assert_eq!(
                fs::read(fixture.directory.join(SELECTION_PATH)).unwrap(),
                bytes
            );
            assert_eq!(
                fs::read_dir(fixture.directory.join(".nara/source-tools"))
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    #[test]
    fn explicit_creation_prepares_only_fixed_directories_and_refuses_existing_selection() {
        let fixture = Fixture::new();
        let root = fixture.capability(CapabilityRights::ReadWrite, TrustMode::Untrusted);
        let host = create_missing_host(&root).unwrap();
        assert_eq!(host.binding(), FilePublicationBinding::Missing);
        assert!(fixture.directory.join(".nara/source-tools").is_dir());
        assert!(!fixture.directory.join(SELECTION_PATH).exists());
        let bytes = encode(&graph()).unwrap();
        fixture.write_record(&bytes);
        assert_eq!(create_missing_host(&root).unwrap_err(), Error::Changed);
        assert_eq!(
            fs::read(fixture.directory.join(SELECTION_PATH)).unwrap(),
            bytes
        );
        assert!(!host.has_cleanup());
    }

    #[test]
    fn missing_binding_uses_no_replace_when_a_later_writer_claims_the_leaf() {
        let fixture = Fixture::new();
        let root = fixture.capability(CapabilityRights::ReadWrite, TrustMode::Untrusted);
        let mut host = create_missing_host(&root).unwrap();
        let bytes = b"another writer owns this leaf";
        fixture.write_record(bytes);
        let result = host.publish(&encode(&graph()).unwrap());
        assert!(matches!(
            result,
            FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged)
        ));
        assert_eq!(
            fs::read(fixture.directory.join(SELECTION_PATH)).unwrap(),
            bytes
        );
        assert_eq!(host.binding(), FilePublicationBinding::Missing);
        assert!(!host.has_cleanup());
    }

    #[test]
    fn explicit_creation_preserves_read_only_rights_without_creating_directories() {
        let fixture = Fixture::new();
        let root = fixture.capability(CapabilityRights::ReadOnly, TrustMode::Untrusted);
        assert_eq!(create_missing_host(&root).unwrap_err(), Error::ReadOnly);
        assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 0);
    }

    #[test]
    fn non_directory_parents_and_nonregular_leaf_are_never_missing() {
        let fixture = Fixture::new();
        let root = fixture.capability(CapabilityRights::ReadOnly, TrustMode::Untrusted);
        fs::write(fixture.directory.join(METADATA_DIRECTORY), b"keep").unwrap();
        assert!(load(&root).is_err());
        assert_eq!(
            fs::read(fixture.directory.join(METADATA_DIRECTORY)).unwrap(),
            b"keep"
        );
        fs::remove_file(fixture.directory.join(METADATA_DIRECTORY)).unwrap();
        fs::create_dir(fixture.directory.join(METADATA_DIRECTORY)).unwrap();
        fs::write(fixture.directory.join(".nara/source-tools"), b"keep").unwrap();
        assert!(load(&root).is_err());
        fs::remove_file(fixture.directory.join(".nara/source-tools")).unwrap();
        fs::create_dir(fixture.directory.join(".nara/source-tools")).unwrap();
        fs::create_dir(fixture.directory.join(SELECTION_PATH)).unwrap();
        assert!(load(&root).is_err());
        assert!(fixture.directory.join(SELECTION_PATH).is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn case_aliases_of_each_fixed_component_reject_without_writes() {
        for (alias_metadata, alias_tools, alias_leaf) in [
            (".NARA", "Source-Tools", "Selection.json"),
            (".nara", "Source-Tools", "selection.json"),
            (".nara", "source-tools", "Selection.json"),
        ] {
            let fixture = Fixture::new();
            fs::create_dir(fixture.directory.join(alias_metadata)).unwrap();
            fs::create_dir(fixture.directory.join(alias_metadata).join(alias_tools)).unwrap();
            let bytes = encode(&graph()).unwrap();
            fs::write(
                fixture
                    .directory
                    .join(alias_metadata)
                    .join(alias_tools)
                    .join(alias_leaf),
                &bytes,
            )
            .unwrap();
            let root = fixture.capability(CapabilityRights::ReadWrite, TrustMode::Untrusted);
            assert_eq!(load(&root).unwrap_err(), Error::InvalidPath);
            assert_eq!(create_missing_host(&root).unwrap_err(), Error::InvalidPath);
            assert_eq!(
                fs::read(
                    fixture
                        .directory
                        .join(alias_metadata)
                        .join(alias_tools)
                        .join(alias_leaf),
                )
                .unwrap(),
                bytes,
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dangling_symlink_is_rejected_instead_of_treated_as_missing() {
        let fixture = Fixture::new();
        fixture.create_parents();
        std::os::unix::fs::symlink("absent", fixture.directory.join(SELECTION_PATH)).unwrap();
        let root = fixture.capability(CapabilityRights::ReadWrite, TrustMode::Untrusted);
        assert!(load(&root).is_err());
        assert!(create_missing_host(&root).is_err());
        assert!(
            fs::symlink_metadata(fixture.directory.join(SELECTION_PATH))
                .unwrap()
                .is_symlink()
        );
    }
}
