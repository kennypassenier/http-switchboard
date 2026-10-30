use super::*;

#[tokio::test]
async fn feat_reload_1_a_valid_change_reaches_the_next_message() {
    let receiver = TestServer::start(vec![200]).await;
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    let outcome = reload::apply(&started, &store, load(&profile(&receiver.base_url, NEW)));
    assert_eq!(
        outcome,
        Outcome::Applied {
            changed: vec!["hook".to_string()]
        }
    );
    assert_eq!(post(&format!("{base}/hook")).await, 200);

    let bodies: Vec<String> = receiver.received().into_iter().map(|r| r.body).collect();
    assert_eq!(bodies, [r#"{"old": 7}"#, r#"{"new": 7}"#]);
}

#[tokio::test]
async fn feat_reload_1_a_broken_config_changes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    // A typo in a key: valid TOML, not a valid config.
    let broken = profile(&receiver.base_url, NEW).replace("timeout_ms", "timout_ms");
    let outcome = reload::apply(&started, &store, load(&broken));
    let Outcome::Invalid { reason } = &outcome else {
        panic!("expected Invalid, got {outcome:?}");
    };
    assert!(
        reason.contains("timout_ms"),
        "the reason names the key: {reason}"
    );
    assert!(reload::describe(&outcome).contains("unchanged"));

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    assert_eq!(receiver.received()[0].body, r#"{"old": 7}"#);
}

#[tokio::test]
async fn feat_reload_1_a_half_written_file_changes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let full = profile(&receiver.base_url, NEW);
    let started = load(&profile(&receiver.base_url, OLD)).unwrap();
    let store = Arc::new(ProfileStore::new(&started));
    let base = serve(&started, Arc::clone(&store)).await;

    // An editor caught mid-save: every cut of the new file must be refused
    // or be the old config, never a mixture.
    for cut in (1..full.len()).step_by(7) {
        let Some(half) = full.get(..cut) else {
            continue;
        };
        let outcome = reload::apply(&started, &store, load(half));
        assert!(
            matches!(
                outcome,
                Outcome::Invalid { .. } | Outcome::NeedsRestart { .. }
            ),
            "a file cut at byte {cut} was applied: {outcome:?}"
        );
    }

    assert_eq!(post(&format!("{base}/hook")).await, 200);
    assert_eq!(receiver.received()[0].body, r#"{"old": 7}"#);
}

#[test]
fn feat_reload_1_a_change_of_shape_asks_for_a_restart() {
    let started = load(&profile("http://127.0.0.1:1", OLD)).unwrap();
    let store = ProfileStore::new(&started);
    let cases = [
        (
            "a second profile",
            format!(
                "{}{}",
                profile("http://127.0.0.1:1", OLD),
                profile("http://127.0.0.1:1", OLD).replace("\"hook\"", "\"other\"")
            ),
        ),
        (
            "a renamed profile",
            profile("http://127.0.0.1:1", OLD).replace("\"hook\"", "\"renamed\""),
        ),
        (
            "a new path",
            profile("http://127.0.0.1:1", OLD).replace("\"/hook\"", "\"/elsewhere\""),
        ),
    ];
    for (what, text) in cases {
        let outcome = reload::apply(&started, &store, load(&text));
        let Outcome::NeedsRestart { reason } = &outcome else {
            panic!("{what}: expected NeedsRestart, got {outcome:?}");
        };
        assert!(
            reload::describe(&outcome).contains("systemctl restart"),
            "{what}: {reason}"
        );
        assert_eq!(
            store.get("hook").unwrap().body,
            OLD,
            "{what} changed the store"
        );
    }
}

#[test]
fn feat_reload_1_an_unchanged_file_reports_nothing_changed() {
    let text = profile("http://127.0.0.1:1", OLD);
    let started = load(&text).unwrap();
    let store = ProfileStore::new(&started);
    let outcome = reload::apply(&started, &store, load(&text));
    assert_eq!(outcome, Outcome::Applied { changed: vec![] });
    assert_eq!(
        reload::describe(&outcome),
        "config reloaded: nothing changed"
    );
}

#[test]
fn feat_reload_1_every_outcome_is_counted_in_metrics() {
    let registry = Registry::new();
    registry.record_reload("applied");
    registry.record_reload("invalid");
    registry.record_reload("invalid");
    let metrics = registry.metrics();
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="applied"} 1"#));
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="needs-restart"} 0"#));
    assert!(metrics.contains(r#"switchboard_config_reloads_total{outcome="invalid"} 2"#));
}
