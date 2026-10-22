use crate::ecs::{self, prelude::*, system::SystemState};
use std::{
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Component)]
struct Value<T: Send + Sync + 'static>(T);

#[derive(Component)]
struct Samples<const N: usize>([u32; N]);

#[derive(Resource)]
struct Counter<const N: usize>(usize);

#[derive(ecs::Bundle)]
struct GenericBundle<'a, T = u32, const N: usize = 2>
where
    'a: 'static,
    T: Send + Sync + 'static,
{
    value: Value<T>,
    samples: Samples<N>,
    #[bundle(ignore)]
    ignored: usize,
    #[bundle(ignore)]
    marker: PhantomData<&'a ()>,
}

#[derive(ecs::bundle::Bundle)]
struct NestedBundle(GenericBundle<'static, u32, 2>);

#[derive(Bundle)]
#[bundle(ignore_from_components)]
struct SpawnOnlyBundle {
    value: Value<u64>,
    #[bundle(ignore)]
    _owned: NonDefault,
}

struct NonDefault(Arc<AtomicUsize>);

impl Drop for NonDefault {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Bundle)]
struct EmptyBundle;

#[derive(ecs::SystemParam)]
struct MutableParams<'w, 's, T, const N: usize>
where
    T: Send + Sync + 'static,
{
    #[system_param(validation_message = "the fixture counter must be installed")]
    counter: ResMut<'w, Counter<N>>,
    values: Query<'w, 's, &'static Value<T>>,
    commands: Commands<'w, 's>,
    calls: Local<'s, usize>,
}

#[derive(ecs::system::SystemParam)]
#[system_param(builder)]
struct ReadParams<'w, 's, T, const N: usize>
where
    T: Send + Sync + 'static,
{
    counter: Res<'w, Counter<N>>,
    values: Query<'w, 's, &'static Value<T>>,
}

#[derive(SystemParam)]
struct ManyParams<'s> {
    a: Local<'s, usize>,
    b: Local<'s, usize>,
    c: Local<'s, usize>,
    d: Local<'s, usize>,
    e: Local<'s, usize>,
    f: Local<'s, usize>,
    g: Local<'s, usize>,
    h: Local<'s, usize>,
    i: Local<'s, usize>,
    j: Local<'s, usize>,
    k: Local<'s, usize>,
    l: Local<'s, usize>,
    m: Local<'s, usize>,
    n: Local<'s, usize>,
    o: Local<'s, usize>,
    p: Local<'s, usize>,
    q: Local<'s, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct FixtureSchedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
struct FixtureSet;

#[test]
fn bundles_preserve_layout_nested_fields_and_from_components() {
    let mut world = World::new();
    let entity = world
        .spawn(NestedBundle(GenericBundle {
            value: Value(7),
            samples: Samples([2, 3]),
            ignored: 99,
            marker: PhantomData,
        }))
        .id();
    assert_eq!(world.get::<Value<u32>>(entity).unwrap().0, 7);
    assert_eq!(world.get::<Samples<2>>(entity).unwrap().0, [2, 3]);

    let extracted = world.entity_mut(entity).take::<NestedBundle>().unwrap().0;
    assert_eq!(extracted.value.0, 7);
    assert_eq!(extracted.samples.0, [2, 3]);
    assert_eq!(extracted.ignored, 0);
    assert!(world.get::<Value<u32>>(entity).is_none());
    assert!(world.get::<Samples<2>>(entity).is_none());

    let ignored_drops = Arc::new(AtomicUsize::new(0));
    let spawn_only = world
        .spawn(SpawnOnlyBundle {
            value: Value(11),
            _owned: NonDefault(Arc::clone(&ignored_drops)),
        })
        .id();
    assert_eq!(world.get::<Value<u64>>(spawn_only).unwrap().0, 11);
    assert_eq!(ignored_drops.load(Ordering::SeqCst), 1);
    world.spawn(EmptyBundle);
}

#[test]
fn system_params_rebind_generics_and_apply_deferred_commands() {
    let mut world = World::new();
    world.insert_resource(Counter::<3>(0));
    world.spawn(Value(5_u32));
    let mut state = SystemState::<MutableParams<u32, 3>>::new(&mut world);
    let deferred_entity;
    {
        let mut params = state.get_mut(&mut world).unwrap();
        params.counter.0 += params.values.iter().count();
        *params.calls += 1;
        assert_eq!(*params.calls, 1);
        deferred_entity = params.commands.spawn((Value(8_u32), Samples([1, 2, 3]))).id();
    }
    assert!(world.get::<Value<u32>>(deferred_entity).is_none());
    state.apply(&mut world);
    assert_eq!(world.get::<Value<u32>>(deferred_entity).unwrap().0, 8);
    assert_eq!(world.get::<Samples<3>>(deferred_entity).unwrap().0, [1, 2, 3]);
    assert_eq!(*state.get_mut(&mut world).unwrap().calls, 1);

    let mut readonly = SystemState::<ReadParams<u32, 3>>::new(&mut world);
    let params = readonly.get(&world).unwrap();
    assert_eq!(params.counter.0, 1);
    assert_eq!(params.values.iter().map(|v| v.0).sum::<u32>(), 13);
}

#[test]
fn system_param_builders_and_schedules_keep_upstream_semantics() {
    use ecs::system::{ParamBuilder, SystemParamBuilder};

    let mut world = World::new();
    world.insert_resource(Counter::<2>(3));
    world.spawn(Value(6_u32));
    let mut state = ReadParamsBuilder {
        counter: ParamBuilder::resource::<Counter<2>>(),
        values: ParamBuilder::query::<&Value<u32>>(),
    }
    .build_state(&mut world);
    let params: ReadParams<u32, 2> = state.get(&world).unwrap();
    assert_eq!(params.counter.0, 3);
    assert_eq!(params.values.iter().count(), 1);

    let mut schedule = Schedule::new(FixtureSchedule);
    schedule.add_systems(
        (|mut params: MutableParams<u32, 2>| {
            params.counter.0 += params.values.iter().count();
            params.commands.spawn(Value(10_u32));
        })
        .in_set(FixtureSet),
    );
    schedule.run(&mut world);
    assert_eq!(world.resource::<Counter<2>>().0, 4);
    assert_eq!(world.query::<&Value<u32>>().iter(&world).count(), 2);
}

#[test]
fn validation_keeps_the_custom_field_error_message() {
    let mut world = World::new();
    let mut state = SystemState::<MutableParams<u32, 3>>::new(&mut world);
    let error = match state.get_mut(&mut world) {
        Ok(_) => panic!("a missing required resource must fail parameter validation"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("the fixture counter must be installed"));
}

#[test]
fn system_params_over_the_tuple_limit_preserve_local_state() {
    let mut world = World::new();
    let mut state = SystemState::<ManyParams>::new(&mut world);
    {
        let params = state.get_mut(&mut world).unwrap();
        for mut local in [
            params.a, params.b, params.c, params.d, params.e, params.f, params.g, params.h, params.i,
            params.j, params.k, params.l, params.m, params.n, params.o, params.p, params.q,
        ] {
            *local += 1;
        }
    }
    let params = state.get_mut(&mut world).unwrap();
    assert_eq!(*params.a, 1);
    assert_eq!(*params.q, 1);
}
