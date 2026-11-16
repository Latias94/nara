use super::*;

#[test]
fn payload_storage_preserves_the_component_value_wire() {
    for value in [ComponentValue::Null, value()] {
        let payload = ComponentPayload::from(value.clone());
        let json = serde_json::to_string(&value).unwrap();
        let ron = ron::to_string(&value).unwrap();
        assert_eq!(serde_json::to_string(&payload).unwrap(), json);
        assert_eq!(ron::to_string(&payload).unwrap(), ron);
        assert_eq!(
            serde_json::from_str::<ComponentPayload>(&json)
                .unwrap()
                .into_owned(),
            value,
        );
        assert_eq!(
            ron::from_str::<ComponentPayload>(&ron)
                .unwrap()
                .into_owned(),
            value,
        );
    }
}

#[test]
fn source_record_preserves_existing_json_and_ron_fields() {
    for (json, ron) in [
        (
            r#"{"version":1,"value":{"type":"null"}}"#,
            r#"(version:1,value:(type:"null"))"#,
        ),
        (
            concat!(
                r#"{"owner":"test.payload","version":1,"value":{"type":"map","value":{"items":{"type":"list","value":["#,
                r#"{"type":"string","value":"hello"},{"type":"u64","value":7}]}}},"contextual_defaults":{"hidden":{"type":"string","value":"default"}}}"#,
            ),
            concat!(
                r#"(owner:Some("test.payload"),version:1,value:(type:"map",value:{"items":(type:"list",value:["#,
                r#"(type:"string",value:"hello"),(type:"u64",value:7)])}),contextual_defaults:{"hidden":(type:"string",value:"default")})"#,
            ),
        ),
    ] {
        let from_json: SceneComponentRecord = serde_json::from_str(json).unwrap();
        let from_ron: SceneComponentRecord = ron::from_str(ron).unwrap();
        assert_eq!(from_json, from_ron);
        assert_eq!(serde_json::to_string(&from_json).unwrap(), json);
        assert_eq!(ron::to_string(&from_json).unwrap(), ron);
    }
}
