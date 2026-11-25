//! Wire contract between project tooling and a project-owned cooker.

use serde::{Deserialize, Serialize};

pub const DELIVERY_ARGUMENT: &str = "--nara-delivery";
pub const DELIVERY_PROTOCOL_VERSION: &str = "1";
pub const DELIVERY_RECEIPT_PREFIX: &str = "NARA_DELIVERY_V1 ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOperation {
    Cook,
    Export,
}

impl DeliveryOperation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cook => "cook",
            Self::Export => "export",
        }
    }
}

/// The cooker's JSON response after publishing the requested directory.
/// Export supplies the executable's filename. Cook leaves it unset.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CookerReceipt {
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub postpublication_error: Option<String>,
}
