use std::{fs, path::Path};

#[test]
fn manifest_and_sources_use_only_the_public_root_package() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("workspace = true"));
    assert!(!manifest.contains("[patch"));
    assert!(!manifest.contains("[dev-dependencies]"));
    assert!(!manifest.contains("[build-dependencies]"));
    assert!(root.join("Cargo.lock").is_file());

    let mut sources = String::new();
    collect_rs(&root.join("src"), &mut sources);
    assert!(!contains_private_engine_import(&sources));
    assert!(!sources.contains("crates/nara_"));
    assert!(!sources.contains("ComponentFieldSchema"));
    assert!(!sources.contains("ComponentValue::"));
    assert!(!sources.contains("ImageImporter"));
    assert!(!sources.contains("ImageFileImportRequest"));
    assert!(!sources.contains("ImageBytesImportRequest"));
}

fn contains_private_engine_import(sources: &str) -> bool {
    sources.lines().any(|line| {
        let line = line.trim_start();
        let imported = line
            .strip_prefix("use ")
            .or_else(|| line.strip_prefix("extern crate "));
        imported.is_some_and(|imported| {
            imported.starts_with("nara_") && !imported.starts_with("nara_reference_game")
        })
    })
}

fn collect_rs(directory: &Path, output: &mut String) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push_str(&fs::read_to_string(path).unwrap());
        }
    }
}
