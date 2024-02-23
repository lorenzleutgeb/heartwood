use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use radicle::identity::{Did, Visibility};
use radicle::issue::cache::Issues as _;
use radicle::node::routing::Store;
use radicle::patch::cache::Patches as _;
use radicle::storage::{ReadRepository, ReadStorage};

use crate::api::error::Error;
use crate::api::project::Info;
use crate::api::Context;
use crate::api::{PaginationQuery, ProjectQuery};
use crate::axum_extra::{Path, Query};

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route(
            "/delegates/:delegate/projects",
            get(delegates_projects_handler),
        )
        .with_state(ctx)
}

/// List all projects which delegate is a part of.
/// `GET /delegates/:delegate/projects`
async fn delegates_projects_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(ctx): State<Context>,
    Path(delegate): Path<Did>,
    Query(qs): Query<PaginationQuery>,
) -> impl IntoResponse {
    let PaginationQuery {
        show,
        page,
        per_page,
    } = qs;
    let page = page.unwrap_or(0);
    let per_page = per_page.unwrap_or(10);
    let storage = &ctx.profile.storage;
    let db = &ctx.profile.database()?;
    let pinned = &ctx.profile.config.web.pinned;
    let mut projects = match show {
        ProjectQuery::All => storage
            .repositories()?
            .into_iter()
            .filter(|repo| match &repo.doc.visibility {
                Visibility::Private { .. } => addr.ip().is_loopback(),
                Visibility::Public => true,
            })
            .collect::<Vec<_>>(),
        ProjectQuery::Pinned => storage.repositories_by_id(pinned.repositories.iter())?,
    };
    projects.sort_by_key(|p| p.rid);

    let infos = projects
        .into_iter()
        .filter_map(|id| {
            if !id.doc.delegates.iter().any(|d| *d == delegate) {
                return None;
            }
            let Ok(repo) = storage.repository(id.rid) else {
                return None;
            };
            let Ok((_, head)) = repo.head() else {
                return None;
            };
            let Ok(payload) = id.doc.project() else {
                return None;
            };
            let Ok(issues) = ctx.profile.issues(&repo) else {
                return None;
            };
            let Ok(issues) = issues.counts() else {
                return None;
            };
            let Ok(patches) = ctx.profile.patches(&repo) else {
                return None;
            };
            let Ok(patches) = patches.counts() else {
                return None;
            };

            let delegates = id.doc.delegates;
            let seeding = db.count(&id.rid).unwrap_or_default();

            Some(Info {
                payload,
                delegates,
                visibility: id.doc.visibility,
                head,
                issues,
                patches,
                id: id.rid,
                seeding,
            })
        })
        .skip(page * per_page)
        .take(per_page)
        .collect::<Vec<_>>();

    Ok::<_, Error>(Json(infos))
}

#[cfg(test)]
mod routes {
    use std::net::SocketAddr;

    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::StatusCode;
    use serde_json::json;

    use crate::test::{self, get, HEAD, RID};

    #[tokio::test]
    async fn test_delegates_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = test::seed(tmp.path());
        let app = super::router(seed.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 8080))));
        let response = get(
            &app,
            "/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects?show=all",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "failed response: {:?}",
            response.json().await
        );
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world-private",
                "description": "Private Rad repository for tests",
                "defaultBranch": "master",
                "delegates": [
                  "did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
                ],
                "visibility": {
                  "type": "private",
                },
                "head": "d26ed310ed140fbef2a066aa486cf59a0f9f7812",
                "patches": {
                  "open": 0,
                  "draft": 0,
                  "archived": 0,
                  "merged": 0,
                },
                "issues": {
                  "open": 0,
                  "closed": 0,
                },
                "id": "rad:zLuTzcmoWMcdK37xqArS8eckp9vK",
                "seeding": 0,
              },
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": ["did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"],
                "visibility": {
                  "type": "public"
                },
                "head": HEAD,
                "patches": {
                  "open": 1,
                  "draft": 0,
                  "archived": 0,
                  "merged": 0,
                },
                "issues": {
                  "open": 1,
                  "closed": 0,
                },
                "id": RID,
                "seeding": 0,
              },
            ])
        );

        let app = super::router(seed).layer(MockConnectInfo(SocketAddr::from((
            [192, 168, 13, 37],
            8080,
        ))));
        let response = get(
            &app,
            "/delegates/did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi/projects?show=all",
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "failed response: {:?}",
            response.json().await
        );
        assert_eq!(
            response.json().await,
            json!([
              {
                "name": "hello-world",
                "description": "Rad repository for tests",
                "defaultBranch": "master",
                "delegates": ["did:key:z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi"],
                "visibility": {
                  "type": "public"
                },
                "head": HEAD,
                "patches": {
                  "open": 1,
                  "draft": 0,
                  "archived": 0,
                  "merged": 0,
                },
                "issues": {
                  "open": 1,
                  "closed": 0,
                },
                "id": RID,
                "seeding": 0,
              }
            ])
        );
    }
}
