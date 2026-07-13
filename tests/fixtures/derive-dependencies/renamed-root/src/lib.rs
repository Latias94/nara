use engine::{PersistentComponent, ecs::Component};

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.RenamedRoot",
    version = 1,
    alias = "Renamed root",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct RenamedRoot {
    #[nara(id = "value", alias = "Value")]
    pub value: i64,
}
