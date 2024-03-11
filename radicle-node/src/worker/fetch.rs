pub mod error;

use std::collections::HashSet;
use std::str::FromStr;

use localtime::LocalTime;

use radicle::crypto::PublicKey;
use radicle::prelude::RepoId;
use radicle::storage::refs::SignedRefsUpdate;
use radicle::storage::{
    ReadRepository, ReadStorage as _, RefUpdate, RemoteRepository, WriteRepository as _,
};
use radicle::{cob, git, node, Storage};
use radicle_fetch::{Allowed, BlockList, FetchLimit};

use super::channels::ChannelsFlush;

#[derive(Debug, Default)]
pub struct FetchResult {
    /// The set of updates references.
    pub updated: Vec<RefUpdate>,
    /// The set of remote namespaces that were updated.
    pub namespaces: HashSet<PublicKey>,
}

pub enum Handle {
    Clone {
        handle: radicle_fetch::Handle<ChannelsFlush>,
        tmp: tempfile::TempDir,
    },
    Pull {
        handle: radicle_fetch::Handle<ChannelsFlush>,
        notifications: node::notifications::StoreWriter,
    },
}

impl Handle {
    pub fn new(
        rid: RepoId,
        local: PublicKey,
        storage: &Storage,
        follow: Allowed,
        blocked: BlockList,
        channels: ChannelsFlush,
        notifications: node::notifications::StoreWriter,
    ) -> Result<Self, error::Handle> {
        let exists = storage.contains(&rid)?;
        if exists {
            let repo = storage.repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, follow, blocked, channels)?;
            Ok(Handle::Pull {
                handle,
                notifications,
            })
        } else {
            let (repo, tmp) = storage.lock_repository(rid)?;
            let handle = radicle_fetch::Handle::new(local, repo, follow, blocked, channels)?;
            Ok(Handle::Clone { handle, tmp })
        }
    }

    pub fn fetch(
        self,
        rid: RepoId,
        storage: &Storage,
        cache: &mut cob::cache::StoreWriter,
        limit: FetchLimit,
        remote: PublicKey,
        refs_at: Option<Vec<SignedRefsUpdate>>,
    ) -> Result<FetchResult, error::Fetch> {
        let (result, notifs) = match self {
            Self::Clone { mut handle, tmp } => {
                log::debug!(target: "worker", "{} cloning from {remote}", handle.local());
                let result = radicle_fetch::clone(&mut handle, limit, remote)?;
                mv(tmp, storage, &rid)?;
                (result, None)
            }
            Self::Pull {
                mut handle,
                notifications,
            } => {
                log::debug!(target: "worker", "{} pulling from {remote}", handle.local());
                let result = radicle_fetch::pull(&mut handle, limit, remote, refs_at)?;
                (result, Some(notifications))
            }
        };

        for rejected in result.rejected() {
            log::warn!(target: "worker", "Rejected update for {}", rejected.refname())
        }

        for warn in result.warnings() {
            log::warn!(target: "worker", "Validation error: {}", warn);
        }

        match result {
            radicle_fetch::FetchResult::Failed { failures, .. } => {
                for fail in failures.iter() {
                    log::error!(target: "worker", "Validation error: {}", fail);
                }
                Err(error::Fetch::Validation)
            }
            radicle_fetch::FetchResult::Success {
                applied, remotes, ..
            } => {
                // N.b. We do not go through handle for this since the cloning handle
                // points to a repository that is temporary and gets moved by [`mv`].
                let repo = storage.repository(rid)?;
                repo.set_identity_head()?;
                repo.set_head()?;

                // Notifications are only posted for pulls, not clones.
                if let Some(mut store) = notifs {
                    // Only create notifications for repos that we have
                    // contributed to in some way, otherwise our inbox will
                    // be flooded by all the repos we are seeding.
                    if repo.remote(&storage.info().key).is_ok() {
                        notify(&rid, &applied, &mut store)?;
                    }
                }

                cache_cobs(&rid, &applied.updated, &repo, cache)?;

                Ok(FetchResult {
                    updated: applied.updated,
                    namespaces: remotes.into_iter().collect(),
                })
            }
        }
    }
}

/// In the case of cloning, we have performed the fetch into a
/// temporary directory -- ensuring that no concurrent operations
/// see an empty repository.
///
/// At the end of the clone, we perform a rename of the temporary
/// directory to the storage repository.
///
/// # Errors
///   - Will fail if `storage` contains `rid` already.
fn mv(tmp: tempfile::TempDir, storage: &Storage, rid: &RepoId) -> Result<(), error::Fetch> {
    use std::io::{Error, ErrorKind};

    let from = tmp.path();
    let to = storage.path_of(rid);

    if !to.exists() {
        std::fs::rename(from, to)?;
    } else {
        log::warn!(target: "worker", "Refusing to move cloned repository {rid} already exists");
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!("repository already exists {:?}", to),
        )
        .into());
    }

    Ok(())
}

// Post notifications for the given refs.
fn notify(
    rid: &RepoId,
    refs: &radicle_fetch::git::refs::Applied<'static>,
    store: &mut node::notifications::StoreWriter,
) -> Result<(), error::Fetch> {
    let now = LocalTime::now();

    for update in refs.updated.iter() {
        if let Some(r) = update.name().to_namespaced() {
            let r = r.strip_namespace();
            if r == *git::refs::storage::SIGREFS_BRANCH {
                // Don't notify about signed refs.
                continue;
            }
            if r == *git::refs::storage::IDENTITY_BRANCH {
                // Don't notify about the peers's identity branch pointer, since there will
                // be a separate notification on the identity COB itself.
                continue;
            }
            if let Some(rest) = r.strip_prefix(git::refname!("refs/heads/patches")) {
                if radicle::cob::ObjectId::from_str(rest.as_str()).is_ok() {
                    // Don't notify about patch branches, since we already get
                    // notifications about patch updates.
                    continue;
                }
            }
        }
        if let RefUpdate::Skipped { .. } = update {
            // Don't notify about skipped refs.
        } else if let Err(e) = store.insert(rid, update, now) {
            log::error!(
                target: "worker",
                "Failed to update notification store for {rid}: {e}"
            );
        }
    }
    Ok(())
}

/// Write new `RefUpdate`s that are related a `Patch` or an `Issue`
/// COB to the COB cache.
fn cache_cobs<S, C>(
    rid: &RepoId,
    refs: &[RefUpdate],
    storage: &S,
    cache: &mut C,
) -> Result<(), error::Cache>
where
    S: ReadRepository + cob::Store,
    C: cob::cache::Update<cob::issue::Issue> + cob::cache::Update<cob::patch::Patch>,
    C: cob::cache::Remove<cob::issue::Issue> + cob::cache::Remove<cob::patch::Patch>,
{
    let issues = cob::issue::Issues::open(storage)?;
    let patches = cob::patch::Patches::open(storage)?;
    for update in refs {
        match update {
            RefUpdate::Updated { name, .. }
            | RefUpdate::Created { name, .. }
            | RefUpdate::Deleted { name, .. } => match name.to_namespaced() {
                Some(name) => {
                    let Some(identifier) = cob::TypedId::from_namespaced(&name)? else {
                        continue;
                    };
                    if identifier.is_issue() {
                        if let Some(issue) = issues.get(&identifier.id)? {
                            cache
                                .update(rid, &identifier.id, &issue)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Update {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: e.into(),
                                })?;
                        }
                    } else if identifier.is_patch() {
                        if let Some(patch) = patches.get(&identifier.id)? {
                            cache
                                .update(rid, &identifier.id, &patch)
                                .map(|_| ())
                                .map_err(|e| error::Cache::Update {
                                    id: identifier.id,
                                    type_name: identifier.type_name,
                                    err: e.into(),
                                })?;
                        }
                    }
                }
                None => continue,
            },
            RefUpdate::Skipped { .. } => { /* Do nothing */ }
        }
    }

    Ok(())
}
