use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use flute2_mcp::error::FluteError;
use flute2_mcp::runner::{CliRunner, ProcessRunner};
use serde_json::json;
use tempfile::TempDir;

fn fake(dir: &TempDir, body: &str) -> PathBuf {
    let path = dir.path().join("flute2");
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn runner(bin: PathBuf, secs: u64) -> ProcessRunner {
    ProcessRunner {
        binary: bin,
        timeout: Duration::from_secs(secs),
        debug: false,
    }
}

#[tokio::test]
async fn parses_success_json() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf '{\"object\":\"ping\"}'\nexit 0\n");
    let out = runner(bin, 5).run(&["ping".into()]).await.unwrap();
    assert_eq!(out, json!({"object": "ping"}));
}

/// A decline is HTTP 200 and exit 0, so the runner must return it as success
/// and leave the caller to read `transactionStatus`.
#[tokio::test]
async fn declined_payment_is_ok_not_an_error() {
    let dir = TempDir::new().unwrap();
    let body =
        r#"{"object":"transaction","data":{"transactionId":"t1","transactionStatus":"Declined"}}"#;
    let bin = fake(&dir, &format!("#!/bin/sh\nprintf '%s' '{body}'\nexit 0\n"));
    let out = runner(bin, 5).run(&["x".into()]).await.unwrap();
    assert_eq!(out["data"]["transactionStatus"], "Declined");
}

#[tokio::test]
async fn empty_stdout_on_success_is_null() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nexit 0\n");
    let out = runner(bin, 5).run(&["x".into()]).await.unwrap();
    assert_eq!(out, json!(null));
}

#[tokio::test]
async fn api_envelope_on_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    let bin = fake(
        &dir,
        "#!/bin/sh\nprintf '{\"kind\":\"api\",\"message\":\"nope\",\"status\":404,\"correlation_id\":\"c-1\"}'\nexit 4\n",
    );
    match runner(bin, 5).run(&["x".into()]).await {
        Err(FluteError::Api {
            status,
            message,
            correlation_id,
        }) => {
            assert_eq!(status, 404);
            assert_eq!(message, "nope");
            assert_eq!(correlation_id.as_deref(), Some("c-1"));
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[tokio::test]
async fn unparseable_nonzero_is_bad_output() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf 'boom'\nexit 1\n");
    assert!(matches!(
        runner(bin, 5).run(&["x".into()]).await,
        Err(FluteError::BadOutput { exit_code: 1, .. })
    ));
}

#[tokio::test]
async fn missing_binary_is_spawn_error() {
    let r = runner(PathBuf::from("/no/such/flute2"), 5);
    assert!(matches!(
        r.run(&["x".into()]).await,
        Err(FluteError::Spawn(_))
    ));
}

#[tokio::test]
async fn hung_child_times_out() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nsleep 5\n");
    let r = ProcessRunner {
        binary: bin,
        timeout: Duration::from_millis(200),
        debug: false,
    };
    assert!(matches!(
        r.run(&["x".into()]).await,
        Err(FluteError::Timeout { .. })
    ));
}

/// The child inherits this server's environment, which is how
/// `FLUTE2_CLIENT_ID` and `FLUTE2_CLIENT_SECRET` reach the CLI without this
/// server ever reading them. `RUST_LOG` is the one name removed, and that strip
/// is asserted end to end in `e2e_stdio.rs`, where the server process itself is
/// launched with `RUST_LOG` set.
#[tokio::test]
async fn child_inherits_the_parent_environment() {
    let dir = TempDir::new().unwrap();
    let bin = fake(
        &dir,
        "#!/bin/sh\nprintf '{\"path\":\"%s\"}' \"$PATH\"\nexit 0\n",
    );
    let out = runner(bin, 5).run(&["x".into()]).await.unwrap();
    assert!(
        !out["path"].as_str().unwrap().is_empty(),
        "the child saw no PATH, so it inherited no environment"
    );
}

#[tokio::test]
async fn mock_records_calls_and_replays_responses() {
    use flute2_mcp::runner::MockRunner;
    let mock = MockRunner::new(vec![Ok(json!({"data": []}))]);
    let out = mock.run(&["a".into(), "b".into()]).await.unwrap();
    assert_eq!(out, json!({"data": []}));
    assert_eq!(mock.calls(), vec![vec!["a".to_string(), "b".to_string()]]);
}

/// An exhausted `MockRunner` names the argv it could not answer, and the
/// responses mutex stays usable so the next call reports the same diagnostic.
#[tokio::test]
async fn mock_without_a_response_panics_naming_the_argv() {
    use flute2_mcp::runner::MockRunner;
    use std::sync::Arc;

    let mock = MockRunner::new(vec![]);

    let first = Arc::clone(&mock);
    let panic = tokio::spawn(async move { first.run(&["charge".into(), "--id=t1".into()]).await })
        .await
        .expect_err("a mock with no canned responses panics")
        .into_panic();
    let message = panic
        .downcast_ref::<String>()
        .expect("the panic carries a formatted message");
    assert!(message.contains("no canned response"), "{message}");
    assert!(message.contains("charge"), "{message}");
    assert!(message.contains("--id=t1"), "{message}");

    let second = Arc::clone(&mock);
    let again = tokio::spawn(async move { second.run(&["ping".into()]).await })
        .await
        .expect_err("a mock with no canned responses panics")
        .into_panic();
    let message = again
        .downcast_ref::<String>()
        .expect("the panic carries a formatted message");
    assert!(message.contains("no canned response"), "{message}");

    assert_eq!(
        mock.calls(),
        vec![
            vec!["charge".to_string(), "--id=t1".to_string()],
            vec!["ping".to_string()],
        ]
    );
}
