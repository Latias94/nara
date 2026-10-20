use std::{ffi::OsString, path::PathBuf};

use serde_json::{Value, json};

use super::{ProjectToolRequest, ProjectToolSummary, ToolError};

pub(super) const TOOL_ARGUMENT: &str = "--nara-project-tool";
pub(super) const PROTOCOL_VERSION: &str = "2";
pub(super) const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
pub(super) const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;
const MAX_ARGUMENT_BYTES: usize = 128 * 1024;

pub(super) fn parse_request(
    arguments: impl IntoIterator<Item = OsString>,
) -> Option<Result<ProjectToolRequest, &'static str>> {
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(TOOL_ARGUMENT)) {
        return None;
    }
    Some(parse_selected(arguments))
}

fn parse_selected(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<ProjectToolRequest, &'static str> {
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(PROTOCOL_VERSION)) {
        return Err(
            "unsupported compiled-project tool protocol version; update the project headless entry and CLI together",
        );
    }
    let operation = arguments.next().ok_or("missing project tool operation")?;
    let mut project = None;
    let mut profile = None;
    let mut parent = None;
    let mut destination = None;
    let mut replacement = None;
    let mut count = 0;
    while let Some(flag) = arguments.next() {
        count += 1;
        let value = arguments
            .next()
            .ok_or("missing project tool option value")?;
        if count > 5 || value.len() > MAX_ARGUMENT_BYTES {
            return Err("project tool request exceeds its bounded size");
        }
        match flag.to_str() {
            Some("--project-root") if project.is_none() => project = Some(PathBuf::from(value)),
            Some("--profile") if profile.is_none() => {
                profile = Some(value.into_string().map_err(|_| "profile must be Unicode")?)
            }
            Some("--parent") if parent.is_none() => parent = Some(PathBuf::from(value)),
            Some("--destination") if destination.is_none() => destination = Some(value),
            Some("--replacement") if replacement.is_none() => {
                replacement = Some(
                    value
                        .into_string()
                        .map_err(|_| "replacement must be Unicode")?,
                )
            }
            _ => return Err("unknown or repeated project tool option"),
        }
    }
    let project = project
        .filter(|path| path.is_absolute())
        .ok_or("project tool requires an explicit absolute project root")?;
    match operation.to_str() {
        Some("check") if parent.is_none() && destination.is_none() && replacement.is_none() => {
            Ok(ProjectToolRequest::Check { project, profile })
        }
        Some("schema-check" | "schema-write")
            if parent.is_none() && destination.is_none() && replacement.is_none() =>
        {
            Ok(ProjectToolRequest::Schema {
                project,
                profile,
                write: operation.to_str() == Some("schema-write"),
            })
        }
        Some("export") if profile.is_none() && replacement.is_none() => {
            Ok(ProjectToolRequest::Export {
                project,
                parent: parent
                    .filter(|path| path.is_absolute())
                    .ok_or("export requires an explicit absolute parent")?,
                destination: destination.ok_or("export requires a destination name")?,
            })
        }
        Some("repair-startup-scene") if parent.is_none() && destination.is_none() => {
            Ok(ProjectToolRequest::RepairStartupScene {
                project,
                profile,
                replacement: replacement.ok_or("repair requires a replacement Scene")?,
            })
        }
        _ => Err("unsupported project tool operation or options"),
    }
}

pub(super) fn encode_response(
    operation: &str,
    result: Result<ProjectToolSummary, ToolError>,
) -> Value {
    match result {
        Ok(ProjectToolSummary::Checked { content_members }) => {
            json!({"version":2,"operation":operation,"success":true,"content_members":content_members})
        }
        Ok(ProjectToolSummary::Schema { written }) => {
            json!({"version":2,"operation":operation,"success":true,"written":written})
        }
        Ok(ProjectToolSummary::Exported {
            content_members,
            product_observations,
        }) => {
            json!({"version":2,"operation":operation,"success":true,"content_members":content_members,"product_observations":product_observations})
        }
        Ok(ProjectToolSummary::Repaired { replacement }) => {
            json!({"version":2,"operation":operation,"success":true,"replacement":replacement})
        }
        Err(error) => {
            let mut diagnostic = error.diagnostic();
            if diagnostic.len() > MAX_DIAGNOSTIC_BYTES {
                let mut end = MAX_DIAGNOSTIC_BYTES;
                while !diagnostic.is_char_boundary(end) {
                    end -= 1;
                }
                diagnostic.truncate(end);
            }
            json!({"version":2,"operation":operation,"success":false,"diagnostic":diagnostic})
        }
    }
}

pub(super) fn decode_response(
    bytes: &[u8],
    operation: &str,
) -> Result<Result<ProjectToolSummary, String>, &'static str> {
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("project tool response exceeds its bounded size");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "project headless executable did not return a project tool response; update its tool-mode entry")?;
    if value["version"].as_u64() != Some(2) || value["operation"].as_str() != Some(operation) {
        return Err("project tool response version or operation does not match the request");
    }
    match value["success"].as_bool() {
        Some(false) => {
            let diagnostic = value["diagnostic"]
                .as_str()
                .filter(|value| value.len() <= MAX_DIAGNOSTIC_BYTES)
                .ok_or("invalid project tool diagnostic")?;
            Ok(Err(diagnostic.to_owned()))
        }
        Some(true) => {
            let count = || {
                value["content_members"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or("invalid project tool member count")
            };
            let summary = match operation {
                "check" => ProjectToolSummary::Checked {
                    content_members: count()?,
                },
                "schema-check" | "schema-write" => ProjectToolSummary::Schema {
                    written: value["written"]
                        .as_bool()
                        .filter(|written| operation == "schema-write" || !*written)
                        .ok_or("invalid project tool schema publication outcome")?,
                },
                "export" => ProjectToolSummary::Exported {
                    content_members: count()?,
                    product_observations: value["product_observations"]
                        .as_u64()
                        .filter(|value| *value == 2)
                        .ok_or("invalid project tool product count")?
                        as usize,
                },
                "repair-startup-scene" => ProjectToolSummary::Repaired {
                    replacement: value["replacement"]
                        .as_str()
                        .filter(|value| value.len() <= MAX_ARGUMENT_BYTES)
                        .ok_or("invalid project tool replacement")?
                        .to_owned(),
                },
                _ => return Err("invalid project tool response operation"),
            };
            Ok(Ok(summary))
        }
        None => Err("invalid project tool response outcome"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_game_arguments_do_not_select_tool_mode() {
        assert!(parse_request([OsString::from("--max-ticks"), OsString::from("1")]).is_none());
    }

    #[test]
    fn unsupported_version_is_an_explicit_error() {
        assert!(
            parse_request([OsString::from(TOOL_ARGUMENT), OsString::from("999")])
                .unwrap()
                .is_err()
        );
        assert!(
            parse_request([OsString::from(TOOL_ARGUMENT), OsString::from("1")])
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn delivery_counts_require_the_current_tool_protocol() {
        let summary = ProjectToolSummary::Checked {
            content_members: 11,
        };
        let response = encode_response("check", Ok(summary.clone()));
        assert_eq!(response["content_members"], 11);
        assert!(response.get("startup_members").is_none());
        assert_eq!(
            decode_response(&serde_json::to_vec(&response).unwrap(), "check"),
            Ok(Ok(summary))
        );
        let legacy = br#"{"version":1,"operation":"check","success":true,"startup_members":11}"#;
        assert!(decode_response(legacy, "check").is_err());
    }

    #[test]
    fn gameplay_output_is_not_tool_agreement() {
        assert!(decode_response(b"starter_headless: ok fixed_ticks=1\n", "check").is_err());
    }

    #[test]
    fn schema_operations_preserve_mode_profile_and_response() {
        for (operation, write) in [("schema-check", false), ("schema-write", true)] {
            let project = std::env::current_dir().unwrap();
            let request = parse_request([
                OsString::from(TOOL_ARGUMENT),
                OsString::from(PROTOCOL_VERSION),
                OsString::from(operation),
                OsString::from("--project-root"),
                project.clone().into_os_string(),
                OsString::from("--profile"),
                OsString::from("desktop"),
            ])
            .unwrap()
            .unwrap();
            assert!(matches!(request, ProjectToolRequest::Schema {
                project: actual,
                profile: Some(profile),
                write: actual_write,
            } if actual == project && profile == "desktop" && actual_write == write));
            let summary = ProjectToolSummary::Schema { written: write };
            let response = encode_response(operation, Ok(summary.clone()));
            assert_eq!(
                decode_response(&serde_json::to_vec(&response).unwrap(), operation),
                Ok(Ok(summary))
            );
        }
    }

    #[test]
    fn unsupported_operations_and_schema_mutation_claims_are_rejected() {
        let project = std::env::current_dir().unwrap();
        for operation in ["schema", "schema-delete", "run-untrusted"] {
            assert!(
                parse_request([
                    OsString::from(TOOL_ARGUMENT),
                    OsString::from(PROTOCOL_VERSION),
                    OsString::from(operation),
                    OsString::from("--project-root"),
                    project.clone().into_os_string(),
                ])
                .unwrap()
                .is_err()
            );
        }
        let response = encode_response(
            "schema-check",
            Ok(ProjectToolSummary::Schema { written: true }),
        );
        assert!(decode_response(&serde_json::to_vec(&response).unwrap(), "schema-check").is_err());
    }
}
