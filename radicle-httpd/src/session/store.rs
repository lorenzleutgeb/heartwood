use std::path::Path;
use std::str::FromStr;
use std::{fmt, io};

use sqlite as sql;
use thiserror::Error;
use time::OffsetDateTime;

use radicle::crypto::PublicKey;
use radicle::node::{Alias, AliasError};
use radicle::sql::transaction;

use crate::api::auth::{AuthState, AuthStateError, Session};

pub const SESSIONS_DB_FILE: &str = "sessions.db";

#[derive(Error, Debug)]
pub enum SessionStoreError {
    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),

    /// Alias error.
    #[error("alias error: {0}")]
    InvalidAlias(#[from] AliasError),

    /// Public Key error.
    #[error("invalid public key: {0}")]
    InvalidPublicKey(#[from] radicle::crypto::PublicKeyError),

    /// Issue/expiration timestamp error
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(#[from] time::error::ComponentRange),

    #[error("invalid timestamp operation")]
    InvalidTimestampOperation,

    /// An Internal error.
    #[error("internal error: {0}")]
    Internal(#[from] sql::Error),

    /// AuthState error
    #[error(transparent)]
    InvalidAuthState(#[from] AuthStateError),
}

/// A file-backed session storage
pub struct DbSession {
    pub db: sql::ConnectionThreadSafe,
}

impl fmt::Debug for DbSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbSession").finish()
    }
}

impl DbSession {
    const SCHEMA: &'static str = include_str!("store/schema.sql");

    /// Open a session storage at the given path. Creates a new session storage if it
    /// doesn't exist.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SessionStoreError> {
        let db = sql::Connection::open_thread_safe(path)?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Same as [`Self::open`], but in read-only mode. This is useful to have multiple
    /// open databases, as no locking is required.
    pub fn reader<P: AsRef<Path>>(path: P) -> Result<Self, SessionStoreError> {
        let db = sql::Connection::open_thread_safe_with_flags(
            path,
            sqlite::OpenFlags::new().with_read_only(),
        )?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    /// Create a new in-memory address book.
    pub fn memory() -> Result<Self, SessionStoreError> {
        let db = sql::Connection::open_thread_safe(":memory:")?;
        db.execute(Self::SCHEMA)?;

        Ok(Self { db })
    }

    pub fn get(&self, id: &str) -> Result<Option<Session>, SessionStoreError> {
        let mut stmt = self.db.prepare(
            "SELECT status,alias,public_key,issued_at,expires_at
                 FROM sessions WHERE id = ?",
        )?;

        stmt.bind((1, id))?;

        if let Some(Ok(row)) = stmt.into_iter().next() {
            let status = row.read::<&str, _>("status");
            let session_status = AuthState::from_str(status)?;
            let alias = Alias::from_str(row.read::<&str, _>("alias"))?;
            let public_key = PublicKey::from_str(row.read::<&str, _>("public_key"))?;
            let issued_at = row.read::<i64, _>("issued_at");
            let expires_at = row.read::<i64, _>("expires_at");

            Ok(Some(Session {
                status: session_status,
                public_key,
                alias,
                issued_at: OffsetDateTime::from_unix_timestamp(issued_at)?,
                expires_at: OffsetDateTime::from_unix_timestamp(expires_at)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert(&mut self, id: &str, session: &Session) -> Result<bool, SessionStoreError> {
        transaction(&self.db, move |db| {
            let mut stmt = db.prepare(
                "INSERT INTO sessions (id, status, public_key, alias, issued_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            stmt.bind((1, id))?;
            stmt.bind((2, sql::Value::String(session.status.to_string())))?;
            stmt.bind((3, sql::Value::String(session.public_key.into())))?;
            stmt.bind((4, sql::Value::String(session.alias.clone().into())))?;
            stmt.bind((5, session.issued_at.unix_timestamp()))?;
            stmt.bind((6, session.expires_at.unix_timestamp()))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }

    pub fn mark_authorized(
        &mut self,
        id: &str,
        expiry: i64,
        comment: &str,
    ) -> Result<bool, SessionStoreError> {
        transaction(&self.db, move |db| {
            let mut stmt = db.prepare(
                "UPDATE sessions SET
                   status=?1, expires_at=?2, comment=?3
                 WHERE id=?4",
            )?;

            stmt.bind((1, sql::Value::String(AuthState::Authorized.to_string())))?;
            stmt.bind((2, expiry))?;
            stmt.bind((3, comment))?;
            stmt.bind((4, id))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }

    pub fn remove(&mut self, id: &str) -> Result<bool, SessionStoreError> {
        transaction(&self.db, move |db| {
            let mut stmt = db.prepare("DELETE FROM sessions WHERE id = ?1")?;
            stmt.bind((1, id))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }

    pub fn remove_expired(&mut self) -> Result<bool, SessionStoreError> {
        transaction(&self.db, move |db| {
            let mut stmt =
                db.prepare("DELETE FROM sessions WHERE expires_at > 0 AND expires_at < ?1")?;
            stmt.bind((1, OffsetDateTime::now_utc().unix_timestamp()))?;
            stmt.next()?;

            Ok(db.change_count() > 0)
        })
    }
}

#[cfg(test)]
mod test {
    use std::ops::{Add, Sub};

    use time::Duration;

    use crate::api::auth::AuthState::Authorized;
    use radicle_crypto::KeyPair;

    use super::*;

    #[test]
    fn test_temp_db() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sessions_test");
        let mut sdb = DbSession::open(path).unwrap();
        let s = generate_sample_session();

        // insert it
        let sid = "my1";
        assert!(sdb.insert(sid, &s).unwrap());

        // find it
        assert!(sdb.get(sid).unwrap().is_some());

        // update it as authorized
        assert!(sdb.mark_authorized(sid, 0, "").unwrap());

        // find it again, it should contain new status and expiration
        let s2 = sdb.get(sid).unwrap().unwrap();
        assert_eq!(s2.issued_at.unix_timestamp(), s.issued_at.unix_timestamp());
        assert_eq!(s2.expires_at.unix_timestamp(), 0);
        assert_eq!(s2.status, Authorized);

        // delete it
        assert!(sdb.remove(sid).unwrap());
    }

    #[test]
    fn test_get_none() {
        let id = "asd";
        let db = DbSession::memory().unwrap();
        let result = db.get(id).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_remove_nothing() {
        let id = "myid";
        let mut db = DbSession::memory().unwrap();
        let removed = db.remove(id).unwrap();

        assert!(!removed);
    }

    #[test]
    fn test_duplicate_insert() {
        let id = "session_id";
        let mut db = DbSession::memory().unwrap();
        let s1 = generate_sample_session();

        assert!(db.insert(id, &s1).unwrap());

        let s2 = generate_sample_session();
        assert!(db.insert(id, &s2).err().is_some());

        // get it back, it should be s1
        let s3 = db.get(id).unwrap().unwrap();
        assert_eq!(s1.public_key, s3.public_key);
        assert_eq!(s1.issued_at.unix_timestamp(), s3.issued_at.unix_timestamp());
        assert_eq!(
            s1.expires_at.unix_timestamp(),
            s3.expires_at.unix_timestamp()
        );
    }

    #[test]
    fn test_remove() {
        let id = "myid";
        let mut db = DbSession::memory().unwrap();
        let removed = db.remove(id).unwrap();

        assert!(!removed);
    }

    #[test]
    fn test_remove_expired() {
        let mut db = DbSession::memory().unwrap();
        let mut s1 = generate_sample_session();
        s1.expires_at = OffsetDateTime::now_utc().sub(Duration::seconds(1));
        assert!(db.insert("id1", &s1).unwrap());

        let mut s2 = generate_sample_session();
        s2.expires_at = s1.expires_at;
        assert!(db.insert("id2", &s2).unwrap());

        let mut s3 = generate_sample_session();
        s3.expires_at = OffsetDateTime::now_utc().add(Duration::seconds(10));
        assert!(db.insert("id3", &s3).unwrap());

        let removed = db.remove_expired().unwrap();
        assert!(removed);

        // Try to get back id1 or id2 should return nothing
        let result = db.get("id1").unwrap();
        assert!(result.is_none());

        let result = db.get("id2").unwrap();
        assert!(result.is_none());

        let s3 = db.get("id3").unwrap();
        assert!(s3.is_some());
    }

    fn generate_sample_session() -> Session {
        let kp = KeyPair::generate();
        Session {
            status: AuthState::Authorized,
            public_key: PublicKey::from(kp.pk),
            alias: Alias::from_str("alice").unwrap(),
            issued_at: OffsetDateTime::now_utc(),
            expires_at: OffsetDateTime::now_utc().add(Duration::days(1)),
        }
    }
}
