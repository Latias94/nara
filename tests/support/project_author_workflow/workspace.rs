use std::{fs, path::Path};

pub(super) fn read_toml(path: &Path) -> toml::Value {
    read_toml_bytes(&fs::read(path).unwrap())
}

fn read_toml_bytes(bytes: &[u8]) -> toml::Value {
    toml::from_str(std::str::from_utf8(bytes).unwrap()).unwrap()
}
