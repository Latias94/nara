//! Independent text-resource domain: typed settings, parsing and an auxiliary source dependency.

use nara::{
    asset::AssetPath,
    core::ByteLimit,
    diagnostic::{Diagnostic, DiagnosticCode, SafeSummary},
    project_host::{
        ProjectAssetDomain, ProjectAssetLimits, ProjectAssetSource, ProjectContentError,
    },
};
use serde::{Deserialize, Serialize};

pub struct TextDomain;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextSettings {
    pub max_characters: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TextResource {
    pub text: String,
}

impl ProjectAssetDomain for TextDomain {
    type Product = TextResource;
    const SOURCE_KIND: &'static str = "consumer.text";

    fn limits() -> ProjectAssetLimits {
        let bytes = ByteLimit::new(4096).unwrap();
        ProjectAssetLimits::new(
            bytes,
            ByteLimit::new(16 * 1024).unwrap(),
            ByteLimit::new(8 * 1024).unwrap(),
        )
    }

    fn prepare(source: &mut ProjectAssetSource<'_>) -> Result<TextResource, ProjectContentError> {
        let metadata = source
            .metadata()
            .admit::<TextSettings>()
            .map_err(|_| invalid_settings())?;
        let maximum = metadata.into_meta().settings().max_characters;
        if maximum == 0 || maximum > 4096 {
            return Err(invalid_settings());
        }
        // The first line names one required auxiliary file; it is data, not another decoder.
        let encoded = std::str::from_utf8(source.bytes()).map_err(|_| invalid_source())?;
        let (dependency, body) = encoded.split_once('\n').ok_or_else(invalid_source)?;
        let dependency = AssetPath::new(dependency).map_err(|_| invalid_source())?;
        let mut text = body.to_owned();
        let suffix = source.read_dependency(&dependency, ByteLimit::new(1024).unwrap())?;
        // Repeated reads prove the public reader preserves one immutable observation.
        let repeated = source.read_dependency(&dependency, ByteLimit::new(1024).unwrap())?;
        assert!(std::sync::Arc::ptr_eq(&suffix, &repeated));
        text.push_str(std::str::from_utf8(&suffix).map_err(|_| invalid_source())?);
        if text.chars().count() > maximum {
            return Err(ProjectContentError::asset_domain(Diagnostic::error(
                DiagnosticCode::new("consumer.text.too-long").unwrap(),
                SafeSummary::new(
                    "Text exceeds max_characters; shorten the source or increase its setting",
                )
                .unwrap(),
            )));
        }
        Ok(TextResource { text })
    }
}

fn invalid_settings() -> ProjectContentError {
    ProjectContentError::asset_domain(Diagnostic::error(
        DiagnosticCode::new("consumer.text.settings-invalid").unwrap(),
        SafeSummary::new("Set max_characters between 1 and 4096 in the text metadata").unwrap(),
    ))
}

fn invalid_source() -> ProjectContentError {
    ProjectContentError::asset_domain(Diagnostic::error(
        DiagnosticCode::new("consumer.text.source-invalid").unwrap(),
        SafeSummary::new("Use UTF-8 text with an auxiliary asset path on its first line").unwrap(),
    ))
}
