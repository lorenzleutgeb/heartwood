use std::iter::repeat_with;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{post, put};
use axum::{Json, Router};
use axum_auth::AuthBearer;
use hyper::StatusCode;
use radicle::crypto::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::api::auth::{self, AuthState, Session};
use crate::api::error::Error;
use crate::api::json;
use crate::api::Context;
use crate::axum_extra::Path;

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/sessions", post(session_create_handler))
        .route(
            "/sessions/:id",
            put(session_signin_handler)
                .get(session_handler)
                .delete(session_delete_handler),
        )
        .with_state(ctx)
}

#[derive(Debug, Deserialize, Serialize)]
struct AuthChallenge {
    sig: Signature,
    pk: PublicKey,
    #[serde(default)]
    comment: String,
}

/// Create session.
/// `POST /sessions`
async fn session_create_handler(State(ctx): State<Context>) -> impl IntoResponse {
    let mut rng = fastrand::Rng::new();
    let session_id_len = if ctx.session_expiry != auth::DEFAULT_AUTHORIZED_SESSIONS_EXPIRATION {
        auth::DEFAULT_SESSION_ID_CUSTOM_EXPIRATION_LENGTH
    } else {
        auth::DEFAULT_SESSION_ID_LENGTH
    };
    let session_id = repeat_with(|| rng.alphanumeric())
        .take(session_id_len)
        .collect::<String>();
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let session = Session {
        status: AuthState::Unauthorized,
        public_key: *signer.public_key(),
        alias: ctx.profile.config.node.alias.clone(),
        issued_at: OffsetDateTime::now_utc(),
        expires_at: OffsetDateTime::now_utc()
            .checked_add(auth::UNAUTHORIZED_SESSIONS_EXPIRATION)
            .unwrap(),
    };
    let encrypted_session_id = signer
        .try_sign(session_id.as_bytes())
        .map_err(|_| Error::Auth("Unauthorized"))?
        .to_string();
    let mut sessions = ctx.open_session_db()?;
    let ok = sessions
        .insert(&encrypted_session_id, &session)
        .map_err(Error::from)?;
    if !ok {
        return Err(Error::Auth("Error inserting session"));
    }

    Ok::<_, Error>((
        StatusCode::CREATED,
        Json(json::session(session_id, &session)),
    ))
}

/// Get a session.
/// `GET /sessions/:id`
async fn session_handler(
    State(ctx): State<Context>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let encrypted_session_id = signer
        .try_sign(session_id.as_bytes())
        .map_err(|_| Error::Auth("Unauthorized"))?
        .to_string();
    let sessions = ctx.read_session_db()?;
    let session = sessions
        .get(&encrypted_session_id)?
        .ok_or(Error::NotFound)?;

    Ok::<_, Error>(Json(json::session(session_id, &session)))
}

/// Update session.
/// `PUT /sessions/:id`
async fn session_signin_handler(
    State(ctx): State<Context>,
    Path(session_id): Path<String>,
    Json(request): Json<AuthChallenge>,
) -> impl IntoResponse {
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let encrypted_session_id = signer
        .try_sign(session_id.as_bytes())
        .map_err(|_| Error::Auth("Unauthorized"))?
        .to_string();
    let mut sessions = ctx.open_session_db()?;
    let mut session = sessions
        .get(&encrypted_session_id)?
        .ok_or(Error::NotFound)?;
    if session.status == AuthState::Unauthorized {
        if session.public_key != request.pk {
            return Err(Error::Auth("Invalid public key"));
        }
        if session.expires_at <= OffsetDateTime::now_utc() {
            sessions.remove(&encrypted_session_id)?;
            return Err(Error::Auth("Session expired"));
        }
        let payload = format!("{}:{}", session_id, request.pk);
        request
            .pk
            .verify(payload.as_bytes(), &request.sig)
            .map_err(Error::from)?;
        session.status = AuthState::Authorized;
        session.set_expiration(ctx.session_expiry, OffsetDateTime::now_utc())?;

        let mut comment = request.comment;
        if comment.len() > 100 {
            comment = comment[..100].to_string();
        }

        let ok = sessions.mark_authorized(
            &encrypted_session_id,
            session.expires_at.unix_timestamp(),
            &comment,
        )?;
        if !ok {
            return Err(Error::Auth("Error marking authorized session in db"));
        }

        return Ok::<_, Error>(Json(json!({ "success": true })));
    }

    Err(Error::Auth("Session already authorized"))
}

/// Delete session.
/// `DELETE /sessions/:id`
async fn session_delete_handler(
    State(ctx): State<Context>,
    AuthBearer(token): AuthBearer,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    if token != session_id {
        return Err(Error::Auth("Not authorized to delete this session"));
    }
    let signer = ctx.profile.signer().map_err(Error::from)?;
    let encrypted_session_id = signer
        .try_sign(session_id.as_bytes())
        .map_err(|_| Error::Auth("Unauthorized"))?
        .to_string();
    let mut sessions = ctx.open_session_db()?;
    let ok = sessions.remove(&encrypted_session_id)?;
    if !ok {
        return Err(Error::NotFound);
    }

    Ok::<_, Error>(Json(json!({ "success": true })))
}

#[cfg(test)]
mod routes {
    use crate::api::auth;
    use crate::commands::web::{sign, SessionInfo};
    use axum::body::Body;
    use axum::http::StatusCode;
    use radicle_crypto::Signer;
    use std::ops::Sub;
    use time::{Duration, OffsetDateTime};

    use crate::api::auth::{AuthState, Session};
    use crate::test::{self, get, post, put};

    #[tokio::test]
    async fn test_session() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.to_owned());

        // Create session.
        let response = post(&app, "/sessions", None, None).await;
        let status = response.status();
        let json = response.json().await;
        let session_info: SessionInfo = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            session_info.session_id.len(),
            auth::DEFAULT_SESSION_ID_LENGTH
        );

        // Check that an unauthorized session has been created.
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        let json = response.json().await;
        let body: Session = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, AuthState::Unauthorized);

        // Create request body
        let signer = ctx.profile.signer().unwrap();
        let signature = sign(signer, &session_info).unwrap();
        let body = serde_json::to_vec(&super::AuthChallenge {
            sig: signature,
            pk: session_info.public_key,
            comment: "".to_string(),
        })
        .unwrap();

        let response = put(
            &app,
            format!("/sessions/{}", session_info.session_id),
            Some(Body::from(body)),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        // Check that session has been authorized.
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        let json = response.json().await;
        let body: Session = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, AuthState::Authorized);

        // The authorized session id should now be able to be used for authenticated requests
        let ok = auth::validate(&ctx, &session_info.session_id).await;
        assert!(ok.is_ok());

        // The session should be persisted in db with encrypted session id
        let db = ctx.open_session_db().unwrap();
        assert!(db.get(&session_info.session_id).unwrap().is_none());
        let signer = ctx.profile.signer().unwrap();
        let encrypted_id = signer
            .try_sign(session_info.session_id.as_bytes())
            .unwrap()
            .to_string();
        assert!(db.get(&encrypted_id).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_custom_session_expiry() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ctx = test::seed(tmp.path());
        ctx.session_expiry = Duration::seconds(0);
        let app = super::router(ctx.to_owned());

        // Create session.
        let response = post(&app, "/sessions", None, None).await;
        let status = response.status();
        let json = response.json().await;
        let session_info: SessionInfo = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            session_info.session_id.len(),
            auth::DEFAULT_SESSION_ID_CUSTOM_EXPIRATION_LENGTH
        );

        // Check that an unauthorized session has been created.
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        let json = response.json().await;
        let body: Session = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, AuthState::Unauthorized);

        // Create request body
        let signer = ctx.profile.signer().unwrap();
        let signature = sign(signer, &session_info).unwrap();
        let body = serde_json::to_vec(&super::AuthChallenge {
            sig: signature,
            pk: session_info.public_key,
            comment: "".to_string(),
        })
        .unwrap();

        let response = put(
            &app,
            format!("/sessions/{}", session_info.session_id),
            Some(Body::from(body)),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        // Check that session has been authorized.
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        let json = response.json().await;
        let body: Session = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, AuthState::Authorized);
        assert_eq!(body.expires_at.unix_timestamp_nanos(), 0);
    }

    #[tokio::test]
    async fn test_expired_unauthorized_session() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let app = super::router(ctx.to_owned());
        let signer = ctx.profile.signer().unwrap();

        // Create session.
        let response = post(&app, "/sessions", None, None).await;
        let json = response.json().await;
        let session_info: SessionInfo = serde_json::from_value(json).unwrap();

        // Check that an unauthorized session has been created.
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        let json = response.json().await;
        let body: Session = serde_json::from_value(json).unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.status, AuthState::Unauthorized);

        // Find the session in the db
        let mut db = ctx.open_session_db().unwrap();
        // The session id should be encrypted
        assert!(db.get(&session_info.session_id).unwrap().is_none());
        let enc_id = signer
            .try_sign(session_info.session_id.as_bytes())
            .unwrap()
            .to_string();
        let mut s = db.get(&enc_id).unwrap().unwrap();
        assert!(s.expires_at > OffsetDateTime::now_utc());

        // Make it expired
        s.expires_at = OffsetDateTime::now_utc().sub(Duration::seconds(1));
        assert!(db.remove(&enc_id).unwrap());
        assert!(db.insert(&enc_id, &s).unwrap());

        // Create request body
        let signature = sign(signer, &session_info).unwrap();
        let body = serde_json::to_vec(&super::AuthChallenge {
            sig: signature,
            pk: session_info.public_key,
            comment: "".to_string(),
        })
        .unwrap();

        let response = put(
            &app,
            format!("/sessions/{}", session_info.session_id),
            Some(Body::from(body)),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Check that session has been deleted
        let response = get(&app, format!("/sessions/{}", session_info.session_id)).await;
        let status = response.status();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_auth_validate_invalid_session() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = test::seed(tmp.path());
        let ok = auth::validate(&ctx, "invalid_token").await;
        assert!(ok.is_err());
    }
}
