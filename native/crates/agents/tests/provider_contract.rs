#[test]
fn both_provider_contracts_in_isolated_child() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_axe-provider-fixture"))
        .arg("--contract-test")
        .env("ANTHROPIC_API_KEY", "authored-fixture-not-a-secret")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Both provider contracts passed"));
}
