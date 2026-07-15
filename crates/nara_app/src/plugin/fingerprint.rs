use std::{
    collections::BTreeSet,
    fmt::{self, Debug, Formatter},
};

use super::{
    PluginDeclaration, PluginSlotId,
    definition::{PluginConfigurationFingerprint, PluginDefinition},
    group::{PluginSlot, ResolvedEditTarget, ResolvedPluginEdit},
    resolve::{PluginPlanEntry, ResolvedPluginGroup},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginPlanFingerprint(pub(super) [u8; 32]);

impl PluginPlanFingerprint {
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl Debug for PluginPlanFingerprint {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

pub(super) fn write_digest(formatter: &mut Formatter<'_>, digest: &[u8; 32]) -> fmt::Result {
    fmt::Display::fmt(&blake3::Hash::from_bytes(*digest), formatter)
}

pub(super) struct FingerprintEncoder {
    hasher: blake3::Hasher,
}

impl FingerprintEncoder {
    pub(super) fn new(domain: &'static [u8]) -> Self {
        let mut encoder = Self {
            hasher: blake3::Hasher::new(),
        };
        encoder.bytes(b"domain", domain);
        encoder
    }

    pub(super) fn bytes(&mut self, tag: &'static [u8], value: &[u8]) {
        self.hasher.update(&(tag.len() as u64).to_le_bytes());
        self.hasher.update(tag);
        self.hasher.update(&(value.len() as u64).to_le_bytes());
        self.hasher.update(value);
    }

    pub(super) fn string(&mut self, tag: &'static [u8], value: &str) {
        self.bytes(tag, value.as_bytes());
    }

    pub(super) fn u32(&mut self, tag: &'static [u8], value: u32) {
        self.bytes(tag, &value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, tag: &'static [u8], value: u64) {
        self.bytes(tag, &value.to_le_bytes());
    }

    pub(super) fn digest(&mut self, tag: &'static [u8], value: &[u8; 32]) {
        self.bytes(tag, value);
    }

    pub(super) fn finish(self) -> PluginPlanFingerprint {
        PluginPlanFingerprint(*self.hasher.finalize().as_bytes())
    }

    pub(super) fn finish_configuration(self) -> PluginConfigurationFingerprint {
        PluginConfigurationFingerprint(*self.hasher.finalize().as_bytes())
    }
}

pub(super) fn fingerprint_plan(
    entries: &[PluginPlanEntry],
    groups: &[ResolvedPluginGroup],
    disabled_slots: &BTreeSet<PluginSlotId>,
) -> PluginPlanFingerprint {
    let mut encoder = FingerprintEncoder::new(b"nara.plugin-plan.v2");
    encoder.u64(b"entry-count", entries.len() as u64);
    for entry in entries {
        encoder.bytes(b"entry", b"begin");
        encode_declaration(&mut encoder, entry.declaration);
        if let Some(key) = entry.definition_key {
            encoder.bytes(b"definition-kind", b"repeatable");
            encoder.string(b"definition-id", key.definition.id);
            encoder.u32(b"definition-version", key.definition.version);
            encoder.digest(b"configuration", &key.configuration.0);
        } else {
            encoder.bytes(b"definition-kind", b"opaque-direct-instance");
        }
        if let Some(slot) = entry.slot {
            encoder.bytes(b"slot-kind", b"present");
            encode_slot(&mut encoder, slot);
        } else {
            encoder.bytes(b"slot-kind", b"absent");
        }
        encoder.u64(b"provenance-count", entry.group_provenance.len() as u64);
        for group in &entry.group_provenance {
            encoder.string(b"provenance", group.as_str());
        }
        encoder.bytes(b"entry", b"end");
    }
    encoder.u64(b"group-count", groups.len() as u64);
    for group in groups {
        encoder.string(b"group-id", group.id.as_str());
        encoder.digest(b"group-fingerprint", &group.definition_fingerprint.0);
    }
    encoder.u64(b"disabled-slot-count", disabled_slots.len() as u64);
    for slot in disabled_slots {
        encoder.string(b"disabled-slot", slot.as_str());
    }
    encoder.finish()
}

fn encode_slot(encoder: &mut FingerprintEncoder, slot: PluginSlot) {
    encoder.string(b"slot-id", slot.id.as_str());
    encoder.string(b"slot-plugin", slot.expected_plugin.as_str());
    encoder.bytes(b"slot-presence", &[slot.presence as u8]);
}

pub(super) fn encode_definition(
    encoder: &mut FingerprintEncoder,
    definition: &PluginDefinition,
    slot: Option<PluginSlot>,
) {
    let key = definition.resolved_key();
    encode_declaration(encoder, definition.resolved_declaration());
    encoder.string(b"definition-id", key.definition.id);
    encoder.u32(b"definition-version", key.definition.version);
    encoder.digest(b"configuration", &key.configuration.0);
    encoder.bytes(
        b"canonical-configuration",
        &definition.canonical_configuration,
    );
    if let Some(slot) = slot {
        encoder.bytes(b"slot-kind", b"present");
        encode_slot(encoder, slot);
    } else {
        encoder.bytes(b"slot-kind", b"absent");
    }
}

pub(super) fn encode_edit(encoder: &mut FingerprintEncoder, edit: &ResolvedPluginEdit) {
    match edit {
        ResolvedPluginEdit::Disable(target) => {
            encoder.bytes(b"edit-kind", b"disable");
            encode_edit_target(encoder, *target);
        }
        ResolvedPluginEdit::Configure(target, definition) => {
            encoder.bytes(b"edit-kind", b"configure");
            encode_edit_target(encoder, *target);
            encode_definition(encoder, definition, None);
        }
        ResolvedPluginEdit::InsertAfter(target, definition) => {
            encoder.bytes(b"edit-kind", b"insert-after");
            encode_edit_target(encoder, *target);
            encode_definition(encoder, definition, None);
        }
        ResolvedPluginEdit::InsertBefore(target, definition) => {
            encoder.bytes(b"edit-kind", b"insert-before");
            encode_edit_target(encoder, *target);
            encode_definition(encoder, definition, None);
        }
    }
}

fn encode_edit_target(encoder: &mut FingerprintEncoder, target: ResolvedEditTarget) {
    match target {
        ResolvedEditTarget::Plugin(plugin) => {
            encoder.bytes(b"target-kind", b"plugin");
            encoder.string(b"target-plugin", plugin.as_str());
        }
        ResolvedEditTarget::Slot(slot) => {
            encoder.bytes(b"target-kind", b"slot");
            encoder.string(b"target-slot", slot.as_str());
        }
    }
}

fn encode_declaration(encoder: &mut FingerprintEncoder, declaration: &PluginDeclaration) {
    encoder.string(b"plugin-id", declaration.id.as_str());
    encoder.bytes(b"plugin-category", &[declaration.category as u8]);
    encode_id_list(
        encoder,
        b"provides-count",
        b"provides",
        declaration.provides.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"requires-plugin-count",
        b"requires-plugin",
        declaration.requires_plugins.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"requires-capability-count",
        b"requires-capability",
        declaration
            .requires_capabilities
            .iter()
            .map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"conflict-count",
        b"conflict",
        declaration.conflicts.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"provides-service-count",
        b"provides-service",
        declaration.provides_services.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"requires-service-count",
        b"requires-service",
        declaration.requires_services.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"requires-product-count",
        b"requires-product",
        declaration
            .requires_product_capabilities
            .iter()
            .map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"provides-schema-count",
        b"provides-schema",
        declaration.provides_schema.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"requires-schema-count",
        b"requires-schema",
        declaration.requires_schema.iter().map(|id| id.as_str()),
    );
    encode_id_list(
        encoder,
        b"shutdown-obligation-count",
        b"shutdown-obligation",
        declaration
            .shutdown_obligations
            .iter()
            .map(|id| id.as_str()),
    );
}

fn encode_id_list<'a>(
    encoder: &mut FingerprintEncoder,
    count_tag: &'static [u8],
    item_tag: &'static [u8],
    ids: impl ExactSizeIterator<Item = &'a str>,
) {
    encoder.u64(count_tag, ids.len() as u64);
    for id in ids {
        encoder.string(item_tag, id);
    }
}

pub(crate) fn empty_plan_fingerprint() -> PluginPlanFingerprint {
    fingerprint_plan(&[], &[], &BTreeSet::new())
}
