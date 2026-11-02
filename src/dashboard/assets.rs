//! The files of the dashboard module (`web/`), embedded in the binary and
//! served by the kit's web app, and the version stamp that busts caches.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(super) const WEB_FILES: &[(&str, &[u8])] = &[
    // The install icons (Kenny approved them 2026-11-01): the kit serves a
    // file of the web app at pwa/<name> in place of its generated letter icon
    // and lists it in the manifest. icon.svg has no manifest slot in the kit
    // yet; it is served with the rest of the web app.
    (
        "pwa/icon-180.png",
        include_bytes!("../../web/pwa/icon-180.png"),
    ),
    (
        "pwa/icon-192.png",
        include_bytes!("../../web/pwa/icon-192.png"),
    ),
    (
        "pwa/icon-512.png",
        include_bytes!("../../web/pwa/icon-512.png"),
    ),
    (
        "pwa/icon-maskable-192.png",
        include_bytes!("../../web/pwa/icon-maskable-192.png"),
    ),
    (
        "pwa/icon-maskable-512.png",
        include_bytes!("../../web/pwa/icon-maskable-512.png"),
    ),
    (
        "pwa/icon-monochrome-512.png",
        include_bytes!("../../web/pwa/icon-monochrome-512.png"),
    ),
    ("pwa/icon.svg", include_bytes!("../../web/pwa/icon.svg")),
    ("index.html", include_bytes!("../../web/index.html")),
    ("dashboard.js", include_bytes!("../../web/dashboard.js")),
    ("hsb-format.js", include_bytes!("../../web/hsb-format.js")),
    ("hsb-state.js", include_bytes!("../../web/hsb-state.js")),
    (
        "view-profiles.js",
        include_bytes!("../../web/view-profiles.js"),
    ),
    ("view-trace.js", include_bytes!("../../web/view-trace.js")),
    (
        "view-settings.js",
        include_bytes!("../../web/view-settings.js"),
    ),
    ("recheck.js", include_bytes!("../../web/recheck.js")),
    (
        "test-command.js",
        include_bytes!("../../web/test-command.js"),
    ),
    (
        "profile-pick.js",
        include_bytes!("../../web/profile-pick.js"),
    ),
    (
        "reloads-model.js",
        include_bytes!("../../web/reloads-model.js"),
    ),
    ("hsb-post.js", include_bytes!("../../web/hsb-post.js")),
    (
        "give-up-seen.js",
        include_bytes!("../../web/give-up-seen.js"),
    ),
    (
        "break-points.js",
        include_bytes!("../../web/break-points.js"),
    ),
    ("copy-text.js", include_bytes!("../../web/copy-text.js")),
    ("hub-verdict.js", include_bytes!("../../web/hub-verdict.js")),
    ("dashboard.css", include_bytes!("../../web/dashboard.css")),
    ("history.js", include_bytes!("../../web/history.js")),
    (
        "history-model.js",
        include_bytes!("../../web/history-model.js"),
    ),
    (
        "history-figures.js",
        include_bytes!("../../web/history-figures.js"),
    ),
    (
        "history-strips.js",
        include_bytes!("../../web/history-strips.js"),
    ),
    ("history.css", include_bytes!("../../web/history.css")),
    ("profiles.js", include_bytes!("../../web/profiles.js")),
    ("profiles.css", include_bytes!("../../web/profiles.css")),
    ("health.js", include_bytes!("../../web/health.js")),
    (
        "health-words.js",
        include_bytes!("../../web/health-words.js"),
    ),
    ("health-hub.js", include_bytes!("../../web/health-hub.js")),
    ("health.css", include_bytes!("../../web/health.css")),
    ("reloads.js", include_bytes!("../../web/reloads.js")),
    ("reloads.css", include_bytes!("../../web/reloads.css")),
    ("translate.js", include_bytes!("../../web/translate.js")),
    (
        "translate-diff.js",
        include_bytes!("../../web/translate-diff.js"),
    ),
    (
        "translate-dom.js",
        include_bytes!("../../web/translate-dom.js"),
    ),
    (
        "translate-fields.js",
        include_bytes!("../../web/translate-fields.js"),
    ),
    (
        "translate-run.js",
        include_bytes!("../../web/translate-run.js"),
    ),
    (
        "translate-states.js",
        include_bytes!("../../web/translate-states.js"),
    ),
    (
        "translate-toml.js",
        include_bytes!("../../web/translate-toml.js"),
    ),
    ("translate.css", include_bytes!("../../web/translate.css")),
];

/// The `?v=` of the module and the stylesheet: the kit serves a year of
/// `immutable` for a versioned URL, so the version has to move when the
/// bytes do, not only when the package version does.
pub(crate) fn asset_version() -> String {
    let mut hasher = DefaultHasher::new();
    for (name, bytes) in WEB_FILES {
        name.hash(&mut hasher);
        bytes.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}
