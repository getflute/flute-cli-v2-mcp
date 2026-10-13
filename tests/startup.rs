use assert_cmd::Command;

/// A configuration error must be fatal at startup, not a surprise on the first
/// tool call, and must exit 2 rather than 1 so a supervisor can tell a
/// misconfiguration from a crash.
#[test]
fn an_unusable_binary_exits_two_before_serving() {
    let out = Command::cargo_bin("flute2-mcp")
        .unwrap()
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("FLUTE2_BIN", "/nope/does/not/exist")
        .env("RUST_LOG", "warn")
        .output()
        .expect("run server");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLUTE2_BIN"),
        "stderr must name the variable at fault: {stderr}"
    );
}

#[test]
fn an_unknown_profile_exits_two() {
    let out = Command::cargo_bin("flute2-mcp")
        .unwrap()
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("FLUTE2_BIN", "/bin/sh")
        .env("FLUTE2_PROFILE", "staging")
        .env("RUST_LOG", "warn")
        .output()
        .expect("run server");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("FLUTE2_PROFILE"),
        "stderr must name the variable at fault"
    );
}

/// `--version` and `-V` print a version and exit 0, rather than the parser's
/// two-line "unexpected argument" refusal.
#[test]
fn flag_version_prints_a_version_and_exits_zero() {
    for flag in ["--version", "-V"] {
        let out = Command::cargo_bin("flute2-mcp")
            .unwrap()
            .env_clear()
            .arg(flag)
            .output()
            .expect("run server");
        assert_eq!(out.status.code(), Some(0), "{flag} must exit 0");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")),
            "{flag} must print the crate version"
        );
    }
}
