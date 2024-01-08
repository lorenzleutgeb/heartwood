use super::*;

use radicle::cob::{patch, resolve_embed};
use radicle::crypto;
use radicle::prelude::*;
use radicle::storage::git::Repository;

use crate::terminal as term;

pub fn run(
    patch_id: &PatchId,
    revision_id: Option<patch::RevisionId>,
    message: term::patch::Message,
    profile: &Profile,
    repository: &Repository,
) -> anyhow::Result<()> {
    let signer = term::signer(profile)?;
    let mut patches = patch::Patches::open(repository)?;
    let Ok(patch) = patches.get_mut(patch_id) else {
        anyhow::bail!("Patch `{patch_id}` not found");
    };

    let (title, description) = term::patch::get_edit_message(message, &patch)?;

    match revision_id {
        Some(id) => edit_revision(patch, id, title, description, repository, &signer),
        None => edit_root(patch, title, description, repository, &signer),
    }
}

fn edit_root<G>(
    mut patch: patch::PatchMut<'_, '_, Repository>,
    title: String,
    description: String,
    repository: &Repository,
    signer: &G,
) -> anyhow::Result<()>
where
    G: crypto::Signer,
{
    let title = if title != patch.title() {
        Some(title)
    } else {
        None
    };
    let description = if description != patch.description() {
        Some(description)
    } else {
        None
    };

    if title.is_none() && description.is_none() {
        // Nothing to do.
        return Ok(());
    }

    let (root, _) = patch.root();
    let target = patch.target();
    let embeds = patch
        .embeds()
        .iter()
        .filter_map(|embed| resolve_embed(repository, embed.clone()))
        .collect::<Vec<_>>();

    patch.transaction("Edit root", signer, |tx| {
        if let Some(t) = title {
            tx.edit(t, target)?;
        }
        if let Some(d) = description {
            tx.edit_revision(root, d, embeds)?;
        }
        Ok(())
    })?;

    Ok(())
}

fn edit_revision<G>(
    mut patch: patch::PatchMut<'_, '_, Repository>,
    revision: patch::RevisionId,
    mut title: String,
    description: String,
    repository: &Repository,
    signer: &G,
) -> anyhow::Result<()>
where
    G: crypto::Signer,
{
    let embeds = patch
        .embeds()
        .iter()
        .filter_map(|embed| resolve_embed(repository, embed.clone()))
        .collect::<Vec<_>>();
    let description = if description.is_empty() {
        title
    } else {
        title.push('\n');
        title.push_str(&description);
        title
    };
    patch.transaction("Edit revision", signer, |tx| {
        tx.edit_revision(revision, description, embeds)?;
        Ok(())
    })?;
    Ok(())
}
