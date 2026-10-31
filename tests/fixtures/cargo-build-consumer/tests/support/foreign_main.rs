//! A real WASI build-only consumer, without Nara or native execution qualification.

#[cfg(not(all(target_arch = "wasm32", target_os = "wasi")))]
compile_error!("the selected artifact must be built for wasm32-wasip1");
#[cfg(debug_assertions)]
compile_error!("the named consumer-checked profile must disable debug assertions");
#[cfg(feature = "unwanted-default")]
compile_error!("this consumer explicitly disables default features");
#[cfg(not(all(feature = "wasi-entry", feature = "expanded")))]
compile_error!("Cargo must expand the requested trial-wasi feature alias");

fn main() {
    std::fs::write("foreign-was-executed", b"this artifact is never executed\n").unwrap();
    println!("foreign build-only consumer");
}
