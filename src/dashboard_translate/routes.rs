//! The page and the route it posts to. Thin transport: all the work is
//! `preview` and `example_message`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde_json::{json, Value};

use super::example::example_message;
use super::preview::preview;
use super::{MAX_MESSAGE_BYTES, PAGE_ROUTE, PREVIEW_ROUTE};
use crate::dashboard::{Profiles, WEB_MOUNT};

/// Register the page, its place in the navigation and the route it posts
/// to. Called from `dashboard::install`.
pub fn install(app: &mut chassis::App, profiles: Arc<Profiles>) {
    app.page(chassis::shell::pages::Page::new(
        "translate",
        "Try a translation",
        PAGE_ROUTE,
    ));
    let router = Router::new()
        .route(PAGE_ROUTE, get(page))
        .route(PREVIEW_ROUTE, post(preview_route))
        // Twice the cap: a message over the cap reaches `preview` and is
        // refused in words; only something far larger gets the bare 413.
        .layer(DefaultBodyLimit::max(MAX_MESSAGE_BYTES * 2))
        .with_state(profiles);
    app.dashboard_routes(router);
}

fn json(status: StatusCode, body: String) -> Response {
    (status, [("content-type", "application/json")], body).into_response()
}

async fn preview_route(
    State(profiles): State<Arc<Profiles>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    let Some(profile) = profiles.store().get(&name) else {
        let names = profiles
            .snapshot()
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return json(
            StatusCode::NOT_FOUND,
            json!({
                "error": format!("there is no profile named '{name}'"),
                "remedy": format!("pick one of: {names}"),
            })
            .to_string(),
        );
    };
    match preview(&profile, &body) {
        Ok(ok) => json(
            StatusCode::OK,
            serde_json::to_string(&ok).unwrap_or_default(),
        ),
        Err(failure) => {
            let status = if failure.kind == "size" {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            json(status, serde_json::to_string(&failure).unwrap_or_default())
        }
    }
}

/// JSON that is safe inside a `<script type="application/json">`.
fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}

/// One entry per profile for the page: what it takes, where it delivers
/// (never the address itself: a webhook path is a credential), its
/// template and an example message.
fn profile_entries(profiles: &Profiles) -> Vec<Value> {
    let views = profiles.snapshot();
    views
        .iter()
        .filter_map(|view| {
            let profile = profiles.store().get(&view.name)?;
            let delivers = match view.sink.kind {
                "url" => {
                    "delivers to its destination (the address is a credential and is never shown)"
                        .to_string()
                }
                _ => format!("publishes to the hub ({})", view.sink.label),
            };
            Some(json!({
                "name": view.name,
                "takes": view.source.label,
                "delivers": delivers,
                "content_type": profile.content_type,
                "template": profile.body,
                "example": example_message(&profile.body),
            }))
        })
        .collect()
}

async fn page(
    Extension(dash): Extension<chassis::Dashboard>,
    State(profiles): State<Arc<Profiles>>,
) -> Result<Html<String>, chassis::Error> {
    let entries = profile_entries(&profiles);
    let data = script_safe(
        &json!({
            "profiles": entries,
            "routes": { "preview": PREVIEW_ROUTE },
            "maxBytes": MAX_MESSAGE_BYTES,
        })
        .to_string(),
    );
    dash.render_project(
        PAGE_ROUTE,
        include_str!("../../web/translate.html"),
        json!({
            "data": data,
            "profiles": entries.iter().map(|e| e["name"].clone()).collect::<Vec<_>>(),
            "mount": WEB_MOUNT,
            "version": crate::dashboard::asset_version(),
        }),
    )
}
