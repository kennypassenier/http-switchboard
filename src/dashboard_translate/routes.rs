//! The page and the route it posts to. Thin transport: all the work is
//! `preview` and `example_message`.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde_json::{json, Value};
use tokio::sync::Semaphore;

use super::example::example_message;
use super::preview::{preview, Failure, Preview};
use super::scratch::{open, refused, with_template, ENVELOPE_TYPE};
use super::{MAX_MESSAGE_BYTES, MAX_TEMPLATE_BYTES, PAGE_ROUTE, PREVIEW_ROUTE};
use crate::config::Profile;
use crate::dashboard::{Profiles, WEB_MOUNT};

/// The longest one preview may take before it is answered with a failure.
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(5);

/// How many previews may render at once. A preview renders a draft template
/// on a blocking thread that cannot be cancelled, so the slot is held by
/// that thread until it ends, not by the request that gave up waiting.
const MAX_CONCURRENT_PREVIEWS: usize = 4;

static PREVIEW_SLOTS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_PREVIEWS);

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
        // Twice the caps: a message or draft over its cap reaches `preview`
        // and is refused in words; only something far larger gets the bare
        // 413. The envelope's JSON escaping is what the doubling absorbs.
        .layer(DefaultBodyLimit::max(
            (MAX_MESSAGE_BYTES + MAX_TEMPLATE_BYTES) * 2,
        ))
        .with_state(profiles);
    app.dashboard_routes(router);
}

fn json(status: StatusCode, body: String) -> Response {
    (status, [("content-type", "application/json")], body).into_response()
}

/// The answer for a refused preview: 413 for size, 400 for a request the
/// route cannot read, 429 when busy, 504 when it took too long, 503 when
/// the work itself died, 422 for the rest.
fn refusal(failure: &Failure) -> Response {
    let status = match failure.kind {
        "size" => StatusCode::PAYLOAD_TOO_LARGE,
        "request" => StatusCode::BAD_REQUEST,
        "busy" => StatusCode::TOO_MANY_REQUESTS,
        "timeout" => StatusCode::GATEWAY_TIMEOUT,
        "internal" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    json(status, serde_json::to_string(failure).unwrap_or_default())
}

fn unknown_profile(profiles: &Profiles, name: &str) -> Response {
    let names = profiles
        .snapshot()
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    json(
        StatusCode::NOT_FOUND,
        json!({
            "error": format!("there is no profile named '{name}'"),
            "remedy": format!("pick one of: {names}"),
        })
        .to_string(),
    )
}

/// The message and, when the page posts an envelope, the draft template.
fn request_of(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<(Vec<u8>, Option<String>), Box<Failure>> {
    let enveloped = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with(ENVELOPE_TYPE));
    if !enveloped {
        return Ok((body.to_vec(), None));
    }
    let envelope = open(body)?;
    Ok((envelope.message.into_bytes(), envelope.template))
}

async fn preview_route(
    State(profiles): State<Arc<Profiles>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(profile) = profiles.store().get(&name) else {
        return unknown_profile(&profiles, &name);
    };
    let (message, draft) = match request_of(&headers, &body) {
        Ok(parts) => parts,
        Err(failure) => return refusal(&failure),
    };
    match run_guarded(profile, message, draft).await {
        Ok(ok) => json(
            StatusCode::OK,
            serde_json::to_string(&ok).unwrap_or_default(),
        ),
        Err(failure) => refusal(&failure),
    }
}

/// Check the draft and render the preview on a blocking thread, under a
/// slot and a deadline. Every way this can go wrong is a [`Failure`] in
/// words, never a 500 and never a panic in the handler.
async fn run_guarded(
    profile: Arc<Profile>,
    message: Vec<u8>,
    draft: Option<String>,
) -> Result<Preview, Box<Failure>> {
    let Ok(slot) = PREVIEW_SLOTS.try_acquire() else {
        return Err(refused(
            "Too many previews are running",
            format!("{MAX_CONCURRENT_PREVIEWS} previews are already being worked out."),
            "Wait a moment and try again.",
            "busy",
        ));
    };
    let job = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let profile = match draft {
            Some(template) => Arc::new(with_template(&profile, &template)?),
            None => profile,
        };
        preview(&profile, &message)
    });
    match tokio::time::timeout(PREVIEW_TIMEOUT, job).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(refused(
            "The preview stopped unexpectedly",
            format!("The work of rendering it ended abnormally ({error})."),
            "Try again; if it keeps happening, simplify the template and report it.",
            "internal",
        )),
        Err(_) => Err(refused(
            "The preview took too long",
            format!(
                "Rendering did not finish within {} s, which usually means a loop or a very \
                 large expansion in the template.",
                PREVIEW_TIMEOUT.as_secs()
            ),
            "Simplify the loops in the template, or try a smaller message.",
            "timeout",
        )),
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
            "maxTemplateBytes": MAX_TEMPLATE_BYTES,
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
