fn main() {
    println!("cargo::rustc-check-cfg=cfg(nara_cooked_player)");
    println!("cargo::rerun-if-changed=src/bin/player/generated.rs");
    if std::path::Path::new("src/bin/player/generated.rs").is_file() {
        println!("cargo::rustc-cfg=nara_cooked_player");
    }
}
