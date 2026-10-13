use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::FluteError;

#[async_trait]
pub trait CliRunner: Send + Sync {
    /// Run `flute2` with these argv tokens; the binary path belongs to the
    /// runner. Success returns the parsed stdout envelope, failure a
    /// `FluteError` carrying the envelope's fields when there were any.
    async fn run(&self, args: &[String]) -> Result<Value, FluteError>;
}

/// Test double. Records every `run` call and replays canned responses in order.
pub struct MockRunner {
    pub calls: Mutex<Vec<Vec<String>>>,
    pub responses: Mutex<Vec<Result<Value, FluteError>>>,
}

impl MockRunner {
    pub fn new(responses: Vec<Result<Value, FluteError>>) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        })
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl CliRunner for MockRunner {
    async fn run(&self, args: &[String]) -> Result<Value, FluteError> {
        self.calls.lock().unwrap().push(args.to_vec());
        let next = {
            let mut responses = self.responses.lock().unwrap();
            (!responses.is_empty()).then(|| responses.remove(0))
        };
        match next {
            Some(r) => r,
            None => panic!("MockRunner: no canned response for call {args:?}"),
        }
    }
}

pub struct ProcessRunner {
    pub binary: PathBuf,
    pub timeout: Duration,
    pub debug: bool,
}

#[async_trait]
impl CliRunner for ProcessRunner {
    async fn run(&self, args: &[String]) -> Result<Value, FluteError> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
            .env_remove("RUST_LOG")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| FluteError::Spawn(format!("{}: {}", self.binary.display(), e)))?;

        let output = match timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(FluteError::Spawn(e.to_string())),
            Err(_) => {
                return Err(FluteError::Timeout {
                    secs: self.timeout.as_secs(),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if self.debug && !stderr.is_empty() {
            tracing::debug!(target: "flute2_mcp::runner", "flute2 stderr: {stderr}");
        }

        let exit_code = output.status.code().unwrap_or(-1);

        // Success is decided by exit status, never by the body: the CLI exits 0
        // for a declined payment and for a 404 on the four delete and revoke
        // verbs.
        if output.status.success() {
            if stdout.trim().is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str(&stdout).map_err(|e| FluteError::Decode {
                message: format!("could not parse stdout as JSON: {e}"),
            })
        } else {
            Err(FluteError::from_envelope_stdout(
                exit_code, &stdout, &stderr,
            ))
        }
    }
}
