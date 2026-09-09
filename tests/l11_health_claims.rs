//! L11 — the documents carry the measured truth about the health probes
//! (correction C2, 2026-09-09).
//!
//! C1 taught that a documented *command* must be executed by a test. C2
//! was the same fault in prose: three documents described `/healthz` as
//! lenient liveness and `?strict=1` as strict readiness, a split the kit
//! removed two releases earlier — and the handover had turned that
//! description into an instruction to point a container healthcheck at
//! `/healthz`, which would restart this service every time the receiver
//! was down.
//!
//! Asserting "the prose is true" against prose is brittle, so this test
//! inverts it. The table below is **measured** against a running service,
//! and then the documents must contain that exact table between markers.
//! A document cannot drift from the binary, because the binary writes the
//! document's table.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use chassis::testing::TestApp;
use chassis::AppSpec;
use http_switchboard::app::App as Switchboard;
use http_switchboard::config;
use reqwest::Method;
use support::TestServer;

/// The documents that must carry the table.
const DOCUMENTED: [&str; 2] = ["README.md", "docs/OPERATIONS_RUNBOOK.md"];
const START: &str = "<!-- health-table:start -->";
const END: &str = "<!-- health-table:end -->";

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

async fn start(receiver: &str) -> TestApp {
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
body = '{{"x": {{{{ x }}}}}}'
"#
    );
    let cfg = config::load("t.toml", &text, &env(&[])).expect("config must load");
    let switchboard = Switchboard::from_config(cfg);
    let registry = Arc::clone(&switchboard.registry);
    let subsystems: Vec<http_switchboard::obs::ProfileSubsystem> = switchboard
        .config
        .profiles
        .iter()
        .map(|p| http_switchboard::obs::ProfileSubsystem::new(&p.name, Arc::clone(&registry)))
        .collect();
    let routes = switchboard.profile_router();
    let spec = AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    };
    TestApp::start_with(spec, Router::new(), move |app| {
        app.api_routes(routes);
        for s in subsystems {
            app.subsystem(s);
        }
    })
    .await
}

async fn status_of(app: &TestApp, path: &str) -> u16 {
    let (status, _) = TestApp::send_text(app.request(Method::GET, path)).await;
    status
}

/// `--healthcheck` against a live service: the exit code a container's
/// HEALTHCHECK and the systemd unit actually see.
///
/// It runs on a blocking thread on purpose. The service under test lives
/// in this test's own runtime, so a blocking `Command::output()` on the
/// runtime thread stops the very server it is probing — which reads as a
/// dead service and exit 1, in every state. That is what this test first
/// reported, and it would have contradicted a correct measurement taken
/// against a separate process.
async fn healthcheck_exit(url: &str) -> i32 {
    let url = url.to_string();
    let bin = binary();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(bin)
            .args(["--healthcheck", &url])
            .output()
            .expect("the binary must be built")
            .status
            .code()
            .unwrap_or(-1)
    })
    .await
    .expect("the probe must not panic")
}

fn render_table(working: &[(&str, String)], failing: &[(&str, String)]) -> String {
    let mut out =
        String::from("| Probe | Every profile working | One profile failing |\n|---|---|---|\n");
    for ((name, ok), (_, bad)) in working.iter().zip(failing) {
        out.push_str(&format!("| {name} | {ok} | {bad} |\n"));
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l11_the_documents_carry_the_health_table_the_service_actually_answers() {
    let receiver = TestServer::start(vec![500]).await;
    let mut app = start(&receiver.base_url).await;
    app.login().await;
    let sender = app.issue_client("probe", &[]).await;
    let base = app.base_url();

    // ── measured with every profile working (nothing tried yet) ────────
    let working = vec![
        (
            "`GET /healthz`",
            status_of(&app, "/healthz").await.to_string(),
        ),
        (
            "`GET /healthz?strict=1`",
            status_of(&app, "/healthz?strict=1").await.to_string(),
        ),
        (
            "`--healthcheck`",
            format!(
                "exit {}",
                healthcheck_exit(&format!("{base}/healthz")).await
            ),
        ),
    ];

    // ── drive one profile into failing, then measure again ─────────────
    let (status, _) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", &sender.token)
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert_eq!(status, 502, "the receiver must refuse, so a profile fails");

    let failing = vec![
        (
            "`GET /healthz`",
            status_of(&app, "/healthz").await.to_string(),
        ),
        (
            "`GET /healthz?strict=1`",
            status_of(&app, "/healthz?strict=1").await.to_string(),
        ),
        (
            "`--healthcheck`",
            format!(
                "exit {}",
                healthcheck_exit(&format!("{base}/healthz")).await
            ),
        ),
    ];

    let measured = render_table(&working, &failing);

    // The fault C2 exists for, pinned as an assertion rather than as
    // prose: the two paths answer the same, and the flag does not.
    assert_eq!(
        failing[0].1, failing[1].1,
        "the 1.x split is gone: /healthz and ?strict=1 must answer alike\n{measured}"
    );
    assert_eq!(
        failing[2].1, "exit 0",
        "--healthcheck must stay liveness: exit 0 while a profile fails\n{measured}"
    );

    // ── and the documents must carry exactly that table ────────────────
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for doc in DOCUMENTED {
        let text = std::fs::read_to_string(root.join(doc))
            .unwrap_or_else(|e| panic!("{doc} must be readable: {e}"));
        let start = text.find(START).unwrap_or_else(|| {
            panic!(
                "{doc} carries no health table. What now: paste this between \
                 {START} and {END}:\n\n{measured}"
            )
        });
        let end = text[start..]
            .find(END)
            .unwrap_or_else(|| panic!("{doc} opens the health table but never closes it ({END})"))
            + start;
        let block = text[start + START.len()..end].trim();
        assert_eq!(
            block,
            measured.trim(),
            "{doc}'s health table is not what the service answers. \
             What now: replace the block between the markers with the measured one above — \
             the binary is what ships."
        );
    }
}
