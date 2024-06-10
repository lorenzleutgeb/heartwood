//! Generic COBs via helpers.
//! A helper is an executable file, for example a (shell) script,
//! or a binary, that implements X.

use serde_json::json;
use tempfile::tempfile;
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::cob::store::Cob;
use crate::test::storage::ReadRepository;

use serde::Deserialize;
use serde::Serialize;

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct Helper(serde_json::Value);

/// Error applying an operation onto a state.
#[derive(Error, Debug)]
pub enum ApplyError {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("git: {0}")]
    GitExt(#[from] git_ext::Error),
    #[error("serde_json: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("not authorized to perform this action")]
    NotAuthorized,
    #[error("apply failed: {0}")]
    Apply(#[from] ApplyError),
    #[error("op decoding failed: {0}")]
    Op(#[from] super::op::OpEncodingError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Action(serde_json::Value);

impl super::store::CobAction for Action {}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

pub type Op = crate::cob::Op<Action>;

impl Cob for Helper {
    type Action = Action;

    type Error = ApplyError;

    fn type_name() -> &'static radicle_cob::TypeName {
        panic!("no static type name available")
    }

    fn from_root<R: crate::test::storage::ReadRepository>(
        op: super::Op<Self::Action>,
        _repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut ser = json!(op);
        ser.as_object_mut().unwrap().insert(
            "actions".to_string(),
            json!(op
                .actions
                .iter()
                .map(|action: &Action| -> Result<serde_json::Value, _> {
                    serde_json::from_value(action.0.clone())
                })
                .collect::<Result<Vec<serde_json::Value>, _>>()?),
        );

        let prefix = String::from("rad-cob-");
        let type_name = op.manifest.type_name.to_string();
        let suffix = type_name
            .rsplit_once('.')
            .map(|(_, suffix)| suffix)
            .unwrap_or(type_name.as_str());
        let mut child = std::process::Command::new(prefix + suffix)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");

        let stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || serde_json::to_writer(stdin, &ser));

        Ok(Helper(serde_json::from_reader(
            child.stdout.take().unwrap(),
        )?))
    }

    fn op<'a, R: crate::test::storage::ReadRepository, I: IntoIterator<Item = &'a super::Entry>>(
        &mut self,
        op: super::Op<Self::Action>,
        concurrent: I,
        _repo: &R,
    ) -> Result<(), <Self as Cob>::Error> {
        if concurrent.into_iter().next().is_some() {
            todo!("what is concurrent?")
        }
        println!("self is {}", json!(&self));

        let temp = NamedTempFile::new()?;
        serde_json::to_writer(&temp, &self)?;
        let temp = temp.into_temp_path();

        let mut ser = json!(op);
        ser.as_object_mut().unwrap().insert(
            "actions".to_string(),
            json!(op
                .actions
                .iter()
                .map(|action: &Action| -> Result<serde_json::Value, _> {
                    serde_json::from_value(action.0.clone())
                })
                .collect::<Result<Vec<serde_json::Value>, _>>()?),
        );

        let prefix = String::from("rad-cob-");
        let type_name = op.manifest.type_name.to_string();
        let suffix = type_name
            .rsplit_once('.')
            .map(|(_, suffix)| suffix)
            .unwrap_or(type_name.as_str());
        let mut child = std::process::Command::new(prefix + suffix)
            .args([temp])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to spawn child process");

        let stdin = child.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || serde_json::to_writer(stdin, &ser));

        Ok(())
    }
}

impl<R: ReadRepository> crate::cob::Evaluate<R> for Helper {
    type Error = Error;

    fn init(entry: &radicle_cob::Entry, store: &R) -> Result<Self, Self::Error> {
        Ok(Self::from_root(Op::try_from(entry)?, store)?)
    }

    fn apply<'a, I: Iterator<Item = (&'a radicle_git_ext::Oid, &'a radicle_cob::Entry)>>(
        &mut self,
        entry: &radicle_cob::Entry,
        concurrent: I,
        store: &R,
    ) -> Result<(), Self::Error> {
        self.op(Op::try_from(entry)?, concurrent.map(|(_, e)| e), store)
            .map_err(Error::Apply)
    }
}
