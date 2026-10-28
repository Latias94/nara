use std::collections::{BTreeMap, BTreeSet};

use nara_fs::{ContentDigest, RelativeComponent, RelativePath};
use nara_reflect::{ComponentTypeId, PersistentComponentSchemaOwnerId};
use serde::Deserialize;

pub(super) const MANIFEST_NAME: &str = "nara-tool.json";
pub(super) const MAX_MANIFEST_BYTES: u64 = 65_536;
pub(super) const MAX_FILES: usize = 64;
pub(super) const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_MEMBER_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct Manifest {
    pub package: String,
    pub release: String,
    pub host_target: String,
    pub owner: String,
    pub component: String,
    pub catalog: String,
    pub catalog_predecessor: Option<String>,
    pub entry: String,
    pub requires: Vec<Requirement>,
    pub files: Vec<Member>,
}

#[derive(Debug)]
pub(super) struct Requirement {
    pub package: String,
    pub manifest: ContentDigest,
}

#[derive(Debug)]
pub(super) struct Member {
    pub path: String,
    pub digest: ContentDigest,
    pub kind: MemberKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberKind {
    Tool,
    Content,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDto {
    format_version: u32,
    package: String,
    release: String,
    host_target: String,
    authority: String,
    protocol_version: u32,
    owner: String,
    component: String,
    catalog: String,
    #[serde(default)]
    catalog_predecessor: Option<String>,
    entry: String,
    requires: Vec<RequirementDto>,
    files: Vec<MemberDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementDto {
    package: String,
    manifest: DigestDto,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestDto {
    length: u64,
    blake3: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberDto {
    path: String,
    length: u64,
    blake3: String,
    kind: String,
}

impl Manifest {
    /// Admits declarations only; payload capture and current-host selection belong to the caller.
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_MANIFEST_BYTES as usize {
            return Err("manifest exceeds byte limit");
        }
        let input: ManifestDto =
            serde_json::from_slice(bytes).map_err(|_| "invalid manifest JSON")?;
        if input.format_version != 1
            || input.protocol_version != 1
            || input.authority != "trusted_process"
        {
            return Err("unsupported manifest version, authority or protocol");
        }

        // Complete declared cost preflight before admitting any member or touching payloads.
        if input.files.is_empty() || input.files.len() > MAX_FILES {
            return Err("invalid manifest file count");
        }
        if input.requires.len() > 16 {
            return Err("too many manifest requirements");
        }
        let mut total_bytes = 0_u64;
        for member in &input.files {
            if member.length > MAX_MEMBER_BYTES {
                return Err("member exceeds byte limit");
            }
            total_bytes = total_bytes
                .checked_add(member.length)
                .ok_or("members exceed total byte limit")?;
            if total_bytes > MAX_TOTAL_BYTES {
                return Err("members exceed total byte limit");
            }
        }

        if !valid_package(&input.package) {
            return Err("invalid manifest package");
        }
        if !input.release.is_ascii()
            || input.release.is_empty()
            || input.release.len() > 128
            || !input.release.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+')
            })
        {
            return Err("invalid manifest release");
        }
        if !matches!(
            input.host_target.as_str(),
            "x86_64-pc-windows-msvc" | "x86_64-unknown-linux-gnu"
        ) {
            return Err("unsupported manifest host target");
        }
        PersistentComponentSchemaOwnerId::try_new(input.owner.as_str())
            .map_err(|_| "invalid manifest owner")?;
        ComponentTypeId::try_new(input.component.as_str())
            .map_err(|_| "invalid manifest component")?;
        validate_path(&input.catalog)?;
        validate_path(&input.entry)?;
        if input.catalog == input.entry {
            return Err("catalog and entry must be distinct");
        }
        if let Some(predecessor) = &input.catalog_predecessor {
            validate_path(predecessor)?;
            if predecessor == &input.catalog
                || predecessor == &input.entry
                || predecessor.eq_ignore_ascii_case(MANIFEST_NAME)
            {
                return Err(
                    "catalog predecessor must be distinct from catalog, entry and manifest",
                );
            }
        }

        let mut packages = BTreeSet::new();
        let mut requires = Vec::with_capacity(input.requires.len());
        for requirement in input.requires {
            if !valid_package(&requirement.package)
                || requirement.package == input.package
                || !packages.insert(requirement.package.clone())
            {
                return Err("invalid, duplicate or self manifest requirement");
            }
            if requirement.manifest.length == 0 || requirement.manifest.length > MAX_MANIFEST_BYTES
            {
                return Err("invalid required manifest byte length");
            }
            requires.push(Requirement {
                package: requirement.package,
                manifest: decode_digest(requirement.manifest.length, &requirement.manifest.blake3)?,
            });
        }

        // The manifest itself also occupies a file path and cannot become a directory.
        let mut paths = BTreeSet::from([MANIFEST_NAME.to_owned()]);
        let mut files = Vec::with_capacity(input.files.len());
        for member in input.files {
            validate_path(&member.path)?;
            if !paths.insert(member.path.to_ascii_lowercase()) {
                return Err("duplicate or reserved manifest member path");
            }
            let kind = match member.kind.as_str() {
                "tool" => MemberKind::Tool,
                "content" => MemberKind::Content,
                _ => return Err("unsupported manifest member kind"),
            };
            files.push(Member {
                path: member.path,
                digest: decode_digest(member.length, &member.blake3)?,
                kind,
            });
        }
        for path in &paths {
            for (separator, _) in path.match_indices('/') {
                if paths.contains(&path[..separator]) {
                    return Err("manifest member file and directory paths collide");
                }
            }
        }
        let mut directories = BTreeMap::new();
        for member in &files {
            for (separator, _) in member.path.match_indices('/') {
                let prefix = &member.path[..separator];
                if directories
                    .insert(prefix.to_ascii_lowercase(), prefix.to_owned())
                    .is_some_and(|spelling| spelling != prefix)
                {
                    return Err("manifest directory spelling aliases collide");
                }
            }
        }
        for path in [&input.catalog, &input.entry]
            .into_iter()
            .chain(input.catalog_predecessor.iter())
        {
            if !files.iter().any(|member| {
                member.path == *path
                    && member.kind == MemberKind::Tool
                    && member.digest.length() != 0
            }) {
                return Err("catalogs and entry must exactly name nonempty tool members");
            }
        }

        Ok(Self {
            package: input.package,
            release: input.release,
            host_target: input.host_target,
            owner: input.owner,
            component: input.component,
            catalog: input.catalog,
            catalog_predecessor: input.catalog_predecessor,
            entry: input.entry,
            requires,
            files,
        })
    }
}

fn valid_package(value: &str) -> bool {
    value.is_ascii()
        && !value.is_empty()
        && value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn validate_path(value: &str) -> Result<(), &'static str> {
    // Establish ASCII before any case folding or native path interpretation.
    if !value.is_ascii() {
        return Err("manifest paths must be ASCII");
    }
    if value.is_empty()
        || value.len() > 512
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return Err("invalid manifest path");
    }
    if value.split('/').count() > 8 {
        return Err("manifest path has too many components");
    }
    for component in value.split('/') {
        if matches!(component, "" | "." | "..") || component.ends_with('.') {
            return Err("invalid manifest path component");
        }
        RelativeComponent::new(component).map_err(|_| "invalid manifest path component")?;
    }
    RelativePath::new(value).map_err(|_| "invalid manifest relative path")?;
    Ok(())
}

fn decode_digest(length: u64, blake3: &str) -> Result<ContentDigest, &'static str> {
    if !blake3.is_ascii()
        || blake3.len() != 64
        || !blake3
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid canonical BLAKE3 digest");
    }
    let mut hash = [0; 32];
    for (index, byte) in hash.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&blake3[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid canonical BLAKE3 digest")?;
    }
    Ok(ContentDigest::from_parts(length, hash))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn member(path: &str, length: u64, kind: &str) -> Value {
        json!({ "path": path, "length": length, "blake3": HASH, "kind": kind })
    }

    fn requirement(package: &str, length: u64) -> Value {
        json!({ "package": package, "manifest": { "length": length, "blake3": HASH } })
    }

    fn fixture() -> Value {
        json!({
            "format_version": 1,
            "package": "example.enemy-tools",
            "release": "1.0.0+local",
            "host_target": "x86_64-pc-windows-msvc",
            "authority": "trusted_process",
            "protocol_version": 1,
            "owner": "example.enemy",
            "component": "EnemyStats",
            "catalog": "catalog.ron",
            "entry": "bin/tool.exe",
            "requires": [requirement("example.shared", 17)],
            "files": [member("catalog.ron", 3, "tool"), member("bin/tool.exe", 5, "tool")]
        })
    }

    fn decode(value: &Value) -> Result<Manifest, &'static str> {
        Manifest::decode(&serde_json::to_vec(value).unwrap())
    }

    fn fixture_with_predecessor() -> Value {
        let mut value = fixture();
        value["catalog_predecessor"] = json!("catalog-predecessor.ron");
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("catalog-predecessor.ron", 1, "tool"));
        value
    }

    #[test]
    fn predecessor_is_one_semantic_optional_manifest_declaration() {
        let mut value = fixture();
        assert!(decode(&value).unwrap().catalog_predecessor.is_none());
        value["catalog_predecessor"] = Value::Null;
        assert!(decode(&value).unwrap().catalog_predecessor.is_none());

        let admitted = decode(&fixture_with_predecessor()).unwrap();
        assert_eq!(
            admitted.catalog_predecessor.as_deref(),
            Some("catalog-predecessor.ron")
        );
        assert_eq!(admitted.files.len(), 3);
        assert_eq!(admitted.files[2].kind, MemberKind::Tool);
    }

    #[test]
    fn predecessor_requires_a_distinct_exact_nonempty_tool_member() {
        for path in [
            "",
            "missing.ron",
            "Catalog-predecessor.ron",
            "catalog.ron",
            "bin/tool.exe",
            MANIFEST_NAME,
            "NARA-TOOL.JSON",
            "../catalog-predecessor.ron",
        ] {
            let mut value = fixture_with_predecessor();
            value["catalog_predecessor"] = json!(path);
            assert!(decode(&value).is_err(), "{path:?}");
        }
        for (field, invalid) in [("kind", json!("content")), ("length", json!(0))] {
            let mut value = fixture_with_predecessor();
            value["files"][2][field] = invalid;
            assert!(decode(&value).is_err(), "{field}");
        }
        for invalid in [json!([]), json!({}), json!(false), json!(1)] {
            let mut value = fixture_with_predecessor();
            value["catalog_predecessor"] = invalid;
            assert!(decode(&value).is_err());
        }
    }

    #[test]
    fn predecessor_rejects_duplicate_fields_and_alternate_field_names() {
        for predecessor in [json!("catalog-predecessor.ron"), Value::Null] {
            let mut value = fixture_with_predecessor();
            value["catalog_predecessor"] = predecessor.clone();
            let encoded = serde_json::to_string(&value).unwrap();
            let field = format!("\"catalog_predecessor\":{predecessor}");
            let duplicated = encoded.replacen(&field, &format!("{field},{field}"), 1);
            assert_ne!(duplicated, encoded);
            assert!(Manifest::decode(duplicated.as_bytes()).is_err());
        }
        let mut value = fixture_with_predecessor();
        let predecessor = value.as_object_mut().unwrap().remove("catalog_predecessor");
        value["predecessor_catalog"] = predecessor.unwrap();
        assert!(decode(&value).is_err());
    }

    #[test]
    fn predecessor_uses_the_existing_member_namespace() {
        for path in [
            "catalog-predecessor.ron",
            "CATALOG-PREDECESSOR.RON",
            "catalog-predecessor.ron/child",
        ] {
            let mut value = fixture_with_predecessor();
            value["files"]
                .as_array_mut()
                .unwrap()
                .push(member(path, 1, "tool"));
            assert!(decode(&value).is_err(), "{path}");
        }
    }

    #[test]
    fn predecessor_shares_file_count_and_payload_byte_limits() {
        let mut value = fixture_with_predecessor();
        value["files"][2]["length"] = json!(MAX_MEMBER_BYTES + 1);
        assert_eq!(decode(&value).unwrap_err(), "member exceeds byte limit");

        value = fixture_with_predecessor();
        for index in 3..MAX_FILES {
            value["files"].as_array_mut().unwrap().push(member(
                &format!("aux-{index}"),
                0,
                "content",
            ));
        }
        assert_eq!(decode(&value).unwrap().files.len(), MAX_FILES);
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("extra", 0, "content"));
        assert_eq!(decode(&value).unwrap_err(), "invalid manifest file count");

        value = fixture_with_predecessor();
        value["files"][0]["length"] = json!(MAX_MEMBER_BYTES);
        value["files"][1]["length"] = json!(MAX_MEMBER_BYTES);
        value["files"].as_array_mut().unwrap().extend([
            member("data/a", MAX_MEMBER_BYTES, "content"),
            member("data/b", MAX_MEMBER_BYTES - 1, "content"),
        ]);
        assert!(decode(&value).is_ok());
        value["files"][2]["length"] = json!(2);
        assert_eq!(
            decode(&value).unwrap_err(),
            "members exceed total byte limit"
        );
    }

    #[test]
    fn admits_both_finite_targets_and_preserves_declarations() {
        for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"] {
            let mut value = fixture();
            value["host_target"] = json!(target);
            let manifest = decode(&value).unwrap();
            assert_eq!(manifest.package, "example.enemy-tools");
            assert_eq!(manifest.release, "1.0.0+local");
            assert_eq!(manifest.host_target, target);
            assert_eq!(manifest.owner, "example.enemy");
            assert_eq!(manifest.component, "EnemyStats");
            assert_eq!(manifest.catalog, "catalog.ron");
            assert_eq!(manifest.entry, "bin/tool.exe");
            assert_eq!(manifest.requires.len(), 1);
            assert_eq!(manifest.requires[0].package, "example.shared");
            assert_eq!(manifest.requires[0].manifest.length(), 17);
            assert_eq!(manifest.files.len(), 2);
            assert_eq!(manifest.files[1].kind, MemberKind::Tool);
            assert_eq!(manifest.files[1].digest.length(), 5);
            let expected_hash = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef].repeat(4);
            assert_eq!(
                manifest.files[1].digest.as_bytes().as_slice(),
                expected_hash.as_slice()
            );
        }
    }

    #[test]
    fn bounds_input_before_json_decoding() {
        let mut bytes = serde_json::to_vec(&fixture()).unwrap();
        bytes.resize(MAX_MANIFEST_BYTES as usize, b' ');
        assert!(Manifest::decode(&bytes).is_ok());
        bytes.push(b' ');
        assert_eq!(
            Manifest::decode(&bytes).unwrap_err(),
            "manifest exceeds byte limit"
        );
        bytes.fill(b'x');
        assert_eq!(
            Manifest::decode(&bytes).unwrap_err(),
            "manifest exceeds byte limit"
        );
    }

    #[test]
    fn rejects_unknown_and_missing_fields_at_every_object_level() {
        let fields: &[(&str, &[&str])] = &[
            (
                "",
                &[
                    "format_version",
                    "package",
                    "release",
                    "host_target",
                    "authority",
                    "protocol_version",
                    "owner",
                    "component",
                    "catalog",
                    "entry",
                    "requires",
                    "files",
                ],
            ),
            ("/requires/0", &["package", "manifest"]),
            ("/requires/0/manifest", &["length", "blake3"]),
            ("/files/0", &["path", "length", "blake3", "kind"]),
        ];
        for (pointer, required) in fields {
            let mut value = fixture();
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".to_owned(), json!(true));
            assert!(decode(&value).is_err(), "unknown field at {pointer}");
            for field in *required {
                let mut value = fixture();
                value
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(*field);
                assert!(decode(&value).is_err(), "missing {pointer}/{field}");
            }
        }
    }

    #[test]
    fn rejects_duplicate_fields_instead_of_collapsing_them() {
        let encoded = serde_json::to_string(&fixture()).unwrap();
        for field in [
            r#""format_version":1"#,
            r#""package":"example.shared""#,
            r#""length":17"#,
            r#""path":"catalog.ron""#,
        ] {
            let duplicated = encoded.replacen(field, &format!("{field},{field}"), 1);
            assert_ne!(duplicated, encoded);
            assert!(Manifest::decode(duplicated.as_bytes()).is_err(), "{field}");
        }
    }

    #[test]
    fn rejects_unknown_versions_authorities_roles_and_wrong_shapes() {
        for (pointer, invalid) in [
            ("/format_version", json!(0)),
            ("/format_version", json!(2)),
            ("/format_version", json!(1.0)),
            ("/protocol_version", json!(2)),
            ("/authority", json!("sandbox")),
            ("/authority", json!("Trusted_Process")),
            ("/files/0/kind", json!("Tool")),
            ("/files/0/kind", json!("executable")),
            ("/files/0/kind", json!({ "tool": null })),
            ("/requires", Value::Null),
            ("/files/0/length", json!(-1)),
            ("/files/0/length", json!(1.5)),
            ("/files/0/length", json!("3")),
        ] {
            let mut value = fixture();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(decode(&value).is_err(), "{pointer}: {value}");
        }
        let encoded = serde_json::to_string(&fixture()).unwrap();
        assert!(Manifest::decode(format!("{encoded} {{}}").as_bytes()).is_err());
        assert!(Manifest::decode(b"[]").is_err());
    }

    #[test]
    fn validates_package_release_and_reflection_identity_boundaries() {
        for (field, invalid) in [
            ("package", ""),
            ("package", "Example"),
            ("package", "-example"),
            ("package", "example+build"),
            ("package", "example/name"),
            ("package", "exаmple"),
            ("release", ""),
            ("release", "1 0"),
            ("release", "1/0"),
            ("release", "１.0"),
            ("owner", ""),
            ("owner", ".owner"),
            ("owner", "owner/name"),
            ("owner", "ownér"),
            ("component", ""),
            ("component", "-type"),
            ("component", "type/name"),
            ("component", "Typе"),
        ] {
            let mut value = fixture();
            value[field] = json!(invalid);
            assert!(decode(&value).is_err(), "{field}: {invalid:?}");
        }
        for field in ["package", "release", "owner", "component"] {
            let mut value = fixture();
            value[field] = json!("a".repeat(128));
            assert!(decode(&value).is_ok(), "{field} at limit");
            value[field] = json!("a".repeat(129));
            assert!(decode(&value).is_err(), "{field} over limit");
        }
        let mut value = fixture();
        value["package"] = json!("0.example_pkg-1");
        value["release"] = json!("+A_1.0-local");
        assert!(decode(&value).is_ok());
    }

    #[test]
    fn rejects_targets_outside_the_finite_allowlist() {
        for target in [
            "",
            "windows",
            "x86_64-pc-windows-gnu",
            "x86_64-unknown-linux-musl",
            "aarch64-pc-windows-msvc",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "X86_64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu-extra",
        ] {
            let mut value = fixture();
            value["host_target"] = json!(target);
            assert!(decode(&value).is_err(), "{target}");
        }
    }

    #[test]
    fn bounds_requirements_and_rejects_duplicate_self_or_invalid_packages() {
        let mut value = fixture();
        value["requires"] = json!([]);
        assert!(decode(&value).is_ok());
        value["requires"] = json!(
            (0..16)
                .map(|index| requirement(&format!("example.dependency-{index}"), 1))
                .collect::<Vec<_>>()
        );
        assert!(decode(&value).is_ok());
        value["requires"]
            .as_array_mut()
            .unwrap()
            .push(requirement("example.extra", 1));
        assert_eq!(
            decode(&value).unwrap_err(),
            "too many manifest requirements"
        );
        for package in [
            "example.shared",
            "example.enemy-tools",
            "Example.shared",
            "example/shаred",
        ] {
            let mut value = fixture();
            value["requires"]
                .as_array_mut()
                .unwrap()
                .push(requirement(package, 1));
            assert!(decode(&value).is_err(), "{package}");
        }
    }

    #[test]
    fn requires_positive_bounded_manifest_lengths() {
        for length in [1, MAX_MANIFEST_BYTES] {
            let mut value = fixture();
            value["requires"][0]["manifest"]["length"] = json!(length);
            assert!(decode(&value).is_ok());
        }
        for length in [0, MAX_MANIFEST_BYTES + 1, u64::MAX] {
            let mut value = fixture();
            value["requires"][0]["manifest"]["length"] = json!(length);
            assert!(decode(&value).is_err(), "{length}");
        }
    }

    #[test]
    fn requires_canonical_lowercase_hex_digests() {
        for invalid in [
            String::new(),
            "0".repeat(63),
            "0".repeat(65),
            HASH.to_ascii_uppercase(),
            "g".repeat(64),
            format!("{}é", "0".repeat(62)),
            format!("{} ", "0".repeat(63)),
        ] {
            for pointer in ["/requires/0/manifest/blake3", "/files/0/blake3"] {
                let mut value = fixture();
                *value.pointer_mut(pointer).unwrap() = json!(invalid);
                assert!(decode(&value).is_err(), "{pointer}: {invalid:?}");
            }
        }
    }

    #[test]
    fn bounds_file_count_and_allows_empty_auxiliary_members() {
        let mut value = fixture();
        value["files"] = json!([]);
        assert_eq!(decode(&value).unwrap_err(), "invalid manifest file count");
        value = fixture();
        let files = value["files"].as_array_mut().unwrap();
        for index in 2..MAX_FILES {
            files.push(member(
                &format!("aux-{index}"),
                0,
                if index % 2 == 0 { "content" } else { "tool" },
            ));
        }
        let admitted = decode(&value).unwrap();
        assert_eq!(admitted.files.len(), MAX_FILES);
        assert_eq!(admitted.files[2].kind, MemberKind::Content);
        assert_eq!(admitted.files[2].digest.length(), 0);
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("extra", 0, "content"));
        assert_eq!(decode(&value).unwrap_err(), "invalid manifest file count");
    }

    #[test]
    fn preflights_member_and_total_cost_before_member_admission() {
        let mut value = fixture();
        value["files"][0]["length"] = json!(MAX_MEMBER_BYTES);
        assert!(decode(&value).is_ok());
        for length in [MAX_MEMBER_BYTES + 1, u64::MAX] {
            value["files"][0]["length"] = json!(length);
            value["files"][0]["blake3"] = json!("invalid");
            assert_eq!(decode(&value).unwrap_err(), "member exceeds byte limit");
        }

        value = fixture();
        value["files"][0]["length"] = json!(MAX_MEMBER_BYTES);
        value["files"][1]["length"] = json!(MAX_MEMBER_BYTES);
        value["files"].as_array_mut().unwrap().extend([
            member("data/a", MAX_MEMBER_BYTES, "content"),
            member("data/b", MAX_MEMBER_BYTES, "content"),
        ]);
        assert!(decode(&value).is_ok());
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("data/c", 1, "content"));
        value["files"][0]["path"] = json!("../invalid");
        value["files"][0]["blake3"] = json!("invalid");
        assert_eq!(
            decode(&value).unwrap_err(),
            "members exceed total byte limit"
        );
    }

    #[test]
    fn rejects_nonportable_path_spellings_before_native_normalization() {
        for path in [
            "",
            "/absolute",
            "a/",
            "a//b",
            "a\\b",
            ".",
            "..",
            "a/./b",
            "a/../b",
            "a.",
            "a /b",
            "a:b",
            "C:/b",
            "//server/file",
            "aux",
            "NUL.txt",
            "bin/COM1.dll",
            "lpt9.any",
            "COM¹",
            "a/COM².txt",
            "tool.exe:stream",
            "file\0name",
            "a+b",
            "café",
            "a／b",
            "a/．/b",
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .push(member(path, 0, "content"));
            assert!(decode(&value).is_err(), "{path:?}");
        }
        let mut value = fixture();
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("COM¹", 0, "content"));
        assert_eq!(decode(&value).unwrap_err(), "manifest paths must be ASCII");
    }

    #[test]
    fn bounds_path_bytes_segments_and_component_lengths() {
        let longest = [
            "a".repeat(128),
            "b".repeat(128),
            "c".repeat(128),
            "d".repeat(125),
        ]
        .join("/");
        assert_eq!(longest.len(), 512);
        for path in [
            longest.clone(),
            "a/b/c/d/e/f/g/h".to_owned(),
            ".hidden/A-1_2.ext".to_owned(),
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .push(member(&path, 0, "content"));
            assert!(decode(&value).is_ok(), "{path}");
        }
        for path in [
            format!("{longest}x"),
            "a/b/c/d/e/f/g/h/i".to_owned(),
            "a".repeat(256),
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .push(member(&path, 0, "content"));
            assert!(decode(&value).is_err(), "{path}");
        }
    }

    #[test]
    fn rejects_case_aliases_and_manifest_self_membership() {
        for path in [
            "catalog.ron",
            "CATALOG.RON",
            MANIFEST_NAME,
            "NARA-TOOL.JSON",
            "Nara-Tool.Json/child",
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .push(member(path, 0, "content"));
            assert!(decode(&value).is_err(), "{path}");
        }
        let mut value = fixture();
        value["files"]
            .as_array_mut()
            .unwrap()
            .push(member("data/nara-tool.json", 0, "content"));
        assert!(decode(&value).is_ok());
    }

    #[test]
    fn rejects_file_directory_ancestor_collisions_in_either_order() {
        for paths in [
            ["assets", "assets/data"],
            ["assets/data", "ASSETS"],
            ["data/FILE", "DATA/file/child"],
            ["bin/tool.exe/child", "unrelated"],
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .extend(paths.map(|path| member(path, 0, "content")));
            assert_eq!(
                decode(&value).unwrap_err(),
                "manifest member file and directory paths collide"
            );
        }
        let mut value = fixture();
        value["files"].as_array_mut().unwrap().extend([
            member("assets", 0, "content"),
            member("assets-more/data", 0, "content"),
            member("data/a", 0, "content"),
            member("data/b", 0, "content"),
        ]);
        assert!(decode(&value).is_ok());
    }

    #[test]
    fn rejects_directory_spelling_aliases_at_every_prefix_depth() {
        for paths in [
            ["Bin/a", "bin/b"],
            ["bin/b", "Bin/a"],
            ["Assets/one/a", "assets/two/b"],
            ["data/Inner/a", "data/inner/b"],
            ["a/b/c/d/e/f/G/one", "a/b/c/d/e/f/g/two"],
        ] {
            let mut value = fixture();
            value["files"]
                .as_array_mut()
                .unwrap()
                .extend(paths.map(|path| member(path, 0, "content")));
            assert_eq!(
                decode(&value).unwrap_err(),
                "manifest directory spelling aliases collide"
            );
        }
        let mut value = fixture();
        value["files"].as_array_mut().unwrap().extend([
            member("data/Inner/a", 0, "content"),
            member("data/Inner/b", 0, "content"),
            member("data/Other/c", 0, "content"),
        ]);
        assert!(decode(&value).is_ok());
    }

    #[test]
    fn catalog_and_entry_require_distinct_exact_nonempty_tool_members() {
        for (field, index, wrong_case) in
            [("catalog", 0, "Catalog.ron"), ("entry", 1, "bin/Tool.exe")]
        {
            for path in ["missing", wrong_case, "nara-tool.json", "catalоg.ron"] {
                let mut value = fixture();
                value[field] = json!(path);
                assert!(decode(&value).is_err(), "{field}: {path}");
            }
            for (member_field, invalid) in [("kind", json!("content")), ("length", json!(0))] {
                let mut value = fixture();
                value["files"][index][member_field] = invalid;
                assert!(decode(&value).is_err(), "{field}: {member_field}");
            }
        }
        let mut value = fixture();
        value["entry"] = value["catalog"].clone();
        assert_eq!(
            decode(&value).unwrap_err(),
            "catalog and entry must be distinct"
        );
    }
}
