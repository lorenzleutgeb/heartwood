use std::ffi::OsString;

use radicle::storage::{ReadStorage, RepositoryInfo};
use radicle_crypto::Verified;

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

use term::Element;

pub const HELP: Help = Help {
    name: "ls",
    description: "List repositories",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad ls [<option>...]

    By default, this command shows you all repositories that you have forked or initialized.
    If you wish to see all seeded repositories, use the `--all` option.

Options

    --private       Show only private repositories
    --public        Show only public repositories
    --seeded, -s    Show all seeded repositories
    --all, -a       Show all repositories in storage
    --verbose, -v   Verbose output
    --json          JSON output
    --help          Print help
"#,
};

pub struct Options {
    #[allow(dead_code)]
    verbose: bool,
    public: bool,
    private: bool,
    all: bool,
    seeded: bool,
    json: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut verbose = false;
        let mut private = false;
        let mut public = false;
        let mut all = false;
        let mut seeded = false;
        let mut json = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("all") | Short('a') => {
                    all = true;
                }
                Long("seeded") | Short('s') => {
                    seeded = true;
                }
                Long("private") => {
                    private = true;
                }
                Long("public") => {
                    public = true;
                }
                Long("verbose") | Short('v') => verbose = true,
                Long("json") => json = true,
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok((
            Options {
                verbose,
                private,
                public,
                all,
                seeded,
                json,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let repos = storage.repositories()?;
    let policy = profile.policies()?;

    let repos = repos.into_iter().filter_map(move |repo| {
        let RepositoryInfo { rid, doc, refs, .. } = &repo;
        if doc.visibility.is_public() && options.private && !options.public {
            return None;
        }
        if !doc.visibility.is_public() && !options.private && options.public {
            return None;
        }
        if refs.is_none() && !options.all && !options.seeded {
            return None;
        }
        match policy.is_seeding(rid) {
            Ok(seeded) => {
                if !seeded && !options.all {
                    return None;
                }
                if !seeded && options.seeded {
                    return None;
                }
                Some(Ok((repo, seeded)))
            }
            Err(e) => Some(Err(anyhow::anyhow!(e))),
        }
    });

    if options.json {
        print_json(repos)?;
    } else {
        print_table(repos)?;
    }

    Ok(())
}

fn print_table(
    repositories: impl Iterator<Item = anyhow::Result<(RepositoryInfo<Verified>, bool)>>,
) -> anyhow::Result<()> {
    let mut rows = repositories
        .map(|repo| {
            let (
                RepositoryInfo {
                    rid,
                    head,
                    doc,
                    refs: _,
                },
                seeded,
            ) = repo?;
            let proj = doc.project()?;
            let head = term::format::oid(head).into();

            Ok([
                term::format::bold(proj.name().to_owned()),
                term::format::tertiary(rid.urn()),
                if seeded {
                    term::format::visibility(&doc.visibility).into()
                } else {
                    term::format::dim("local").into()
                },
                term::format::secondary(head),
                term::format::italic(proj.description().to_owned()),
            ])
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    rows.sort();

    if rows.is_empty() {
        term::print(term::format::italic("Nothing to show."));
    } else {
        let mut table = term::Table::new(radicle_term::TableOptions::bordered());
        table.push([
            "Name".into(),
            "RID".into(),
            "Visibility".into(),
            "Head".into(),
            "Description".into(),
        ]);
        table.divider();
        table.extend(rows);
        table.print();
    }

    Ok(())
}

fn print_json(
    repositories: impl Iterator<Item = anyhow::Result<(RepositoryInfo<Verified>, bool)>>,
) -> anyhow::Result<()> {
    for repo in repositories {
        let (
            RepositoryInfo {
                rid,
                head,
                doc,
                refs: _,
            },
            seeded,
        ) = repo?;
        let proj = doc.project()?;
        let visibility = if seeded {
            match doc.visibility {
                radicle::identity::Visibility::Public => "public",
                radicle::identity::Visibility::Private { .. } => "private",
            }
        } else {
            "local"
        };
        println!(
            "{}",
            serde_json::json!({
                "name": proj.name(),
                "rid": rid,
                "visibility": visibility,
                "head": head,
                "description": proj.description()
            })
        );
    }
    Ok(())
}
