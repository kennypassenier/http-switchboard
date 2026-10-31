use super::*;

#[test]
fn k10_the_command_line_fails_closed_and_says_what_to_do() {
    // The runbook's procedures are exactly these verbs, so a regression
    // here lands in the emergency path (Phase 7, G7).
    let dir = tempdir("cli");

    // An unknown argument: the kit refuses with the usage, exit 1.
    let out = Command::new(binary()).arg("--frobnicate").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--help"));

    // A config file that does not exist.
    let out = Command::new(binary())
        .args(["--state-dir"])
        .arg(&dir)
        .arg("--config")
        .arg(dir.join("nowhere.toml"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(stderr.contains("nowhere.toml"), "{stderr}");

    // A listen address that is not one.
    let config = dir.join("ok.toml");
    std::fs::write(
        &config,
        r#"
[[profiles]]
name = "hook"
from = { http_path = "/hook" }
to = { url = "http://127.0.0.1:9/in" }
content_type = "application/json"
body = '''{"x": 1}'''
"#,
    )
    .unwrap();
    let out = Command::new(binary())
        .args(["--config"])
        .arg(&config)
        .args(["--state-dir"])
        .arg(&dir)
        .args(["--listen", "not-an-address"])
        .env("HTTP_SWITCHBOARD_TOKEN", TEST_TOKEN)
        .env("HTTP_SWITCHBOARD_SECRET_KEY", TEST_SECRET_KEY)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");

    // An unknown option on the test verb.
    let out = Command::new(binary())
        .args(["test", "--nonsense", "x"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nonsense"));
}
