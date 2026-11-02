//! Work limits for rendering a template, and the one place an engine is
//! built. A template is operator-written, but its input is whatever a
//! sender posted, and the dashboard preview renders both on the service's
//! own threads: without a ceiling a loop over a hostile payload is a way to
//! pin a core or fill memory.

use minijinja::{AutoEscape, Environment, ErrorKind, UndefinedBehavior};

use super::{describe, RenderError};

/// How many engine instructions one render may spend. Generous on
/// purpose: an Alertmanager template looping over a few hundred alerts
/// costs a few thousand, so only a runaway loop meets this.
pub const MAX_TEMPLATE_FUEL: u64 = 500_000;

/// The largest body a render may produce, in bytes. Matches the inbound
/// body cap: a translation that outgrows what may come in is a mistake
/// (a nested loop) rather than a message.
pub const MAX_RENDERED_BODY_BYTES: usize = 1024 * 1024;

/// The engine every render uses: strict undefined, fuel-limited, and JSON
/// autoescape when the profile delivers JSON. Shared with the dashboard
/// preview so it can never be more permissive than production.
pub fn template_env(json_profile: bool) -> Environment<'static> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(MAX_TEMPLATE_FUEL));
    if json_profile {
        env.set_auto_escape_callback(|_| AutoEscape::Json);
    } else {
        env.set_auto_escape_callback(|_| AutoEscape::None);
    }
    env
}

/// Map an engine error to ours; running out of fuel gets its own words.
pub(super) fn render_error(profile: &str, err: &minijinja::Error) -> RenderError {
    let detail = if err.kind() == ErrorKind::OutOfFuel {
        format!(
            "the template needed more than {MAX_TEMPLATE_FUEL} engine steps, which is the ceiling. \
             What now: remove the nested or unbounded loop — a template should read fields, not compute over the payload"
        )
    } else {
        describe(err)
    };
    RenderError::Render {
        profile: profile.to_string(),
        detail,
    }
}

/// Refuse a rendered body past the cap.
pub(super) fn check_body_size(profile: &str, body: &str) -> Result<(), RenderError> {
    if body.len() > MAX_RENDERED_BODY_BYTES {
        return Err(RenderError::Render {
            profile: profile.to_string(),
            detail: format!(
                "the rendered body is {} bytes, past the {MAX_RENDERED_BODY_BYTES}-byte ceiling. \
                 What now: shorten the template or stop copying whole arrays from the message into the body",
                body.len()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runaway_loop_runs_out_of_fuel_with_advice() {
        let env = template_env(false);
        let err = env
            .render_str(
                "{% for a in r %}{% for b in r %}{{ a }}{% endfor %}{% endfor %}",
                serde_json::json!({"r": (0..2000).collect::<Vec<_>>()}),
            )
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::OutOfFuel);
        let RenderError::Render { detail, .. } = render_error("p", &err) else {
            panic!("expected a render error");
        };
        assert!(detail.contains("What now"), "{detail}");
    }

    #[test]
    fn an_oversized_body_is_refused_after_rendering() {
        let big = "x".repeat(MAX_RENDERED_BODY_BYTES + 1);
        assert!(check_body_size("p", &big).is_err());
        assert!(check_body_size("p", "small").is_ok());
    }
}
