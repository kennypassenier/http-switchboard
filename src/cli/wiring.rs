//! Hooking the switchboard into the kit's `App`: subsystems, routes,
//! dashboard, browser preferences and the start/flush lifecycle.

use std::sync::Arc;

use chassis::storage::{AppDbSpec, BrowserKeys};
use chassis::App;
use http_switchboard::app::App as Switchboard;
use http_switchboard::config::Source;
use http_switchboard::obs::{ProfileSubsystem, RegistryMetrics};

/// Health, metrics, inbound routes, vocabulary and dashboard.
pub(crate) fn wire_service(app: &mut App, switchboard: &Switchboard) {
    for profile in &switchboard.config.profiles {
        app.subsystem(ProfileSubsystem::new(
            &profile.name,
            Arc::clone(&switchboard.registry),
        ));
        if let Source::Http { path } = &profile.source {
            // A delivery may legitimately take longer than the kit's request
            // timeout (retries × timeout_ms + settle); the sender waits for
            // the real answer (W1), so the kit's clock stays out of it.
            app.exempt_from_timeout(path.clone());
        }
    }
    app.metrics_source(RegistryMetrics(Arc::clone(&switchboard.registry)));
    // H1 (3.0.0): the profile paths are API routes, so the kit checks an
    // `Authorization: Bearer <client token>` before this service sees the
    // body, and revoking a sender's token locks it out the same second.
    // `chassis clients issue <sender> --url … --token-env …` mints one
    // without opening the dashboard.
    app.api_routes(switchboard.profile_router());
    // V1: what a "client" is called here. Presentation only — the routes,
    // the JSON keys and the log fields keep saying `client`.
    app.vocabulary("sender", "senders");
    // The four status sections, their routes and the module that draws
    // them (kp-themes 11 views): one call, shared with the door tests.
    http_switchboard::dashboard::install(
        app,
        Arc::new(http_switchboard::dashboard::Profiles::new(
            &switchboard.config,
            Arc::clone(&switchboard.profiles),
            Arc::clone(&switchboard.registry),
        )),
    );
}

/// chassis-rs 3.12: the reader's own browser choices follow them across
/// devices. The preferences are a table of the app database, which has no
/// tables of its own here. Only user choices are declared, never a
/// secret: the profiles View, the Last 24 hours tiles and press choice,
/// and every `kp-remember:` choice kp-themes keeps (the refresh chooser's
/// interval, the matrix view, a table's sort and columns).
pub(crate) fn wire_browser_prefs(app: &mut App) -> Result<(), String> {
    app.app_db(AppDbSpec::new(Vec::new()))
        .map_err(|e| e.to_string())?;
    app.sync_browser_keys(
        BrowserKeys::new()
            .exact("hsb-profiles-view")
            .exact("hsb-history-tiles")
            .exact("hsb-history-press")
            .prefix("kp-remember:"),
    );
    Ok(())
}

/// Start the pumps after the bind and stop them in the shutdown window.
pub(crate) fn wire_lifecycle(app: &mut App, switchboard: Arc<Switchboard>) {
    let stop: Arc<std::sync::Mutex<Option<tokio::sync::broadcast::Sender<()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    {
        let switchboard = Arc::clone(&switchboard);
        let stop = Arc::clone(&stop);
        app.on_start(move || {
            let tx = switchboard.spawn_pumps();
            *stop.lock().expect("stop lock") = Some(tx);
        });
    }
    app.on_flush(move || {
        // Stopping accepting is the shutdown; the pumps hold nothing, so
        // being cut off mid-poll costs at worst a duplicate (S3).
        if let Some(tx) = stop.lock().expect("stop lock").take() {
            // `send` only errors when no pump is listening, in which case
            // there is nothing to stop.
            let _ = tx.send(());
        }
    });
}
