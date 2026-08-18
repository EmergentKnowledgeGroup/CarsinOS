use std::process::Command;

#[test]
fn print_config_does_not_emit_the_configured_gateway_token_to_stdout_or_stderr() {
    let token = format!("test-only-gateway-token-{}", std::process::id());
    let state_canary = format!("state-path-secret-{}", std::process::id());
    let output = Command::new(env!("CARGO_BIN_EXE_carsinos-cli"))
        .arg("print-config")
        .env("CARSINOS_GATEWAY_TOKEN", &token)
        .env("CARSINOS_STATE_DIR", format!("Z:\\{state_canary}\\state"))
        .output()
        .expect("run carsinos print-config");
    assert!(output.status.success(), "print-config failed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stdout.contains("token_present=true"));
    assert!(stdout.contains("token_source=environment"));
    assert!(stdout.contains("state_dir_present=true"));
    assert!(!stdout.contains(&token));
    assert!(!stderr.contains(&token));
    assert!(!stdout.contains(&state_canary));
    assert!(!stderr.contains(&state_canary));
}

#[test]
fn print_config_errors_do_not_echo_invalid_secret_bearing_config_values() {
    let canary = format!("invalid-bind-secret-{}", std::process::id());
    let output = Command::new(env!("CARGO_BIN_EXE_carsinos-cli"))
        .arg("print-config")
        .env("CARSINOS_GATEWAY_BIND", &canary)
        .output()
        .expect("run carsinos print-config with invalid bind");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(&canary));
    assert!(!stderr.contains(&canary));
}
