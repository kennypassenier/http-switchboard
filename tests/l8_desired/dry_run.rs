use super::*;

#[test]
fn w4_the_dry_run_shows_the_result_and_sends_nothing() {
    // Against the shipped config and the recorded Alertmanager payload,
    // so this also proves the two agree with each other.
    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            "deploy/config.example.toml",
            "--profile",
            "alertmanager",
            "--input",
            "tests/fixtures/alertmanager_firing.json",
        ])
        .env("KYU_TOKEN", "vault-value")
        .output()
        .expect("the binary must be built");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");

    let expected = std::fs::read_to_string("tests/fixtures/alertmanager_expected.json").unwrap();
    assert!(
        stdout.contains(expected.trim()),
        "the dry run must show exactly what the real path produces.\nexpected:\n{expected}\ngot:\n{stdout}"
    );
    assert!(stdout.contains("would send POST"), "{stdout}");
}

#[test]
fn w4_a_header_value_is_never_printed() {
    let dir = std::env::temp_dir().join(format!("hsw-l8-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("with-header.toml");
    std::fs::write(
        &config,
        r#"
[[profiles]]
name = "t"
from = { http_path = "/t" }
to = { url = "http://127.0.0.1:9/x" }
content_type = "application/json"
headers = { authorization = "Bearer do-not-print-me" }
body = '''{"ok": true}'''
"#,
    )
    .unwrap();
    let input = dir.join("in.json");
    std::fs::write(&input, "{}").unwrap();

    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "t",
            "--input",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{printed}");
    assert!(
        !printed.contains("do-not-print-me"),
        "a header value was printed: {printed}"
    );
    assert!(printed.contains("authorization: ***"), "{printed}");
}

#[test]
fn w4_an_unknown_profile_lists_the_ones_that_exist() {
    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            "deploy/config.example.toml",
            "--profile",
            "nope",
            "--input",
            "tests/fixtures/alertmanager_firing.json",
        ])
        .env("KYU_TOKEN", "vault-value")
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(stderr.contains("alertmanager"), "{stderr}");
}

#[test]
fn w4_the_dry_run_reads_a_real_config_and_not_only_the_example() {
    // fix-3, found 2026-09-10 while moving to kit 2.0.0. The operations
    // runbook tells an operator to run this verb against the DEPLOYED
    // config. A deployed config also carries the kit's own knobs and its
    // `[[notify.webhook]]` tables, and this verb runs before the kit parses
    // anything, so nothing was stripping them: the verb answered "not valid
    // TOML" on a file that is valid TOML and that the service itself starts
    // from happily.
    //
    // Placement: integration test — it needs the real binary and a real
    // file. Timing: full suite.
    let dir = std::env::temp_dir().join(format!("hsw-realconfig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    let example = std::fs::read_to_string("deploy/config.example.toml").unwrap();
    std::fs::write(
        &path,
        format!(
            "listen = \"0.0.0.0:8080\"\nlog = \"info\"\n\n\
             [[notify.webhook]]\nurl = \"http://127.0.0.1:9/hook\"\n\n{example}"
        ),
    )
    .unwrap();

    let out = std::process::Command::new(binary())
        .args(["test", "--config"])
        .arg(&path)
        .args([
            "--profile",
            "alertmanager",
            "--input",
            "tests/fixtures/alertmanager_firing.json",
        ])
        .env("KYU_TOKEN", "vault-value")
        .output()
        .expect("the binary must be built");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "the dry-run must read a config that carries the kit's own knobs.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("would send"), "{stdout}");
}
