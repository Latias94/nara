use nara_reflect::{
    ComponentFieldId, ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyStep,
};

#[test]
fn invalid_selector_identity_errors_do_not_retain_or_display_the_input() {
    let private = "private-invalid-identity".repeat(8192);
    for property in [
        ComponentPropertyPath::new(ComponentFieldId::new(&private)),
        ComponentPropertyPath::new(ComponentFieldId::new("root")).with_steps([
            ComponentPropertyStep::Member(ComponentFieldId::new(&private)),
        ]),
        ComponentPropertyPath::new(ComponentFieldId::new("root")).with_steps([
            ComponentPropertyStep::ListItem {
                by: ComponentFieldId::new(&private),
                key: ComponentPropertyKey::U64(1),
            },
        ]),
    ] {
        let error = property.logical_bytes().unwrap_err();
        assert_eq!(error, ComponentPropertyError::InvalidFieldId);
        assert!(!format!("{error:?}").contains("private-invalid-identity"));
        assert!(error.to_string().len() < 128);
    }
}
