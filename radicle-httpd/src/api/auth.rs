use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Formatter;
use std::str::FromStr;
use time::serde::timestamp;
use time::{Duration, OffsetDateTime};

use radicle::crypto::PublicKey;
use radicle::node::Alias;

use crate::api::error::Error;
use crate::api::Context;
use crate::session::store::SessionStoreError;

pub const UNAUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::seconds(60);
pub const DEFAULT_AUTHORIZED_SESSIONS_EXPIRATION: Duration = Duration::weeks(1);
pub const DEFAULT_SESSION_ID_LENGTH: usize = 32;
pub const DEFAULT_SESSION_ID_CUSTOM_EXPIRATION_LENGTH: usize = 64;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthState {
    Authorized,
    Unauthorized,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthStateError {
    #[error("invalid authorization state")]
    Invalid,
}

impl FromStr for AuthState {
    type Err = AuthStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "authorized" => Ok(AuthState::Authorized),
            "unauthorized" => Ok(AuthState::Unauthorized),
            _ => Err(AuthStateError::Invalid),
        }
    }
}

impl fmt::Display for AuthState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorized => write!(f, "authorized"),
            Self::Unauthorized => write!(f, "unauthorized"),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub status: AuthState,
    pub public_key: PublicKey,
    pub alias: Alias,
    #[serde(with = "timestamp")]
    pub issued_at: OffsetDateTime,
    /// The session expiration timestamp.
    /// If `expires_at.unix_timestamp_nanos() <= 0`, then the session lasts indefinitely.
    #[serde(with = "timestamp")]
    pub expires_at: OffsetDateTime,
}

impl Session {
    /// Check if session is authorized. It must have authorized status and not be expired.
    fn is_authorized(&self, current_time: OffsetDateTime) -> bool {
        self.status == AuthState::Authorized && !self.is_expired(current_time)
    }

    /// Check if session is expired, based on current time. `expires_at <= 0` means no expiration.
    fn is_expired(&self, current_time: OffsetDateTime) -> bool {
        self.expires_at.unix_timestamp_nanos() > 0 && self.expires_at <= current_time
    }

    /// Set session expiration timestamp, based on current time and configured duration of sessions.
    /// When the expiration duration is <= 0, it means that sessions never expire.
    pub fn set_expiration(
        &mut self,
        expiry: Duration,
        current_time: OffsetDateTime,
    ) -> Result<(), SessionStoreError> {
        // zero or negative expiration duration means that the session does not expire
        self.expires_at = if expiry.is_zero() || expiry.is_negative() {
            OffsetDateTime::from_unix_timestamp(0).map_err(SessionStoreError::InvalidTimestamp)?
        } else {
            current_time
                .checked_add(expiry)
                .ok_or(SessionStoreError::InvalidTimestampOperation)?
        };

        Ok(())
    }
}

pub async fn validate(ctx: &Context, token: &str) -> Result<(), Error> {
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let encrypted_session_id = signer
        .try_sign(token.as_bytes())
        .map_err(|_| Error::Auth("Unauthorized"))?
        .to_string();
    let sessions = ctx.read_session_db()?;
    let session = sessions
        .get(&encrypted_session_id)?
        .ok_or(Error::Auth("Unauthorized"))?;

    let now = OffsetDateTime::now_utc();
    if !session.is_authorized(now) {
        if session.is_expired(now) {
            let mut db = ctx.open_session_db()?;
            db.remove(&encrypted_session_id)?;
        }
        return Err(Error::Auth("Unauthorized"));
    }

    Ok(())
}
