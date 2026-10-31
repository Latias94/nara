//! An actual native Host tool with observable profile, feature and launch behavior.

#[cfg(feature = "unwanted-default")]
compile_error!("this consumer explicitly disables default features");
#[cfg(feature = "metadata-only")]
compile_error!("a decoy workspace dependency must not activate this selected binary's feature");
#[cfg(not(all(feature = "host-entry", feature = "expanded")))]
compile_error!("Cargo must expand the requested trial-host feature alias");

mod generation;

fn main() {
    std::fs::write("host-was-executed", b"explicit Host launch\n").unwrap();
    println!(
        "host-generation={};debug_assertions={};expanded={}",
        generation::GENERATION,
        cfg!(debug_assertions),
        cfg!(feature = "expanded"),
    );
}
