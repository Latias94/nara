use nara_ui_kernel::UiColor;
use nara_ui_widgets::{UiAppearance, UiAppearancePalette, UiDensity};

pub(crate) fn editor_appearance() -> Result<UiAppearance, String> {
    UiAppearance::new(
        UiAppearancePalette {
            window: UiColor::from_srgb8(0x17, 0x1a, 0x1f),
            panel: UiColor::from_srgb8(0x1d, 0x21, 0x28),
            raised: UiColor::from_srgb8(0x25, 0x2b, 0x34),
            input: UiColor::from_srgb8(0x14, 0x17, 0x1b),
            border: UiColor::from_srgb8(0x6b, 0x75, 0x84),
            text: UiColor::from_srgb8(0xe6, 0xea, 0xf0),
            muted_text: UiColor::from_srgb8(0xa4, 0xad, 0xb9),
            disabled_text: UiColor::from_srgb8(0x69, 0x73, 0x81),
            accent: UiColor::from_srgb8(0x5e, 0xc8, 0xbf),
            on_accent: UiColor::from_srgb8(0x10, 0x25, 0x22),
            selection: UiColor::from_srgb8(0x26, 0x44, 0x42),
            focus: UiColor::from_srgb8(0x78, 0xe0, 0xd5),
            danger: UiColor::from_srgb8(0xf0, 0x8f, 0x95),
            danger_text: UiColor::from_srgb8(0xf0, 0x8f, 0x95),
            positive: UiColor::from_srgb8(0x91, 0xc9, 0x9a),
            hover: UiColor::from_srgb8(0x2a, 0x32, 0x3c),
            pressed: UiColor::from_srgb8(0x34, 0x40, 0x4b),
        },
        UiDensity::Normal,
        1.0,
    )
    .map_err(|error| error.to_string())
}
