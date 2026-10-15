use nara::app::{App, RuntimeInstance};

fn main() {
    let mut runtime = RuntimeInstance::start(App::new()).unwrap();
    let _scope = runtime.with_driver_scope(|scope| scope);
}
