use nara_core::source::StableAssetId;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePayload {
    source: StableAssetId,
    contexts: Vec<SourceContext>,
    bindings: Vec<ActionBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceContext {
    context: ActionContext,
    enabled: bool,
}

impl ActionMapFileCandidate {
    /// Reads the authored format selected by the filename in the project manifest.
    pub fn decode_source_bytes(
        source_path: &str,
        bytes: &[u8],
        limits: ActionMapFileLimits,
    ) -> Result<Self, ActionMapFileError> {
        if source_path.ends_with(".ron") {
            Self::decode_ron_bytes_with_limits(bytes, limits)
        } else if source_path.ends_with(".json") {
            Self::decode_json_bytes_with_limits(bytes, limits)
        } else {
            Err(ActionMapFileError::UnsupportedSourceExtension)
        }
    }

    pub fn decode_ron_bytes(bytes: &[u8]) -> Result<Self, ActionMapFileError> {
        Self::decode_ron_bytes_with_limits(bytes, ActionMapFileLimits::default())
    }

    pub fn decode_ron_bytes_with_limits(
        bytes: &[u8],
        limits: ActionMapFileLimits,
    ) -> Result<Self, ActionMapFileError> {
        let contract = action_map_contract(ACTION_MAP_FORMAT_KIND)?;
        let envelope = decode_persistent_file_with_preflight(
            bytes,
            limits.encoded_bytes(),
            &contract,
            |bytes| validate_shape(bytes, limits.shape()),
            |bytes| {
                ron::de::from_bytes::<PersistentFileHeader>(bytes)
                    .map_err(ActionMapFileError::RonParse)
            },
            |bytes| {
                ron::de::from_bytes::<PersistentFileEnvelope<SourcePayload>>(bytes)
                    .map_err(ActionMapFileError::RonParse)
            },
        )
        .map_err(map_decode_error)?;
        let payload = envelope.into_payload();
        let product = AssetProductRef::new(payload.source, ImportedProductId::PRIMARY)
            .map_err(ActionMapFileError::SourceId)?;
        let mut builder = ActionMapBuilder::new(limits.definition());
        for entry in payload.contexts {
            builder
                .declare_context(entry.context, entry.enabled)
                .map_err(ActionMapFileError::Definition)?;
        }
        for binding in payload.bindings {
            builder
                .bind(binding)
                .map_err(ActionMapFileError::Definition)?;
        }
        Self::new(
            product,
            builder.compile().map_err(ActionMapFileError::Definition)?,
        )
    }

    pub fn to_ron_string(&self) -> Result<String, ActionMapFileError> {
        self.to_ron_string_with_limits(ActionMapFileLimits::default())
    }

    pub fn to_ron_string_with_limits(
        &self,
        limits: ActionMapFileLimits,
    ) -> Result<String, ActionMapFileError> {
        #[derive(Serialize)]
        struct SourcePayloadRef<'a> {
            source: StableAssetId,
            contexts: ContextsRef<'a>,
            bindings: &'a [ActionBinding],
        }

        let envelope = PersistentFileEnvelope::canonical_v1(
            FormatKind::new(ACTION_MAP_FORMAT_KIND).map_err(ActionMapFileError::FormatKind)?,
            engine_version(ENGINE_MIN_VERSION)?,
            FormatGenerator::new("nara", engine_version(env!("CARGO_PKG_VERSION"))?)
                .map_err(ActionMapFileError::Generator)?,
            SourcePayloadRef {
                source: self.product.source(),
                contexts: ContextsRef(self.definition.contexts()),
                bindings: self.definition.bindings(),
            },
        );
        let mut output = BoundedOutput {
            bytes: Vec::new(),
            maximum: limits.encoded_bytes().get(),
            exceeded: None,
        };
        ron::ser::to_writer_pretty(&mut output, &envelope, ron::ser::PrettyConfig::default())
            .map_err(|error| match output.exceeded {
                Some(observed) => ActionMapFileError::EncodedBytesExceeded {
                    observed,
                    maximum: output.maximum,
                },
                None => ActionMapFileError::RonEncode(error),
            })?;
        Self::decode_ron_bytes_with_limits(&output.bytes, limits)?;
        String::from_utf8(output.bytes).map_err(ActionMapFileError::EncodedUtf8)
    }
}

fn validate_shape(bytes: &[u8], limits: SerdeShapeLimits) -> Result<(), ActionMapFileError> {
    let mut parser =
        ron::de::Deserializer::from_bytes(bytes).map_err(ActionMapFileError::RonParse)?;
    preflight_serde_shape(&mut parser, limits).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => ActionMapFileError::Shape(error),
        SerdeShapePreflightError::Parse(error) => {
            ActionMapFileError::RonParse(parser.span_error(error))
        }
    })?;
    parser
        .end()
        .map_err(|error| ActionMapFileError::RonParse(parser.span_error(error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AxisDeadZone, KeyCode};

    const SOURCE: &str = r#"(
        kind: "action_map",
        format_version: 1,
        engine_min_version: "0.1.0",
        generator: (name: "fixture", version: "0.1.0"),
        payload: (
            source: "b23d8f3a-9770-5c1a-bf4d-81fb93171257",
            contexts: [(context: "gameplay", enabled: true)],
            bindings: [
                (action: "jump", input: Key(KeyA), context: "gameplay"),
                (action: "move.x", input: GamepadAxis(0), context: "gameplay",
                    transform: Axis(dead_zone: 0.25, inverted: false)),
            ],
        ),
    )"#;

    #[test]
    fn authored_ron_and_legacy_json_share_the_same_compiled_definition() {
        let candidate = ActionMapFileCandidate::decode_ron_bytes(SOURCE.as_bytes()).unwrap();
        let expected = [
            ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::KeyA),
            ActionBinding::gamepad_axis(
                ActionId::new("move.x").unwrap(),
                0,
                AxisDeadZone::new(0.25).unwrap(),
                false,
            ),
        ];
        assert_eq!(candidate.definition().bindings(), &expected);
        let written = candidate.to_ron_string().unwrap();
        let json = candidate.to_json_string().unwrap();
        for (path, contents) in [
            ("default.actions.ron", written),
            ("default.actions.json", json),
        ] {
            let decoded = ActionMapFileCandidate::decode_source_bytes(
                path,
                contents.as_bytes(),
                ActionMapFileLimits::default(),
            )
            .unwrap();
            assert_eq!(decoded.product(), candidate.product());
            assert_eq!(
                decoded.definition().contexts(),
                candidate.definition().contexts()
            );
            assert_eq!(decoded.definition().bindings(), &expected);
        }
    }

    #[test]
    fn ron_source_rejects_unknown_fields_trailing_data_and_budget_overruns() {
        let decode = |contents: &str| ActionMapFileCandidate::decode_ron_bytes(contents.as_bytes());
        assert!(matches!(
            decode(&SOURCE.replace("input: Key(KeyA)", "input: Key(KeyA), extra: 3")),
            Err(ActionMapFileError::RonParse(_)),
        ));
        assert!(matches!(
            decode(&format!("{SOURCE} ()")),
            Err(ActionMapFileError::RonParse(_))
        ));
        assert!(matches!(
            decode(&SOURCE.replace(
                "b23d8f3a-9770-5c1a-bf4d-81fb93171257",
                "00000000-0000-0000-0000-000000000000"
            )),
            Err(ActionMapFileError::SourceId(_)),
        ));
        let limits = ActionMapFileLimits::default().with_encoded_bytes(ByteLimit::new(16).unwrap());
        assert!(matches!(
            ActionMapFileCandidate::decode_ron_bytes_with_limits(SOURCE.as_bytes(), limits),
            Err(ActionMapFileError::EncodedBytesExceeded { maximum: 16, .. }),
        ));
    }
}
