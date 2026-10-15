use nara::app::{App, RuntimeInstance};

fn main() {
    let mut runtime = RuntimeInstance::start(App::new()).unwrap();
    runtime.world_mut();
}
