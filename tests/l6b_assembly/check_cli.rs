use super::*;

#[test]
fn l6b_a_broken_config_stops_the_binary_with_a_remedy() {
    let dir = std::env::temp_dir().join(format!("hsw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.toml");
    std::fs::write(&path, "[[profiles]]\nname = \"a\"\n").unwrap();

    // 2.0.0: the kit's --check; the state dir is the kit's probe target.
    let out = std::process::Command::new(binary())
        .args(["--check", "--config"])
        .arg(&path)
        .arg("--state-dir")
        .arg(&dir)
        .output()
        .expect("the binary must be built");

    assert!(!out.status.success(), "a broken config must not pass");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(
        stderr.contains("broken.toml"),
        "the file is named: {stderr}"
    );
}

#[test]
fn l6b_the_shipped_example_config_passes_the_check() {
    // The config that ships with the project is checked by the binary
    // itself, not only by a unit test that happens to parse it.
    let state = std::env::temp_dir().join(format!("hsw-check-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let out = std::process::Command::new(binary())
        .args([
            "--check",
            "--config",
            "deploy/config.example.toml",
            "--state-dir",
        ])
        .arg(&state)
        .env("KYU_TOKEN", "vault-value")
        // 3.0.0 (H1): the dashboard is compiled in, so the kit refuses to
        // start or to --check without its two secrets. A test that starts
        // this service supplies them the way the env file on the host does.
        .env("HTTP_SWITCHBOARD_TOKEN", TEST_TOKEN)
        .env("HTTP_SWITCHBOARD_SECRET_KEY", TEST_SECRET_KEY)
        .output()
        .expect("the binary must be built");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("1 profile"), "{stdout}");
}

#[test]
fn l6b_without_the_two_secrets_the_check_refuses_and_says_which_command_makes_them() {
    // 3.0.0 (H1): the door is the kit's, so the kit's two secrets became
    // mandatory. The systemd unit runs `--check` as ExecStartPre, which
    // means a host whose env file lacks them does not start the service —
    // this test pins the message that tells the operator why.
    let state = std::env::temp_dir().join(format!("hsw-nosecrets-{}", std::process::id()));
    std::fs::create_dir_all(&state).unwrap();
    let out = std::process::Command::new(binary())
        .args([
            "--check",
            "--config",
            "deploy/config.example.toml",
            "--state-dir",
        ])
        .arg(&state)
        .env("KYU_TOKEN", "vault-value")
        .env_remove("HTTP_SWITCHBOARD_TOKEN")
        .env_remove("HTTP_SWITCHBOARD_SECRET_KEY")
        .output()
        .expect("the binary must be built");

    assert!(
        !out.status.success(),
        "it must not pass without the secrets"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(
        stderr.contains("gen-secret"),
        "the command is named: {stderr}"
    );
    assert!(
        stderr.contains("HTTP_SWITCHBOARD_TOKEN") && stderr.contains("HTTP_SWITCHBOARD_SECRET_KEY"),
        "both variables are named: {stderr}"
    );
}
