//! Presentation-only projections for persistent authoring documents.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_reflect::ComponentTypeId;

use crate::{
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, SceneEntityRecord,
    authoring_decode::{
        AuthoringRecordFact, AuthoringRecordLocation, AuthoringRecordOwnerClaim,
        AuthoringRecordReadiness, PublishedAuthoringPrefabDocument,
        PublishedAuthoringSceneDocument,
    },
};

/// Exact source location omitted from an authoring preview projection.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringPreviewOmissionLocation {
    DirectEntityComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    NestedPrefab {
        entity: SceneEntityId,
    },
}

/// Closed reason one source value was excluded from an authoring preview projection.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPreviewOmissionReason {
    RecordUnavailable(AuthoringRecordReadiness),
    RecordFactMissing,
    RecordFactAmbiguous,
    RecordFactMismatch,
    NestedPrefabUnsupported,
}

/// Typed fact describing one fail-closed authoring preview omission.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringPreviewOmission {
    location: AuthoringPreviewOmissionLocation,
    reason: AuthoringPreviewOmissionReason,
}

impl AuthoringPreviewOmission {
    #[must_use]
    pub const fn location(&self) -> &AuthoringPreviewOmissionLocation {
        &self.location
    }

    #[must_use]
    pub const fn reason(&self) -> AuthoringPreviewOmissionReason {
        self.reason
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoringPreviewDocument<Document> {
    document: Document,
    omissions: Vec<AuthoringPreviewOmission>,
}

impl<Document> AuthoringPreviewDocument<Document> {
    #[must_use]
    pub const fn document(&self) -> &Document {
        &self.document
    }

    #[must_use]
    pub fn omissions(&self) -> &[AuthoringPreviewOmission] {
        &self.omissions
    }

    #[must_use]
    pub fn into_parts(self) -> (Document, Vec<AuthoringPreviewOmission>) {
        (self.document, self.omissions)
    }
}

/// Non-persistent Scene projection admitted only for installed authoring presentation.
///
/// Direct components survive only when one exact direct-record fact classifies the current
/// owner/version as Available. Nested Prefab instances and their patches are removed and reported
/// rather than interpreted. Projection clones source data and consults no runtime registry or
/// migration path.
#[doc(hidden)]
pub type AuthoringScenePreviewDocument = AuthoringPreviewDocument<SceneDocument>;

/// Non-persistent Prefab projection admitted only for installed authoring presentation.
///
/// It applies the same direct-record and nested-override policy as the Scene projection while
/// retaining Prefab document identity for downstream authoring consumers.
#[doc(hidden)]
pub type AuthoringPrefabPreviewDocument = AuthoringPreviewDocument<PrefabDocument>;

/// Failure to derive a preview from a session without published authoring authority.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringPreviewProjectionError {
    ClassificationBasisUnavailable,
}

impl Display for AuthoringPreviewProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClassificationBasisUnavailable => formatter.write_str(
                "authoring preview requires a session created from a published authoring document",
            ),
        }
    }
}

impl Error for AuthoringPreviewProjectionError {}

/// Derives a presentation-only Scene projection without changing published source authority.
#[doc(hidden)]
#[must_use]
pub fn project_published_authoring_scene_preview(
    published: &PublishedAuthoringSceneDocument,
) -> AuthoringScenePreviewDocument {
    let projection = project_authoring_preview(
        &published.document().roots,
        &published.document().entities,
        published.record_facts(),
    );
    AuthoringPreviewDocument {
        document: SceneDocument {
            roots: projection.roots,
            entities: projection.entities,
        },
        omissions: projection.omissions,
    }
}

/// Derives a presentation-only Scene projection from the current authoring session state.
#[doc(hidden)]
pub fn project_authoring_scene_session_preview(
    session: &SceneAuthoringSession,
) -> Result<AuthoringScenePreviewDocument, AuthoringPreviewProjectionError> {
    let record_facts = session
        .authoring_record_facts()
        .ok_or(AuthoringPreviewProjectionError::ClassificationBasisUnavailable)?;
    let projection = project_authoring_preview(
        &session.document().roots,
        &session.document().entities,
        record_facts,
    );
    Ok(AuthoringPreviewDocument {
        document: SceneDocument {
            roots: projection.roots,
            entities: projection.entities,
        },
        omissions: projection.omissions,
    })
}

/// Derives a presentation-only Prefab projection without changing published source authority.
#[doc(hidden)]
#[must_use]
pub fn project_published_authoring_prefab_preview(
    published: &PublishedAuthoringPrefabDocument,
) -> AuthoringPrefabPreviewDocument {
    let projection = project_authoring_preview(
        &published.document().roots,
        &published.document().entities,
        published.record_facts(),
    );
    AuthoringPreviewDocument {
        document: PrefabDocument {
            roots: projection.roots,
            entities: projection.entities,
        },
        omissions: projection.omissions,
    }
}

/// Derives a presentation-only Prefab projection from the current authoring session state.
#[doc(hidden)]
pub fn project_authoring_prefab_session_preview(
    session: &PrefabAuthoringSession,
) -> Result<AuthoringPrefabPreviewDocument, AuthoringPreviewProjectionError> {
    let record_facts = session
        .authoring_record_facts()
        .ok_or(AuthoringPreviewProjectionError::ClassificationBasisUnavailable)?;
    let projection = project_authoring_preview(
        &session.document().roots,
        &session.document().entities,
        record_facts,
    );
    Ok(AuthoringPreviewDocument {
        document: PrefabDocument {
            roots: projection.roots,
            entities: projection.entities,
        },
        omissions: projection.omissions,
    })
}

struct AuthoringPreviewDocumentParts {
    roots: Vec<SceneEntityId>,
    entities: Vec<SceneEntityRecord>,
    omissions: Vec<AuthoringPreviewOmission>,
}

enum IndexedDirectRecordFact<'a> {
    Unique(&'a AuthoringRecordFact),
    Ambiguous,
}

fn project_authoring_preview(
    roots: &[SceneEntityId],
    source_entities: &[SceneEntityRecord],
    record_facts: &[AuthoringRecordFact],
) -> AuthoringPreviewDocumentParts {
    let mut direct_facts =
        BTreeMap::<SceneEntityId, BTreeMap<ComponentTypeId, IndexedDirectRecordFact<'_>>>::new();
    for fact in record_facts {
        let AuthoringRecordLocation::EntityComponent { entity } = fact.location() else {
            continue;
        };
        let facts = direct_facts.entry(entity.clone()).or_default();
        match facts.entry(fact.component().clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(IndexedDirectRecordFact::Unique(fact));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.insert(IndexedDirectRecordFact::Ambiguous);
            }
        }
    }

    let mut omissions = Vec::new();
    let mut entities = Vec::with_capacity(source_entities.len());
    for source_entity in source_entities {
        let mut entity = SceneEntityRecord::new(source_entity.id.clone())
            .with_children(source_entity.children.iter().cloned());
        let entity_facts = direct_facts.get(&entity.id);
        for (component, record) in &source_entity.components {
            let reason = match entity_facts.and_then(|facts| facts.get(component)) {
                None => Some(AuthoringPreviewOmissionReason::RecordFactMissing),
                Some(IndexedDirectRecordFact::Ambiguous) => {
                    Some(AuthoringPreviewOmissionReason::RecordFactAmbiguous)
                }
                Some(IndexedDirectRecordFact::Unique(fact))
                    if fact.version() != record.version
                        || !owner_matches_record(fact.owner(), record) =>
                {
                    Some(AuthoringPreviewOmissionReason::RecordFactMismatch)
                }
                Some(IndexedDirectRecordFact::Unique(fact)) if !fact.readiness().is_available() => {
                    Some(AuthoringPreviewOmissionReason::RecordUnavailable(
                        fact.readiness(),
                    ))
                }
                Some(IndexedDirectRecordFact::Unique(_)) => None,
            };
            if let Some(reason) = reason {
                omissions.push(AuthoringPreviewOmission {
                    location: AuthoringPreviewOmissionLocation::DirectEntityComponent {
                        entity: entity.id.clone(),
                        component: component.clone(),
                    },
                    reason,
                });
            } else {
                entity.components.insert(component.clone(), record.clone());
            }
        }

        if source_entity.prefab.is_some() {
            omissions.push(AuthoringPreviewOmission {
                location: AuthoringPreviewOmissionLocation::NestedPrefab {
                    entity: entity.id.clone(),
                },
                reason: AuthoringPreviewOmissionReason::NestedPrefabUnsupported,
            });
        }
        entities.push(entity);
    }

    AuthoringPreviewDocumentParts {
        roots: roots.to_vec(),
        entities,
        omissions,
    }
}

fn owner_matches_record(owner: &AuthoringRecordOwnerClaim, record: &SceneComponentRecord) -> bool {
    match (owner, record.owner()) {
        (AuthoringRecordOwnerClaim::Declared(fact), Some(record)) => fact == record,
        (AuthoringRecordOwnerClaim::Unclaimed, None) => true,
        (AuthoringRecordOwnerClaim::Declared(_), None)
        | (AuthoringRecordOwnerClaim::Unclaimed, Some(_)) => false,
    }
}
