use nara_ui_kernel::{UiBrush, UiEdges};
use nara_ui_widgets::{UiAppearance, UiAppearanceError, UiTextField, UiTextFieldAppearance};

pub(super) fn field_appearance(
    appearance: UiAppearance,
    text: &UiTextField,
    width: f32,
    invalid: bool,
) -> Result<UiTextFieldAppearance, UiAppearanceError> {
    let mut field = appearance.field(text.input_token().is_some(), text.enabled(), invalid)?;
    if invalid {
        field.visual = field
            .visual
            .with_background(UiBrush::Solid(appearance.palette().input));
    }
    let padding = field.padding;
    if width <= padding.left() + padding.right() {
        let inline = (width - 1.0) * 0.5;
        field.padding = UiEdges::new(inline, inline, padding.top(), padding.bottom())?;
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use nara_ui_kernel::{UiColor, UiRootKey};

    use super::*;

    #[test]
    fn editor_error_fields_keep_readable_text_and_independent_focus_or_error_borders() {
        let appearance = crate::frontend::drawn::editor_appearance().unwrap();
        let root = UiRootKey::runtime("editor.appearance", "error-field").unwrap();
        let mut engine = nara_text::TextEngine::with_fonts(
            vec![nara_text::bundled_regular_font_face().unwrap()],
            Default::default(),
        )
        .unwrap();
        let mut input = UiTextField::new(root, "Value", "invalid", 14.0, &mut engine).unwrap();
        let luminance = |color: UiColor| {
            let [red, green, blue, _] = color.components();
            0.2126 * red + 0.7152 * green + 0.0722 * blue
        };
        for focused in [false, true] {
            if focused {
                input.focus().unwrap();
            }
            let field = field_appearance(appearance, &input, 240.0, true).unwrap();
            let UiBrush::Solid(foreground) = field.foreground else {
                panic!("the Editor field must resolve its foreground")
            };
            let Some(UiBrush::Solid(background)) = field.visual.background() else {
                panic!("the Editor field must resolve its background")
            };
            let foreground = luminance(foreground);
            let background = luminance(*background);
            let contrast =
                (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
            assert!(
                contrast >= 4.5,
                "invalid-field text contrast is {contrast}:1"
            );
            assert_eq!(
                field.visual.border().unwrap().brush(),
                &UiBrush::Solid(if focused {
                    appearance.palette().focus
                } else {
                    appearance.palette().danger
                }),
            );
        }
    }
}
