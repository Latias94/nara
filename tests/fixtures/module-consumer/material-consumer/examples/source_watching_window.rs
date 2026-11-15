//! Ordinary main-thread Source product with scripted native edits and exact present receipts.

#[cfg(any(windows, target_os = "linux"))]
#[allow(dead_code, unused_imports)]
#[path = "../tests/source_watching.rs"]
mod source_watching;

fn main() {
    #[cfg(any(windows, target_os = "linux"))]
    source_watching::desktop::run();
    #[cfg(not(any(windows, target_os = "linux")))]
    panic!("managed native Scene watching is qualified only for Windows/Linux adapters");
}
