pub(crate) const MAX_CODE_BYTES: usize = 96;
pub(crate) const MAX_SUMMARY_BYTES: usize = 1_024;

pub(crate) fn is_valid_identifier(
    value: &str,
    maximum_bytes: usize,
    reject_sensitive_shape: bool,
) -> bool {
    let Some(first) = value.bytes().next() else {
        return false;
    };
    value.len() <= maximum_bytes
        && value.is_ascii()
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && (!reject_sensitive_shape || !contains_sensitive_shape(value))
}

pub(crate) fn is_valid_summary(value: &'static str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUMMARY_BYTES
        && !value.chars().any(char::is_control)
        && !value.chars().any(is_unsafe_format_character)
        && !contains_sensitive_shape(value)
        && !resembles_absolute_path(value)
}

fn contains_sensitive_shape(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    lowercase.contains("bearer")
        || lowercase.contains("token")
        || lowercase.contains("password")
        || lowercase.contains("credential")
        || lowercase.contains("secret")
        || lowercase.contains("api_key")
        || lowercase.contains("api-key")
        || lowercase.contains("://")
}

fn resembles_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with("\\\\")
        || value.starts_with("//")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn is_unsafe_format_character(character: char) -> bool {
    matches!(
        character,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180b}'..='\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e007f}'
    )
}
