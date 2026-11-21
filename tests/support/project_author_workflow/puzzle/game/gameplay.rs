//! Game-owned grid movement and the authored pressure plate and exit.

#[path = "proof.rs"]
mod proof;

use std::{collections::VecDeque, ops::RangeInclusive};

use nara::{
    ProductRecipeError, SchemaContribution,
    advanced_prelude::{STARTUP_SCENE_ACTIVATION_PLUGIN_ID, add_startup_scene_activation_systems},
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId},
    ecs::{Commands, Entity, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    hierarchy::{
        HIERARCHY_PLUGIN_ID, HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement,
        HierarchyPlacementOwner,
    },
    input::{
        ActionContext, ActionId, ActionPhase, INPUT_PLUGIN_ID, InputSeatId, InputSet,
        SemanticActionCause, SemanticActions,
    },
    prelude::{
        App, Color, Component, CoreStage, FixedUpdateSet, PersistentComponent, Plugin, Query, Res,
        ResMut, Resource, Transform2d, Vec2,
    },
    reflect::{
        COMPONENT_REGISTRY_PLUGIN_ID, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition,
    },
    sprite::{SPRITE_PLUGIN_ID, Sprite},
    transform::TRANSFORM_PLUGIN_ID,
};

const PLUGIN_ID: PluginId = PluginId::new("push-box.gameplay");
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("push-box.components");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("push-box.components"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("push-box.components.native", 1),
    components::initial_catalog,
    components::register,
);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[
        COMPONENT_REGISTRY_PLUGIN_ID,
        HIERARCHY_PLUGIN_ID,
        INPUT_PLUGIN_ID,
        STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
        SPRITE_PLUGIN_ID,
        TRANSFORM_PLUGIN_ID,
    ])
    .provides_schema(&[PROVIDER_ID]);

const COLUMNS: RangeInclusive<i64> = 0..=5;
const ROWS: RangeInclusive<i64> = -1..=1;
const START_ACTOR: Cell = Cell::new(0, 0);
const START_BOX: Cell = Cell::new(1, 0);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const MAX_BUFFERED_PRESSES: usize = 16;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "push-box.PressurePlate",
    version = 1,
    alias = "Pressure plate",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct PressurePlate {
    #[nara(id = "column", alias = "Column")]
    column: i64,
    #[nara(id = "row", alias = "Row")]
    row: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "push-box.ExitDoor",
    version = 1,
    alias = "Exit door",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct ExitDoor {
    #[nara(id = "column", alias = "Column")]
    column: i64,
    #[nara(id = "row", alias = "Row")]
    row: i64,
}

nara::persistent_components! { mod components = [PressurePlate, ExitDoor]; }

#[derive(Default)]
pub struct GameplayPlugin;

/// Installs the game and its one native schema provider through the shared product recipe.
pub fn contribution() -> Result<SchemaContribution<GameplayPlugin>, ProductRecipeError> {
    SchemaContribution::for_default([PROVIDER])
}

impl Plugin for GameplayPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Controls>()?.add_systems(
            CoreStage::PreUpdate,
            capture_presses.after(InputSet::ResolveActions),
        )?;
        add_startup_scene_activation_systems(app, initialize)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (capture_presses, simulate)
                .chain()
                .in_set(FixedUpdateSet::Simulate),
        )?;
        proof::install(app)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    column: i64,
    row: i64,
}

impl Cell {
    const fn new(column: i64, row: i64) -> Self {
        Self { column, row }
    }

    fn offset(self, columns: i64, rows: i64) -> Self {
        Self::new(self.column + columns, self.row + rows)
    }

    fn in_room(self) -> bool {
        COLUMNS.contains(&self.column) && ROWS.contains(&self.row)
    }

    fn position(self) -> Vec2 {
        Vec2::new(self.column as f32 - 2.5, self.row as f32) * 72.0
    }
}

struct Piece {
    entity: Entity,
    cell: Cell,
}

#[derive(Resource)]
struct Puzzle {
    actor: Piece,
    box_piece: Piece,
    plate: Cell,
    door: Piece,
    door_open: bool,
    won: bool,
}

impl Puzzle {
    fn reset(&mut self) {
        self.actor.cell = START_ACTOR;
        self.box_piece.cell = START_BOX;
        self.door_open = START_BOX == self.plate;
        self.won = false;
    }

    fn passable(&self, cell: Cell) -> bool {
        cell.in_room() && (self.door_open || cell != self.door.cell)
    }

    fn move_actor(&mut self, columns: i64, rows: i64) -> bool {
        let destination = self.actor.cell.offset(columns, rows);
        if self.won || !self.passable(destination) {
            return false;
        }
        if destination == self.box_piece.cell {
            let box_destination = destination.offset(columns, rows);
            if !self.passable(box_destination) {
                return false;
            }
            self.box_piece.cell = box_destination;
            self.door_open |= box_destination == self.plate;
        }
        self.actor.cell = destination;
        self.won = self.door_open && destination == self.door.cell;
        true
    }
}

fn initialize(
    mut commands: Commands,
    plates: Query<(Entity, &PressurePlate)>,
    doors: Query<(Entity, &ExitDoor)>,
) -> Result<(), BevyError> {
    let (plate_entity, plate) = plates.single()?;
    let (door_entity, door) = doors.single()?;
    let plate = Cell::new(plate.column, plate.row);
    let door = Cell::new(door.column, door.row);
    if !plate.in_room() || !door.in_room() || [plate, START_ACTOR, START_BOX].contains(&door) {
        return Err("pressure plate and exit must occupy valid puzzle cells".into());
    }

    let place = |entity| HierarchyOperation::Place {
        entity,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    };
    let mut placements = Vec::new();
    for column in COLUMNS {
        for row in ROWS {
            let cell = square(
                Cell::new(column, row),
                68.0,
                Color::rgb(0.16, 0.22, 0.31),
                0,
            );
            placements.push(place(commands.spawn(cell).id()));
        }
    }
    let door_open = START_BOX == plate;
    commands
        .entity(plate_entity)
        .insert(square(plate, 56.0, Color::rgb(0.88, 0.69, 0.20), 1));
    commands
        .entity(door_entity)
        .insert(square(door, 60.0, door_color(door_open), 1));
    let actor = commands
        .spawn(square(START_ACTOR, 42.0, Color::rgb(0.25, 0.75, 0.94), 2))
        .id();
    let box_entity = commands
        .spawn(square(START_BOX, 46.0, Color::rgb(0.83, 0.40, 0.14), 2))
        .id();
    placements.extend([place(actor), place(box_entity)]);
    commands.apply_hierarchy_batch(placements);
    commands.insert_resource(Puzzle {
        actor: Piece {
            entity: actor,
            cell: START_ACTOR,
        },
        box_piece: Piece {
            entity: box_entity,
            cell: START_BOX,
        },
        plate,
        door: Piece {
            entity: door_entity,
            cell: door,
        },
        door_open,
        won: false,
    });
    Ok(())
}

fn square(cell: Cell, size: f32, color: Color, layer: i32) -> (Transform2d, Sprite) {
    (
        Transform2d::from_translation(cell.position()),
        Sprite::from_color(Vec2::splat(size), color).with_layer(layer),
    )
}

fn door_color(open: bool) -> Color {
    if open {
        Color::rgb(0.16, 0.64, 0.42)
    } else {
        Color::rgb(0.67, 0.21, 0.28)
    }
}

#[derive(Clone, Copy)]
enum Control {
    Move(i64, i64),
    Reset,
}

#[derive(Resource)]
struct Controls {
    context: ActionContext,
    bindings: [(ActionId, Control); 5],
    last_started: Option<u64>,
    pending: VecDeque<Control>,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            context: ActionContext::gameplay(),
            bindings: [
                ("confirm", Control::Reset),
                ("move.left", Control::Move(-1, 0)),
                ("move.right", Control::Move(1, 0)),
                ("move.up", Control::Move(0, 1)),
                ("move.down", Control::Move(0, -1)),
            ]
            .map(|(action, control)| {
                (
                    ActionId::new(action).expect("gameplay action IDs are valid"),
                    control,
                )
            }),
            last_started: None,
            pending: VecDeque::with_capacity(MAX_BUFFERED_PRESSES),
        }
    }
}

fn capture_presses(actions: Res<SemanticActions>, mut controls: ResMut<Controls>) {
    let Controls {
        context,
        bindings,
        last_started,
        pending,
    } = &mut *controls;
    let observed_through = *last_started;
    for edge in actions.as_slice().iter().filter(|edge| {
        edge.seat == SEAT
            && edge.context == *context
            && edge.phase == ActionPhase::Started
            && edge.cause == SemanticActionCause::PhysicalInput
    }) {
        let Some(origin) = edge.provenance else {
            continue;
        };
        let sequence = origin.sequence();
        if observed_through.is_some_and(|last| sequence <= last) {
            continue;
        }
        *last_started = Some(last_started.map_or(sequence, |last| last.max(sequence)));
        if let Some((_, control)) = bindings.iter().find(|(action, _)| action == &edge.action)
            && pending.len() < MAX_BUFFERED_PRESSES
        {
            pending.push_back(*control);
        }
    }
}

fn simulate(
    mut puzzle: ResMut<Puzzle>,
    mut controls: ResMut<Controls>,
    mut transforms: Query<&mut Transform2d>,
    mut sprites: Query<&mut Sprite>,
) -> Result<(), BevyError> {
    let Some(control) = controls.pending.pop_front() else {
        return Ok(());
    };
    match control {
        Control::Reset => puzzle.reset(),
        Control::Move(columns, rows) => {
            if !puzzle.move_actor(columns, rows) {
                return Ok(());
            }
        }
    }
    transforms.get_mut(puzzle.actor.entity)?.translation = puzzle.actor.cell.position();
    transforms.get_mut(puzzle.box_piece.entity)?.translation = puzzle.box_piece.cell.position();
    sprites.get_mut(puzzle.door.entity)?.material.tint = door_color(puzzle.door_open);
    Ok(())
}
