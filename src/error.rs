use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FluteError {
    #[error("api error (status {status}): {message}")]
    Api {
        status: u16,
        message: String,
        correlation_id: Option<String>,
    },
    #[error("transport error: {message}")]
    Transport { message: String },
    #[error("auth error: {message}")]
    Auth { message: String },
    #[error("decode error: {message}")]
    Decode { message: String },
    #[error("cli usage error: {message}")]
    Client { message: String },
    #[error("could not spawn flute2: {0}")]
    Spawn(String),
    #[error("flute2 timed out after {secs}s")]
    Timeout { secs: u64 },
    #[error("flute2 produced unparseable output (exit {exit_code})")]
    BadOutput {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
}

#[derive(Debug, Deserialize)]
struct Envelope {
    kind: String,
    message: String,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    correlation_id: Option<String>,
}

impl FluteError {
    /// Parse the CLI's failure envelope off stdout. `status` and
    /// `correlation_id` are present only on `kind: "api"`, and a body that is
    /// not one of the five known kinds is reported verbatim rather than
    /// guessed at.
    pub fn from_envelope_stdout(exit_code: i32, stdout: &str, stderr: &str) -> FluteError {
        let parsed: Result<Envelope, _> = serde_json::from_str(stdout.trim());
        let Ok(env) = parsed else {
            return FluteError::BadOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            };
        };
        match env.kind.as_str() {
            "api" => FluteError::Api {
                status: env.status.unwrap_or(0),
                message: env.message,
                correlation_id: env.correlation_id,
            },
            "transport" => FluteError::Transport {
                message: env.message,
            },
            "auth" => FluteError::Auth {
                message: env.message,
            },
            "decode" => FluteError::Decode {
                message: env.message,
            },
            "client" => FluteError::Client {
                message: field_names(&env.message),
            },
            _ => FluteError::BadOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            },
        }
    }
}

/// Rewrite the CLI's flag spellings in a `client` message as this server's
/// field names: `--capture-method <CAPTURE_METHOD>` becomes `capture_method`
/// and `--billing-*` becomes `billing_*`. Every tool field is its flag with `-`
/// read as `_`, so the rewrite needs no table.
fn field_names(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find("--") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let len = after
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '*'))
            .unwrap_or(after.len());
        if len == 0 || !after.starts_with(|c: char| c.is_ascii_lowercase()) {
            out.push_str("--");
            rest = after;
            continue;
        }
        out.push_str(&after[..len].replace('-', "_"));
        rest = &after[len..];
        // clap follows a flag with its value placeholder, ` <NAME>` or ` [<NAME>]`.
        for (open, close) in [(" <", '>'), (" [<", ']')] {
            if let Some(tail) = rest.strip_prefix(open)
                && let Some(end) = tail.find(close)
                && tail[..end]
                    .trim_end_matches('>')
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_')
            {
                rest = &tail[end + 1..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_api_envelope_with_all_fields() {
        let body = r#"{"kind":"api","message":"validation failed","status":422,"correlation_id":"abc-123"}"#;
        match FluteError::from_envelope_stdout(3, body, "") {
            FluteError::Api {
                status,
                message,
                correlation_id,
            } => {
                assert_eq!(status, 422);
                assert_eq!(message, "validation failed");
                assert_eq!(correlation_id.as_deref(), Some("abc-123"));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn parses_auth_envelope_without_status() {
        let body = r#"{"kind":"auth","message":"no credentials"}"#;
        assert!(matches!(
            FluteError::from_envelope_stdout(2, body, ""),
            FluteError::Auth { message } if message == "no credentials"
        ));
    }

    #[test]
    fn parses_transport_decode_and_client_envelopes() {
        for (kind, ctor) in [("transport", 1), ("decode", 1), ("client", 3)] {
            let body = format!(r#"{{"kind":"{kind}","message":"m"}}"#);
            let err = FluteError::from_envelope_stdout(ctor, &body, "");
            let named = match err {
                FluteError::Transport { .. } => "transport",
                FluteError::Decode { .. } => "decode",
                FluteError::Client { .. } => "client",
                other => panic!("expected {kind}, got {other:?}"),
            };
            assert_eq!(named, kind);
        }
    }

    #[test]
    fn unparseable_stdout_becomes_bad_output() {
        match FluteError::from_envelope_stdout(2, "not json", "stderr text") {
            FluteError::BadOutput {
                exit_code,
                stdout,
                stderr,
            } => {
                assert_eq!(exit_code, 2);
                assert_eq!(stdout, "not json");
                assert_eq!(stderr, "stderr text");
            }
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kind_becomes_bad_output() {
        let body = r#"{"kind":"martian","message":"x"}"#;
        assert!(matches!(
            FluteError::from_envelope_stdout(1, body, ""),
            FluteError::BadOutput { .. }
        ));
    }

    #[test]
    fn client_messages_name_fields_rather_than_flags() {
        for (cli, mcp) in [
            (
                "invalid value 'Manual' for '--capture-method <CAPTURE_METHOD>' [possible values: auto, manual]",
                "invalid value 'Manual' for 'capture_method' [possible values: auto, manual]",
            ),
            (
                "--card, --cvv and --exp must be supplied together",
                "card, cvv and exp must be supplied together",
            ),
            ("give every --billing-* flag", "give every billing_* flag"),
            (
                "invalid value 'x' for '--ach-faster-processing [<ACH_FASTER_PROCESSING>]'",
                "invalid value 'x' for 'ach_faster_processing'",
            ),
            (
                "--page-size must be between 1 and 100",
                "page_size must be between 1 and 100",
            ),
            ("a dash -- alone stays", "a dash -- alone stays"),
        ] {
            assert_eq!(field_names(cli), mcp);
        }
    }
}
