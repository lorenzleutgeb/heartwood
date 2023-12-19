use std::ffi::OsString;
use std::time;

use anyhow::anyhow;

use radicle::node::policy;
use radicle::node::policy::Scope;
use radicle::node::Handle;
use radicle::{prelude::*, Node};
use radicle_term::Element as _;

use crate::commands::rad_sync as sync;
use crate::terminal::args::{Args, Error, Help};
use crate::{project, terminal as term};

pub const HELP: Help = Help {
    name: "seed",
    description: "Manage repository seeding policies",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad seed [<rid>] [-d | --delete] [--[no-]fetch] [--scope <scope>] [<option>...]

    The `seed` command, when no Repository ID (<rid>) is provided, will list the
    repositories being seeded.

    When a Repository ID (<rid>) is provided it updates the seeding policy for
    that repository. By default, a seeding policy will be created or updated.
    To delete a policy, use the `--delete` flag.

    When seeding a repository, a scope can be specified: this can be either `all` or
    `followed`. When using `all`, all remote nodes will be followed for that repository.
    On the other hand, with `followed`, only the repository delegates will be followed,
    plus any remote that is explicitly followed via `rad follow <nid>`.

Options

    --delete, -d           Delete the seeding policy
    --[no-]fetch           Fetch repository after updating seeding policy
    --scope <scope>        Peer follow scope for this repository
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug)]
pub enum Operation {
    Seed { rid: Id, fetch: bool, scope: Scope },
    List,
    Unseed { rid: Id },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum OperationName {
    #[default]
    Seed,
    Unseed,
}

#[derive(Debug)]
pub struct Options {
    pub op: Operation,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut rid: Option<Id> = None;
        let mut scope: Option<Scope> = None;
        let mut fetch: Option<bool> = None;
        let mut op: Option<OperationName> = None;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match &arg {
                Value(val) => {
                    rid = Some(term::args::rid(val)?);
                }
                Long("delete") | Short('d') if op.is_none() => {
                    op = Some(OperationName::Unseed);
                }
                Long("scope") if op.unwrap_or_default() == OperationName::Seed => {
                    let val = parser.value()?;
                    scope = Some(term::args::parse_value("scope", val)?);
                }
                Long("fetch") if op.unwrap_or_default() == OperationName::Seed => {
                    fetch = Some(true);
                }
                Long("no-fetch") if op.unwrap_or_default() == OperationName::Seed => {
                    fetch = Some(false);
                }
                Long("verbose") | Short('v') => verbose = true,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match rid {
            Some(rid) => match op.unwrap_or_default() {
                OperationName::Seed => Operation::Seed {
                    rid,
                    fetch: fetch.unwrap_or(true),
                    scope: scope.unwrap_or(Scope::All),
                },
                OperationName::Unseed => Operation::Unseed { rid },
            },
            None => Operation::List,
        };

        Ok((Options { op, verbose }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let mut node = radicle::Node::new(profile.socket());

    match options.op {
        Operation::Unseed { rid } => delete(rid, &mut node, &profile)?,
        Operation::Seed { rid, fetch, scope } => {
            update(rid, scope, &mut node, &profile)?;

            if fetch && node.is_running() {
                sync::fetch(
                    rid,
                    sync::RepoSync::default(),
                    time::Duration::from_secs(6),
                    &mut node,
                )?;
            }
        }
        Operation::List => seeding(&profile)?,
    }

    Ok(())
}

pub fn update(
    rid: Id,
    scope: Scope,
    node: &mut Node,
    profile: &Profile,
) -> Result<(), anyhow::Error> {
    let updated = project::seed(rid, scope, node, profile)?;
    let outcome = if updated { "updated" } else { "exists" };

    term::success!(
        "Seeding policy {outcome} for {} with scope '{scope}'",
        term::format::tertiary(rid),
    );

    Ok(())
}

pub fn delete(rid: Id, node: &mut Node, profile: &Profile) -> anyhow::Result<()> {
    if project::unseed(rid, node, profile)? {
        term::success!("Seeding policy for {} removed", term::format::tertiary(rid));
    }
    Ok(())
}

pub fn seeding(profile: &Profile) -> anyhow::Result<()> {
    let store = profile.policies()?;
    let mut t = term::Table::new(term::table::TableOptions::bordered());
    t.push([
        term::format::default(String::from("RID")),
        term::format::default(String::from("Scope")),
        term::format::default(String::from("Policy")),
    ]);
    t.divider();

    for policy::Repo { id, scope, policy } in store.seed_policies()? {
        let id = id.to_string();
        let scope = scope.to_string();
        let policy = policy.to_string();

        t.push([
            term::format::highlight(id),
            term::format::secondary(scope),
            term::format::secondary(policy),
        ])
    }
    t.print();

    Ok(())
}
