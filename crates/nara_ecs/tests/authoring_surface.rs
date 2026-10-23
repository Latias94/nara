use nara_ecs::{bevy_ecs, prelude::*};

#[derive(Component)]
struct Value(u32);

#[derive(Bundle)]
struct Values {
    value: Value,
}

#[derive(Default, Resource)]
struct Total(u32);

#[derive(SystemParam)]
struct Accumulate<'w, 's> {
    values: Query<'w, 's, &'static Value>,
    total: ResMut<'w, Total>,
    calls: Local<'s, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
struct Tick;

#[derive(Clone, Debug, PartialEq, Eq, Hash, SystemSet)]
struct Accumulation;

fn accumulate(mut params: Accumulate) {
    *params.calls += 1;
    params.total.0 += params.values.iter().map(|value| value.0).sum::<u32>() * *params.calls;
}

#[test]
fn ordinary_authoring_and_bevy_ecs_share_the_native_system_model() {
    let mut world: World = bevy_ecs::world::World::new();
    world.init_resource::<Total>();
    world.spawn(Values { value: Value(3) });

    let mut schedule: Schedule = bevy_ecs::schedule::Schedule::new(Tick);
    schedule.add_systems(accumulate.in_set(Accumulation));
    schedule.run(&mut world);
    schedule.run(&mut world);

    let native_total: bevy_ecs::change_detection::Ref<Total> = world.resource_ref::<Total>();
    assert_eq!(native_total.0, 9);
}
