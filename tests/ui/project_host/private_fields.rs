use nara::project_host::{HeadlessRunIntent, advanced::RuntimeProbe};

fn access_private_composition(probe: RuntimeProbe, intent: HeadlessRunIntent<()>) {
    let _ = probe.definition;
    let _ = intent.recipe;
}

fn main() {}
