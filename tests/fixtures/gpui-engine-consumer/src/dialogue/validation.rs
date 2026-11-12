use std::collections::BTreeSet;

use super::{
    CHOICE_LIMIT, DialogueView, EditorExtensionDocumentError, NODE_LIMIT, NodeBody, POSITION_LIMIT,
    SPEAKER_BYTES, TEXT_BYTES, error,
};

pub(super) fn validate(view: &DialogueView) -> Result<(), EditorExtensionDocumentError> {
    if view.nodes.len() > NODE_LIMIT {
        return Err(error(
            "dialogue.node-limit",
            "The dialogue graph contains too many nodes",
        ));
    }
    if view
        .entry
        .is_some_and(|entry| !view.nodes.contains_key(&entry))
    {
        return Err(error(
            "dialogue.entry-missing",
            "The dialogue entry node does not exist",
        ));
    }

    let mut choice_ids = BTreeSet::new();
    for (id, node) in &view.nodes {
        if *id != node.id {
            return Err(error(
                "dialogue.node-key",
                "A dialogue node must match its stored identifier",
            ));
        }
        if id.0 == 0 || id.0 == u64::MAX {
            return Err(error(
                "dialogue.node-id",
                "A dialogue node identifier is outside the supported range",
            ));
        }
        if node
            .position
            .iter()
            .any(|position| !position.is_finite() || position.abs() > POSITION_LIMIT)
        {
            return Err(error(
                "dialogue.position",
                "A dialogue node position is outside the supported range",
            ));
        }
        match node.body.as_ref() {
            NodeBody::Line { speaker, text } => {
                if speaker.len() > SPEAKER_BYTES {
                    return Err(error(
                        "dialogue.speaker-limit",
                        "A dialogue speaker exceeds the supported byte limit",
                    ));
                }
                validate_text(text)?;
            }
            NodeBody::Choice { prompt, choices } => {
                validate_text(prompt)?;
                if choices.len() > CHOICE_LIMIT.saturating_sub(choice_ids.len()) {
                    return Err(error(
                        "dialogue.choice-limit",
                        "The dialogue graph contains too many choices",
                    ));
                }
                for choice in choices {
                    if choice.id.0 == 0 || choice.id.0 == u64::MAX {
                        return Err(error(
                            "dialogue.choice-id",
                            "A dialogue choice identifier is outside the supported range",
                        ));
                    }
                    if !choice_ids.insert(choice.id) {
                        return Err(error(
                            "dialogue.choice-duplicate",
                            "A dialogue choice may appear only once",
                        ));
                    }
                    validate_text(&choice.label)?;
                }
            }
            NodeBody::End { text } => validate_text(text)?,
        }
    }

    for (port, target) in &view.links {
        if !view.contains_output(*port) {
            return Err(error(
                "dialogue.port-missing",
                "The dialogue link output does not exist on its owning node",
            ));
        }
        if !view.nodes.contains_key(target) {
            return Err(error(
                "dialogue.target-missing",
                "The dialogue link target node does not exist",
            ));
        }
    }
    Ok(())
}

fn validate_text(text: &str) -> Result<(), EditorExtensionDocumentError> {
    if text.len() > TEXT_BYTES {
        return Err(error(
            "dialogue.text-limit",
            "A dialogue text field exceeds the supported byte limit",
        ));
    }
    Ok(())
}
