//! What happened to one message for one profile, and the sender's answer.

use axum::http::StatusCode;
use axum::response::IntoResponse;

/// The result of one profile's handling of one request.
pub(super) enum Verdict {
    Delivered,
    /// The destination (or the network to it) did not take it; worth
    /// sending again once the cause is fixed.
    Failed(String),
    /// The body is not JSON. `detail` is the parser's position report
    /// ("expected value at line 1 column 1"), never the body itself.
    BadJson(String),
    /// The message cannot be used (the template cannot render it, or the
    /// destination refused it for good). `failure` is for the sender,
    /// `summary` the dashboard's one-liner.
    GaveUp {
        failure: String,
        summary: String,
    },
}

/// The sender's answer. AR4: with several profiles on one path, all must
/// succeed or the sender is told it failed — and the documented
/// consequence is that their retry re-delivers the branch that did succeed.
///
/// Malformed JSON is the sender's mistake and the same for every profile
/// on the path, so it is a 400 that says what is wrong, why it matters and
/// what to do; it never repeats the body.
pub(super) fn answer(verdicts: &[Verdict]) -> axum::response::Response {
    if let Some(detail) = verdicts.iter().find_map(|v| match v {
        Verdict::BadJson(detail) => Some(detail),
        _ => None,
    }) {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "not delivered: the body is not valid JSON ({detail}).\n\
                 Why: this path reads fields out of a JSON body, and a body in another \
                 format has none to read.\n\
                 What now: send a well-formed JSON body (and content-type: application/json); \
                 sending the same body again will be refused the same way.\n"
            ),
        )
            .into_response();
    }
    let failures: Vec<&str> = verdicts
        .iter()
        .filter_map(|v| match v {
            Verdict::Failed(text) | Verdict::GaveUp { failure: text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if failures.is_empty() {
        return (StatusCode::OK, "delivered\n").into_response();
    }
    (
        StatusCode::BAD_GATEWAY,
        format!(
            "not delivered.\n{}\nWhat now: this message was NOT accepted; send it again once \
             the cause above is addressed.\n",
            failures.join("\n")
        ),
    )
        .into_response()
}
