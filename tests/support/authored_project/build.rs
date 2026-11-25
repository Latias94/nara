fn main() {
    println!("cargo::rustc-check-cfg=cfg(nara_cooked_player)");
    println!("cargo::rerun-if-env-changed=NARA_CARGO_GENERATED_RUST_SOURCE");
    let source = match std::env::var("NARA_CARGO_GENERATED_RUST_SOURCE") {
        Ok(source) => source,
        Err(std::env::VarError::NotPresent) => return,
        Err(error) => panic!("invalid generated Player bootstrap: {error}"),
    };
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    std::fs::write(output.join("nara-player-bootstrap.rs"), source)
        .expect("write generated Player bootstrap to OUT_DIR");
    println!("cargo::rustc-cfg=nara_cooked_player");
}
