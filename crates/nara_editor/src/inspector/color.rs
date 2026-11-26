use nara_reflect::ComponentValue;
use open_gpui::Rgba;

const CHANNELS: [&str; 4] = ["r", "g", "b", "a"];

#[derive(Clone, Copy)]
pub(super) struct ColorSeed(Option<[f64; 4]>);

impl ColorSeed {
    pub fn admit(
        values: &[Option<ComponentValue>],
        shared: Option<&ComponentValue>,
    ) -> Option<Self> {
        values
            .iter()
            .flatten()
            .all(|value| unit_channels(value).is_some())
            .then(|| Self(shared.and_then(unit_channels)))
    }

    pub fn display(self) -> Rgba {
        let [r, g, b, a] = self.0.map_or([0.5, 0.5, 0.5, 1.], |[r, g, b, a]| {
            [encode_srgb(r), encode_srgb(g), encode_srgb(b), a as f32]
        });
        Rgba { r, g, b, a }
    }

    pub fn value(self, color: Rgba) -> Result<ComponentValue, String> {
        let channels = [color.r, color.g, color.b, color.a];
        if !channels.iter().all(|channel| (0. ..=1.).contains(channel)) {
            return Err("The sRGB picker requires channels in the 0–1 range.".into());
        }
        let displayed = self.display();
        let original_display = [displayed.r, displayed.g, displayed.b, displayed.a];
        CHANNELS
            .into_iter()
            .zip(channels)
            .enumerate()
            .map(|(index, (name, channel))| {
                let linear = match self.0 {
                    Some(original) if channel == original_display[index] => original[index],
                    _ if name == "a" => f64::from(channel),
                    _ => decode_srgb(f64::from(channel)),
                };
                ComponentValue::f64(linear).map(|value| (name.to_owned(), value))
            })
            .collect::<Result<_, _>>()
            .map(ComponentValue::Map)
            .map_err(|error| error.to_string())
    }
}

fn unit_channels(value: &ComponentValue) -> Option<[f64; 4]> {
    let channel = |name| {
        value
            .get(name)?
            .as_f64()
            .filter(|channel| (0. ..=1.).contains(channel))
    };
    Some([channel("r")?, channel("g")?, channel("b")?, channel("a")?])
}

fn encode_srgb(value: f64) -> f32 {
    (if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }) as f32
}

fn decode_srgb(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
