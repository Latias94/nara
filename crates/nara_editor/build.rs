fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // The unoptimized dockspace reducer exceeds the default Windows main thread stack.
    // Reserve virtual address space without changing the initial committed stack size.
    match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("msvc") => println!("cargo::rustc-link-arg=/STACK:8388608"),
        Ok("gnu") => println!("cargo::rustc-link-arg=-Wl,--stack,8388608"),
        _ => {}
    }
}
