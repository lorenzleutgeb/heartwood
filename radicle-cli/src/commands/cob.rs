use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::prelude::*;
use nonempty::NonEmpty;
use radicle::cob;
use radicle::cob::helper::Helper;

use radicle::cob::Op;
use radicle::identity::Identity;
use radicle::issue::cache::Issues;
use radicle::patch::cache::Patches;
use radicle::patch::Patch;
use radicle::prelude::RepoId;
use radicle::storage::{ReadStorage, WriteStorage};
use radicle_cob::object::collaboration::list;
use serde_json::json;
use serde_jsonlines::JsonLinesReader;

use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "cob",
    description: "Manage collaborative objects",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad cob <command> [<option>...]
    rad cob act    --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob create --repo <rid> --type <typename> <filename>     [<option>...]
    rad cob list   --repo <rid> --type <typename>
    rad cob log    --repo <rid> --type <typename> --object <oid> [<option>...]
    rad cob show   --repo <rid> --type <typename> --object <oid> [<option>...]

Commands

    act                        Add actions to a COB
    create                     Create a new COB of a given type given initial actions
    list                       List all COBs of a given type (--object is not needed)
    log                        Print a log of all raw operations on a COB

Log options

    --format (pretty | json)   Desired output format (default: pretty)

Act, New, Show options

    --format json              Desired output format (default: json)

Other options

    --help                     Print help
"#,
};

#[derive(Clone, Copy, PartialEq)]
enum OperationName {
    Act,
    Create,
    List,
    Log,
    Show,
}

enum EmbedContent {
    Path(PathBuf),
    Hash(Rev),
}

struct Embed {
    name: String,
    content: EmbedContent,
}

enum Operation {
    Act(Rev),
    Create {
        message: String,
        actions: PathBuf,
        embeds: Vec<Embed>,
    },
    List,
    Log {
        oid: Rev,
        format: Format,
    },
    Show(Rev),
}

enum Format {
    Json,
    Pretty,
}

pub struct Options {
    rid: RepoId,
    op: Operation,
    type_name: cob::TypeName,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;
        use OperationName::*;

        let mut parser = lexopt::Parser::from_args(args);

        let op = match parser.next()? {
            None | Some(Long("help") | Short('h')) => {
                return Err(Error::Help.into());
            }
            Some(Value(val)) => match val.to_string_lossy().as_ref() {
                "act" => Act,
                "create" => Create,
                "list" => List,
                "log" => Log,
                "show" => Show,
                unknown => anyhow::bail!("unknown operation '{unknown}'"),
            },
            Some(arg) => return Err(anyhow::anyhow!(arg.unexpected())),
        };

        let mut type_name: Option<cob::TypeName> = None;
        let mut oid: Option<Rev> = None;
        let mut rid: Option<RepoId> = None;
        let mut format: Format = Format::Pretty;
        let mut message: Option<String> = None;
        let mut embed_name: Option<String> = None;
        let mut embeds: Vec<Embed> = vec![];
        let mut actions: Option<PathBuf> = None;

        while let Some(arg) = parser.next()? {
            match (&op, &arg) {
                (_, Long("help") | Short('h')) => {
                    return Err(Error::Help.into());
                }
                (_, Long("repo") | Short('r')) => {
                    rid = Some(term::args::rid(&parser.value()?)?);
                }
                (_, Long("type") | Short('t')) => {
                    let v = term::args::string(&parser.value()?);
                    type_name = Some(cob::TypeName::from_str(&v)?);
                }
                (Act | Log | Show, Long("object") | Short('o')) => {
                    let v = term::args::string(&parser.value()?);
                    oid = Some(Rev::from(v));
                }
                (Act | Create, Long("message") | Short('m')) => {
                    message = Some(term::args::string(&parser.value()?));
                }
                (Log | Show, Long("format")) => {
                    format = match (op, term::args::string(&parser.value()?).as_ref()) {
                        (Show, "pretty") => Format::Pretty,
                        (_, "json") => Format::Json,
                        (_, unknown) => anyhow::bail!("unknown format '{unknown}'"),
                    };
                }
                (Act | Create, Long("embed-name")) if embed_name.is_none() => {
                    embed_name = Some(term::args::string(&parser.value()?));
                }
                (Act | Create, Long("embed-content-hash")) if embed_name.is_some() => {
                    embeds.push(Embed {
                        name: embed_name.unwrap(),
                        content: EmbedContent::Hash(Rev::from(term::args::string(
                            &parser.value()?,
                        ))),
                    });
                    embed_name = None;
                }
                (Act | Create, Long("embed-content-file")) if embed_name.is_some() => {
                    embeds.push(Embed {
                        name: embed_name.unwrap(),
                        content: EmbedContent::Path(PathBuf::from(&parser.value()?)),
                    });
                    embed_name = None;
                }
                (Act | Create, Value(val)) => {
                    actions = Some(PathBuf::from(term::args::string(&val)));
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        if op != Create && op != List && oid.is_none() {
            anyhow::bail!("an object id must be specified with `--object`")
        } else if let Some(name) = embed_name {
            anyhow::bail!("content for embed with name '{name}' must be specified")
        }

        Ok((
            Options {
                op: {
                    match op {
                        Act => Operation::Act(oid.unwrap()),
                        Create => Operation::Create {
                            message: message.ok_or_else(|| {
                                anyhow!("a message must be specified with `--message`")
                            })?,
                            actions: actions.ok_or_else(|| {
                                anyhow!("a file containing initial actions must be specified")
                            })?,
                            embeds,
                        },
                        List => Operation::List,
                        Log => Operation::Log {
                            oid: oid.unwrap(),
                            format,
                        },
                        Show => Operation::Show(oid.unwrap()),
                    }
                },
                rid: rid
                    .ok_or_else(|| anyhow!("a repository id must be specified with `--repo`"))?,
                type_name: type_name
                    .ok_or_else(|| anyhow!("an object type must be specified with `--type`"))?,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let Options { rid, op, type_name } = options;
    let profile: radicle::Profile = ctx.profile()?;
    let storage = &profile.storage;
    let repo = storage.repository(rid)?;

    match op {
        Operation::Act(_oid) => {
            todo!();
        }
        Operation::Create {
            message,
            embeds: _,
            actions,
        } => {
            let repo = storage.repository_mut(rid)?;
            let reader = JsonLinesReader::new(BufReader::new(File::open(actions)?));

            // TODO(lorenzleutgeb): Handle embeds.

            if type_name == cob::patch::TYPENAME.clone() {
                let store: cob::store::Store<Patch, _> = cob::store::Store::open(&repo)?;
                let actions = reader
                    .read_all::<radicle::cob::patch::Action>()
                    .collect::<std::io::Result<Vec<_>>>()?;
                let actions = NonEmpty::from_vec(actions)
                    .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                let (oid, _) = store.create(&message, actions, vec![], &profile.signer()?)?;
                println!("{}", oid)
            } else if type_name == cob::issue::TYPENAME.clone() {
                let store: cob::store::Store<cob::issue::Issue, _> =
                    cob::store::Store::open(&repo)?;
                let actions = reader
                    .read_all::<radicle::cob::issue::Action>()
                    .collect::<std::io::Result<Vec<_>>>()?;
                let actions = NonEmpty::from_vec(actions)
                    .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                let (oid, _) = store.create(&message, actions, vec![], &profile.signer()?)?;
                println!("{}", oid)
            } else if type_name == cob::identity::TYPENAME.clone() {
                let store: cob::store::Store<radicle::cob::identity::Identity, _> =
                    cob::store::Store::open(&repo)?;
                let actions = reader
                    .read_all::<radicle::cob::identity::Action>()
                    .collect::<std::io::Result<Vec<_>>>()?;
                let actions = NonEmpty::from_vec(actions)
                    .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                let (oid, _) = store.create(&message, actions, vec![], &profile.signer()?)?;
                println!("{}", oid)
            } else {
                let store: cob::store::Store<radicle::cob::helper::Helper, _> =
                    cob::store::Store::open(&repo)?;
                let actions = reader
                    .read_all::<radicle::cob::helper::Action>()
                    .collect::<std::io::Result<Vec<_>>>()?;
                let actions = NonEmpty::from_vec(actions)
                    .ok_or_else(|| anyhow::anyhow!("at least one action is required"))?;
                let (oid, _) =
                    store.create_raw(&type_name, &message, actions, vec![], &profile.signer()?)?;
                println!("{}", oid)
            }
        }
        Operation::List => {
            let cobs = list::<NonEmpty<cob::Entry>, _>(&repo, &type_name)?;
            for cob in cobs {
                println!("{}", cob.id);
            }
        }
        Operation::Log { oid, format } => {
            let oid = oid.resolve(&repo.backend)?;
            let ops = cob::store::ops(&oid, &type_name, &repo)?;

            for op in ops.into_iter().rev() {
                match format {
                    Format::Json => print_op_json(op)?,
                    Format::Pretty => print_op_pretty(op)?,
                }
            }
        }
        Operation::Show(oid) => {
            let oid = &oid.resolve(&repo.backend)?;

            if type_name == cob::patch::TYPENAME.clone() {
                let patches = profile.patches(&repo)?;
                let Some(patch) = patches.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &patch)?
            } else if type_name == cob::issue::TYPENAME.clone() {
                let issues = profile.issues(&repo)?;
                let Some(issue) = issues.get(oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &issue)?
            } else if type_name == cob::identity::TYPENAME.clone() {
                let Some(cob) = cob::get::<Identity, _>(&repo, &type_name, oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &cob.object)?
            } else {
                let Some(cob) = cob::get::<Helper, _>(&repo, &type_name, oid)? else {
                    anyhow::bail!(cob::store::Error::NotFound(type_name, *oid))
                };
                serde_json::to_writer_pretty(std::io::stdout(), &cob.object())?;
            }
            println!();
        }
    }

    Ok(())
}

fn print_op_pretty(op: Op<Vec<u8>>) -> anyhow::Result<()> {
    let time = DateTime::<Utc>::from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(op.timestamp.as_secs()),
    )
    .to_rfc2822();
    term::print(term::format::yellow(format!("commit   {}", op.id)));
    if let Some(oid) = op.identity {
        term::print(term::format::tertiary(format!("resource {oid}")));
    }
    for parent in op.parents {
        term::print(format!("parent   {}", parent));
    }
    for parent in op.related {
        term::print(format!("rel      {}", parent));
    }
    term::print(format!("author   {}", op.author));
    term::print(format!("date     {}", time));
    term::blank();
    for action in op.actions {
        let obj: serde_json::Value = serde_json::from_slice(&action)?;
        let val = serde_json::to_string_pretty(&obj)?;
        for line in val.lines() {
            term::indented(term::format::dim(line));
        }
        term::blank();
    }
    Ok(())
}

fn print_op_json(op: Op<Vec<u8>>) -> anyhow::Result<()> {
    let mut ser = json!(op);
    ser.as_object_mut().unwrap().insert(
        "actions".to_string(),
        json!(op
            .actions
            .iter()
            .map(|action: &Vec<u8>| -> Result<serde_json::Value, _> {
                serde_json::from_slice(action)
            })
            .collect::<Result<Vec<serde_json::Value>, _>>()?),
    );
    term::print(ser);
    Ok(())
}
