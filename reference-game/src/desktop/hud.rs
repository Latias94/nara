use nara::{
    ecs::{
        Local, Single,
        bevy_ecs::{error::Result as SystemResult, query::With},
    },
    prelude::*,
    ui::prelude::{
        UiAlign, UiBorder, UiBrush, UiColor, UiCornerRadii, UiDimension, UiEdges, UiFlexDirection,
        UiJustify, UiStyle, UiTextAlignment, UiVisual,
    },
};

use crate::{
    Failure, Round, RoundStatus, arena,
    campaign::{Campaign, Sector},
};

const DOMAIN: &str = "night-relay";
const PANEL: UiColor = UiColor::rgba(0.025, 0.044, 0.064, 0.98);
const EDGE: UiColor = UiColor::rgba(0.095, 0.22, 0.26, 1.0);
const LIGHT: UiColor = UiColor::rgba(0.83, 0.94, 0.98, 1.0);
const SECONDARY: UiColor = UiColor::rgba(0.50, 0.66, 0.73, 1.0);
const MINT: UiColor = UiColor::rgba(0.25, 0.93, 0.72, 1.0);
const AMBER: UiColor = UiColor::rgba(1.0, 0.65, 0.20, 1.0);

#[derive(Component)]
pub(super) struct ResultPanel;

#[derive(Component)]
pub(super) enum Label {
    Vitals,
    Objective,
    Result,
    Reason,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Readout {
    sector: Sector,
    entering_sector: bool,
    status: RoundStatus,
    cells: usize,
    shields: u32,
    seconds: u64,
}

pub(super) fn setup(world: &mut World) -> SystemResult {
    let padding = UiEdges::symmetric(24.0, 22.0)?;
    let root = world
        .spawn((
            UiRoot::default(),
            UiRuntimeKey::new(DOMAIN, "hud")?,
            UiNode::new(
                UiStyle::fill()
                    .with_justify_content(UiJustify::SpaceBetween)
                    .with_padding(padding),
            ),
        ))
        .id();
    let plate = UiVisual::solid(PANEL)
        .with_border(UiBorder::new(1.0, UiBrush::Solid(EDGE))?)
        .with_radii(UiCornerRadii::all(7.0)?);
    let top = world
        .spawn((
            ChildOf(root),
            plate.clone(),
            UiNode::new(
                UiStyle::flex()
                    .with_flex_direction(UiFlexDirection::Row)
                    .with_align_items(UiAlign::Center)
                    .with_justify_content(UiJustify::SpaceBetween)
                    .with_gap(28.0)
                    .with_padding(UiEdges::symmetric(18.0, 10.0)?),
            ),
        ))
        .id();
    let mission = world
        .spawn((ChildOf(top), UiNode::new(UiStyle::flex().with_gap(1.0))))
        .id();
    world.spawn((
        ChildOf(mission),
        UiRuntimeKey::new(DOMAIN, "mission-title")?,
        text("NIGHT RELAY", 28.0, MINT)?,
    ));
    world.spawn((
        ChildOf(mission),
        Label::Objective,
        UiRuntimeKey::new(DOMAIN, "objective")?,
        text("", 15.0, SECONDARY)?,
    ));
    world.spawn((
        ChildOf(top),
        Label::Vitals,
        UiRuntimeKey::new(DOMAIN, "vitals")?,
        text("", 19.0, LIGHT)?,
    ));

    world.spawn((
        ChildOf(root),
        UiRuntimeKey::new(DOMAIN, "controls")?,
        plate.clone(),
        UiNode::new(UiStyle::flex().with_padding(UiEdges::symmetric(16.0, 10.0)?)),
        text(
            "WASD / ARROWS   Fly               R / ENTER   Restart               ESC   Quit",
            16.0,
            SECONDARY,
        )?
        .with_alignment(UiTextAlignment::Center),
    ));
    let result = world
        .spawn((
            ChildOf(root),
            ResultPanel,
            plate,
            UiNode::new(
                UiStyle::absolute(0.0, 0.0, 0.0, 132.0)
                    .with_left(UiDimension::Percent(0.26))
                    .with_top(UiDimension::Percent(0.41))
                    .with_width(UiDimension::Percent(0.48))
                    .with_justify_content(UiJustify::Center)
                    .with_gap(7.0)
                    .with_z_index(1)
                    .with_visible(false),
            ),
        ))
        .id();
    world.spawn((
        ChildOf(result),
        Label::Result,
        UiRuntimeKey::new(DOMAIN, "result-title")?,
        text("", 31.0, MINT)?.with_alignment(UiTextAlignment::Center),
    ));
    world.spawn((
        ChildOf(result),
        Label::Reason,
        UiRuntimeKey::new(DOMAIN, "result-reason")?,
        text("", 16.0, LIGHT)?.with_alignment(UiTextAlignment::Center),
    ));
    Ok(())
}

fn text(content: &str, size: f32, color: UiColor) -> Result<UiText, nara::ui::UiStyleError> {
    Ok(UiText::new(content, size)?.with_brush(UiBrush::Solid(color)))
}

pub(super) fn update(
    round: Res<Round>,
    campaign: Res<Campaign>,
    mut labels: Query<(&Label, &mut UiText)>,
    mut panel: Single<&mut UiNode, With<ResultPanel>>,
    mut previous: Local<Option<Readout>>,
) {
    let remaining = round.remaining();
    let readout = Readout {
        sector: campaign.sector,
        entering_sector: campaign.pending.is_some(),
        status: round.status(),
        cells: round.collected(),
        shields: round.shields(),
        seconds: remaining.as_secs() + u64::from(remaining.subsec_nanos() != 0),
    };
    if *previous == Some(readout) {
        return;
    }
    *previous = Some(readout);

    for (label, mut text) in &mut labels {
        let next = match label {
            Label::Vitals => text
                .clone()
                .with_content(format!(
                    "TIME  {:02}:{:02}      CELLS  {}/{}      SHIELDS  {}",
                    readout.seconds / 60,
                    readout.seconds % 60,
                    readout.cells,
                    arena::CELL_COUNT,
                    readout.shields,
                ))
                .with_brush(UiBrush::Solid(
                    if readout.seconds <= 8 || readout.shields <= 1 {
                        AMBER
                    } else {
                        LIGHT
                    },
                )),
            Label::Objective => {
                text.clone()
                    .with_content(match (readout.sector, round.can_extract()) {
                        (Sector::One, false) => "Collect all three amber cells to power the exit.",
                        (Sector::One, true) => "Gate powered. Fly to the exit.",
                        (Sector::Two, false) => "Sector 2 - Collect all three amber cells.",
                        (Sector::Two, true) => "Sector 2 - Gate powered. Fly to the exit.",
                    })
            }
            Label::Result => {
                let (caption, color) = match readout.status {
                    RoundStatus::Running => ("", LIGHT),
                    RoundStatus::Escaped => ("EXTRACTION COMPLETE", MINT),
                    RoundStatus::Failed(_) => ("SIGNAL LOST", AMBER),
                };
                text.clone()
                    .with_content(caption)
                    .with_brush(UiBrush::Solid(color))
            }
            Label::Reason => text.clone().with_content(match readout.status {
                RoundStatus::Running => "",
                RoundStatus::Escaped if readout.entering_sector => "Entering Sector 2…",
                RoundStatus::Escaped => "All three cells recovered. Press R or Enter to fly again.",
                RoundStatus::Failed(Failure::TimeExpired) => {
                    "Time expired. Press R or Enter to retry."
                }
                RoundStatus::Failed(Failure::ShieldDepleted) => {
                    "Shields depleted. Press R or Enter to retry."
                }
            }),
        };
        if *text != next {
            *text = next;
        }
    }
    let visible = readout.status != RoundStatus::Running;
    let style = panel.style();
    if style.visible() != visible {
        **panel = UiNode::new(style.with_visible(visible));
    }
}
