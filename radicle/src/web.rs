use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::prelude::RepoId;

/// Web configuration.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Pinned content.
    pub pinned: Pinned,
    /// URL pointing to an image used in the header of a node page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    /// URL pointing to an image used as the node avatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Node description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Pinned content. This can be used to pin certain content when
/// listing, e.g. pin repositories on a web client.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pinned {
    /// Pinned repositories.
    pub repositories: HashSet<RepoId>,
}
