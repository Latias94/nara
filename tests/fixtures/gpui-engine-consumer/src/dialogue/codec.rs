use std::collections::{BTreeMap, btree_map::Entry};

use serde::{Deserialize, Serialize};

use super::{
    DialogueGraph, DialogueView, EditorExtensionDocumentError, Link, NODE_LIMIT, Node, NodeBody,
    NodeId, SOURCE_BYTES, error, validation,
};

const SCHEMA_VERSION: u64 = 1;

#[derive(Deserialize)]
struct Header {
    schema_version: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema_version: u64,
    entry: Option<NodeId>,
    nodes: Vec<Node>,
    links: Vec<Link>,
}

pub(super) fn encode(view: &DialogueView) -> Result<Vec<u8>, EditorExtensionDocumentError> {
    validation::validate(view)?;
    let source = Source {
        schema_version: SCHEMA_VERSION,
        entry: view.entry,
        nodes: view.nodes.values().cloned().collect(),
        links: view.links().collect(),
    };
    let bytes = serde_json::to_vec(&source).map_err(|_| {
        error(
            "dialogue.source-encode",
            "The dialogue graph could not be encoded",
        )
    })?;
    if bytes.len() > SOURCE_BYTES {
        return Err(source_limit());
    }
    Ok(bytes)
}

pub(super) fn decode(bytes: &[u8]) -> Result<DialogueGraph, EditorExtensionDocumentError> {
    if bytes.len() > SOURCE_BYTES {
        return Err(source_limit());
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| invalid_source())?;
    if header.schema_version != SCHEMA_VERSION {
        return Err(error(
            "dialogue.schema-version",
            "The dialogue graph schema version is not supported",
        ));
    }
    let source: Source = serde_json::from_slice(bytes).map_err(|_| invalid_source())?;
    if source.nodes.len() > NODE_LIMIT {
        return Err(error(
            "dialogue.node-limit",
            "The dialogue graph contains too many nodes",
        ));
    }

    let mut nodes = BTreeMap::new();
    for node in source.nodes {
        match nodes.entry(node.id) {
            Entry::Occupied(_) => {
                return Err(error(
                    "dialogue.node-duplicate",
                    "A dialogue node may appear only once",
                ));
            }
            Entry::Vacant(entry) => {
                entry.insert(node);
            }
        }
    }
    let mut links = BTreeMap::new();
    for link in source.links {
        match links.entry(link.from) {
            Entry::Occupied(_) => {
                return Err(error(
                    "dialogue.link-duplicate",
                    "A dialogue output may have only one link",
                ));
            }
            Entry::Vacant(entry) => {
                entry.insert(link.to);
            }
        }
    }
    let view = DialogueView::authored(source.entry, nodes, links);
    let _ = encode(&view)?;
    let next_node = view.nodes.keys().next_back().map_or(1, |id| id.0 + 1);
    let mut next_choice = 1;
    for node in view.nodes.values() {
        if let NodeBody::Choice { choices, .. } = node.body.as_ref() {
            for choice in choices {
                next_choice = next_choice.max(choice.id.0 + 1);
            }
        }
    }
    DialogueGraph::from_decoded(view, next_node, next_choice)
}

fn invalid_source() -> EditorExtensionDocumentError {
    error(
        "dialogue.source-invalid",
        "The dialogue graph source is not valid schema data",
    )
}

fn source_limit() -> EditorExtensionDocumentError {
    error(
        "dialogue.source-limit",
        "The dialogue graph source exceeds the supported byte limit",
    )
}
