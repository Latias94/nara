# nara_rapier2d

An immutable static Rapier2D query scene and a stateless top-down swept character controller.
The crate uses Rapier 0.33.0 directly and reexports it as `nara_rapier2d::rapier`. It depends on
`nara_core` for math and does not require the root Nara facade, an App, or an ECS World.

`affine_box_collider` converts a local box through a complete finite 2D affine matrix. Translation
places the native collider at the transformed center; rotation, scale, reflection, and shear
are baked into its native convex polygon. Admission requires four distinct retained corners and
normalizable edges. Local offsets and world corners must meet the absolute `1e-4` plus relative
`1e-5` coordinate budget. The affine row must be exactly `[0, 0, 1]`. Application coordinate and
size limits belong to the consumer.

`StaticCollisionScene::new` owns standalone, unattached native colliders and refreshes the native
BVH with one `CollisionPipeline::step`. It does not integrate dynamics or advance application
time. Empty scenes are valid. Construction admits finite poses and finite native world AABBs;
native shape validity remains the caller's responsibility. The scene has no mutation interface.
Replace it with another scene when geometry changes; retained old scenes remain independently
queryable. `queries(filter)` borrows the original native query pipeline, including its read-only
collider and body sets, for rays, shape casts, intersections, and custom native filtering.

`TopDownCharacter::new` owns a native `SharedShape`, checks its finite positive local AABB and
finite positive margin once, and stores a native `KinematicCharacterController` preset:

- Sliding enabled; autostep and ground snapping disabled.
- Fixed unit Y up, maximum climb angle `PI`, and minimum slide angle `-PI`.
- Absolute separation margin and Rapier's default normal nudge.

`move_shape` passes the desired world displacement directly to Rapier and returns its effective
translation and collision callback count. Convert velocity to displacement using `delta` once
at the call site. The duration is supplied to Rapier separately; a zero duration does not rescale
the supplied displacement. Collision counts describe callbacks, including repeated hits, rather
than unique colliders. The wrapper adds no outer sweep, slide solver, dynamic impulses, geometry
rescan, or scene rebuild. The native controller retains its own collision and grounding queries.
Neither scene nor character pose is mutated. Character pose, velocity, time, and history belong
to the consumer.

Initial penetration is not automatically recovered. Explicitly call `overlaps(scene, pose)`
before accepting a source pose, checkpoint, or restored pose. This uses native shape intersection
without adding the controller margin. Movement and placement queries exclude sensors. The
character is not inserted into the scene and needs no self-collider handle. Native query support
for shape combinations remains Rapier's contract.

```rust
use std::time::Duration;

use nara_core::{Mat3, Vec2};
use nara_rapier2d::{
    CollisionError, StaticCollisionScene, TopDownCharacter, affine_box_collider,
    rapier::{math::{Pose, Vector}, prelude::SharedShape},
};

fn movement() -> Result<(), CollisionError> {
    let wall = affine_box_collider(
        Vec2::ZERO,
        Vec2::new(0.05, 4.0),
        Mat3::from_translation(Vec2::new(2.0, 0.0)),
    )?;
    let scene = StaticCollisionScene::new([wall])?;
    let character = TopDownCharacter::new(SharedShape::ball(0.25), 0.01)?;
    let pose = Pose::IDENTITY;
    assert!(!character.overlaps(&scene, pose)?);

    let delta = Duration::from_millis(16);
    let desired = Vec2::new(4.0, 1.0) * delta.as_secs_f32();
    let movement = character.move_shape(&scene, pose, desired, delta)?;
    let next_pose = pose.append_translation(Vector::new(
        movement.translation.x,
        movement.translation.y,
    ));
    assert!(!character.overlaps(&scene, next_pose)?);
    Ok(())
}
```

Owner tests cover affine corner retention and admission, initial native BVH visibility and
filtering, empty and retained scenes, placement intersection, all four movement directions,
diagonal sliding, corners, fast thin-wall sweeps, and reuse without controller history. Build,
formatting, and tests use the repository's ordinary workspace commands.
