//! The files of the dashboard module (`web/`), embedded in the binary and
//! served by the kit's web app, and the version stamp that busts caches.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(super) const WEB_FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../../web/index.html")),
    ("dashboard.js", include_bytes!("../../web/dashboard.js")),
    ("dashboard.css", include_bytes!("../../web/dashboard.css")),
    ("history.js", include_bytes!("../../web/history.js")),
    ("history.css", include_bytes!("../../web/history.css")),
    ("profiles.js", include_bytes!("../../web/profiles.js")),
    ("profiles.css", include_bytes!("../../web/profiles.css")),
    ("health.js", include_bytes!("../../web/health.js")),
    ("health.css", include_bytes!("../../web/health.css")),
    ("reloads.js", include_bytes!("../../web/reloads.js")),
    ("reloads.css", include_bytes!("../../web/reloads.css")),
    ("translate.js", include_bytes!("../../web/translate.js")),
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
