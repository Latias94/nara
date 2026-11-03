use std::{
    fs, thread,
    time::{Duration, Instant},
};

use nara_asset::AssetSourceKind;
use nara_core::ByteLimit;
use nara_fs::RelativePath;
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
use nara_scene::{AuthoringSchemaBasis, PrefabDocument, SceneDocument, SourceDocumentKind};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
    document::{
        AuthoringDocumentAdmissionBasis, AuthoringDocumentKind, AuthoringDocumentSourceFact,
    },
    native_source::NativeDocumentSourcePolicy,
    source_index::ProjectSourceIndexState,
    source_inventory::{test_support::*, *},
};

mod successor;
