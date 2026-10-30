/// Whether a present logical reference participates in the current consumer's preparation.
///
/// Both demands require a valid target in strict content validation. Optional/null data describes
/// reference presence separately; Deferred never means that a present target may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AssetReferenceDemand {
    Immediate,
    Deferred,
}
