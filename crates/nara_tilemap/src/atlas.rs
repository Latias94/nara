//! Exact atlas-to-image extent agreement shared by source and processed content.

use nara_image::ImageExtent;

use crate::TileAtlasLayout;

/// The caller validates the layout before checking its exact Image extent.
pub(crate) fn matches_image_extent(atlas: TileAtlasLayout, image_extent: ImageExtent) -> bool {
    let atlas_extent = atlas.atlas_size();
    exact_atlas_axis_matches(
        atlas.tile_size.x,
        atlas.columns,
        atlas.margin.x,
        atlas.spacing.x,
        image_extent.width,
    ) && exact_atlas_axis_matches(
        atlas.tile_size.y,
        atlas.rows,
        atlas.margin.y,
        atlas.spacing.y,
        image_extent.height,
    ) && f64::from(atlas_extent.x) == f64::from(image_extent.width)
        && f64::from(atlas_extent.y) == f64::from(image_extent.height)
}

fn exact_atlas_axis_matches(
    tile_size: f32,
    count: u32,
    margin: f32,
    spacing: f32,
    extent: u32,
) -> bool {
    let tile_size = f64::from(tile_size);
    let count = f64::from(count);
    let spacing = f64::from(spacing);
    let tiles = tile_size * count;
    let gaps = spacing * (count - 1.0);
    let terms = [
        tiles,
        tile_size.mul_add(count, -tiles),
        f64::from(margin) * 2.0,
        gaps,
        spacing.mul_add(count - 1.0, -gaps),
        -f64::from(extent),
    ];
    let mut expansion = [0.0; 6];
    let mut length = 0;
    for term in terms {
        let mut accumulator = term;
        let mut next_length = 0;
        for index in 0..length {
            let component = expansion[index];
            let sum = accumulator + component;
            let component_in_sum = sum - accumulator;
            let residual =
                (accumulator - (sum - component_in_sum)) + (component - component_in_sum);
            if residual != 0.0 {
                expansion[next_length] = residual;
                next_length += 1;
            }
            accumulator = sum;
        }
        if accumulator != 0.0 {
            expansion[next_length] = accumulator;
            next_length += 1;
        }
        length = next_length;
    }
    length == 0
}
