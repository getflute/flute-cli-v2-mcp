use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::error::FluteError;

pub mod address;
pub mod api_keys;
pub mod customers;
pub mod payment_links;
pub mod payment_methods;
pub mod payment_sessions;
pub mod pos;
pub mod settings;
pub mod settlements;
pub mod terminals;
pub mod transactions;
pub mod util;

/// `de_flexible_u32` for `page_size`, refusing a value outside 1–100 so a call
/// that breaks the schema's range is refused before the CLI runs.
pub fn de_page_size<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    match de_flexible_u32(deserializer)? {
        Some(n) if !(1..=100).contains(&n) => Err(serde::de::Error::custom(format!(
            "page_size must be between 1 and 100 (got {n})"
        ))),
        v => Ok(v),
    }
}

/// Deserialize an optional `u32` that may arrive as a JSON number **or** a
/// numeric string. MCP clients routinely send integers quoted; without this,
/// serde rejects them with `invalid type: string "..", expected u32`, which
/// reaches the client as JSON-RPC `-32602`. A non-numeric string is still an
/// error. A field using this must also carry `#[serde(default)]`, because
/// `deserialize_with` does not run for an absent key.
pub fn de_flexible_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    let value = Option::<NumOrStr>::deserialize(deserializer).map_err(|_| {
        serde::de::Error::custom("expected a non-negative integer, as a number or a string")
    })?;
    match value {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => {
            let trimmed = s.trim();
            trimmed.parse::<u32>().map(Some).map_err(|_| {
                serde::de::Error::custom(format!("expected an integer, got string {trimmed:?}"))
            })
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// A bare resource id, for the verbs that take one positionally.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Id {
    /// The resource id.
    pub id: String,
}

/// A bare resource id plus an idempotency key, for the keyed writes that take
/// their id positionally.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyedId {
    /// The resource id.
    pub id: String,
    /// Sent as `Idempotency-Key`. Pass a fresh one, e.g. a UUID, on the
    /// first attempt; re-sending the same key with the same arguments returns
    /// the first result instead of repeating the operation. The same key with
    /// different arguments answers 422, and a key is not scoped to one tool,
    /// so use a new key for each distinct operation. At most 255 printable
    /// ASCII characters.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// `--flag value`, omitted when the value is absent.
pub fn opt(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(v) = value {
        args.push(flag.to_string());
        args.push(v);
    }
}

/// `--flag 25`, omitted when the value is absent.
pub fn opt_num(args: &mut Vec<String>, flag: &str, value: Option<u32>) {
    if let Some(v) = value {
        args.push(flag.to_string());
        args.push(v.to_string());
    }
}

/// A bare `--flag`, pushed only for `Some(true)`. `Some(false)` and `None` both
/// push nothing: on the `create` verbs the flag has no value form, so the only
/// way to say "off" is to leave it out.
pub fn switch(args: &mut Vec<String>, flag: &str, value: Option<bool>) {
    if value == Some(true) {
        args.push(flag.to_string());
    }
}

/// `--flag true` or `--flag false`, for the update verbs where the flag takes an
/// optional value and `false` is a distinct instruction rather than silence.
pub fn toggle(args: &mut Vec<String>, flag: &str, value: Option<bool>) {
    if let Some(v) = value {
        args.push(flag.to_string());
        args.push(v.to_string());
    }
}

/// `--flag a --flag b`, for the flags the CLI accepts more than once.
pub fn repeat(args: &mut Vec<String>, flag: &str, values: Vec<String>) {
    for v in values {
        args.push(flag.to_string());
        args.push(v);
    }
}

/// The pagination and sort flags every collection read shares. A list the CLI
/// gives only one direction flag passes `None` for the other.
pub struct PageArgs {
    pub page_index: Option<u32>,
    pub page_size: Option<u32>,
    pub all: Option<bool>,
    pub sort_by: Option<String>,
    pub asc: Option<bool>,
    pub desc: Option<bool>,
}

impl PageArgs {
    pub fn push(self, args: &mut Vec<String>) {
        opt_num(args, "--page-index", self.page_index);
        opt_num(args, "--page-size", self.page_size);
        switch(args, "--all", self.all);
        opt(args, "--sort-by", self.sort_by);
        switch(args, "--asc", self.asc);
        switch(args, "--desc", self.desc);
    }
}

pub(crate) fn value_to_result(value: Value) -> CallToolResult {
    CallToolResult::success(vec![
        Content::json(value).expect("serde_json::Value is always JSON-serializable"),
    ])
}

/// Cap raw CLI output at 4 KiB on a UTF-8 boundary before embedding it in an
/// error payload, so a non-JSON failure body cannot balloon the response or
/// carry a large amount of unexpected content back to the client. The second
/// element of the pair is whether the input was cut.
fn truncate_4k(s: &str) -> (&str, bool) {
    const MAX: usize = 4096;
    if s.len() <= MAX {
        return (s, false);
    }
    let mut end = MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], true)
}

pub(crate) fn flute_err_to_result(err: FluteError) -> CallToolResult {
    let payload = match &err {
        FluteError::Api {
            status,
            message,
            correlation_id,
        } => serde_json::json!({
            "kind": "api", "status": status, "message": message, "correlation_id": correlation_id,
        }),
        FluteError::Transport { message } => {
            serde_json::json!({ "kind": "transport", "message": message })
        }
        FluteError::Auth { message } => serde_json::json!({
            "kind": "auth",
            "message": format!(
                "{message} — set FLUTE2_CLIENT_ID and FLUTE2_CLIENT_SECRET in the server's \
                 environment, or run `flute2 auth login`"
            ),
        }),
        FluteError::Decode { message } => {
            serde_json::json!({ "kind": "decode", "message": message })
        }
        FluteError::Client { message } => {
            serde_json::json!({ "kind": "client", "message": message })
        }
        FluteError::Spawn(msg) => serde_json::json!({
            "kind": "spawn",
            "message": format!("could not spawn flute2 — set FLUTE2_BIN or install the CLI ({msg})"),
        }),
        FluteError::Timeout { secs } => serde_json::json!({
            "kind": "timeout", "message": format!("flute2 timed out after {secs}s"),
        }),
        FluteError::BadOutput {
            exit_code,
            stdout,
            stderr,
        } => {
            let (stdout, stdout_truncated) = truncate_4k(stdout);
            let (stderr, stderr_truncated) = truncate_4k(stderr);
            serde_json::json!({
                "kind": "bad_output",
                "message": err.to_string(),
                "exit_code": exit_code,
                "stdout": stdout,
                "stdout_truncated": stdout_truncated,
                "stderr": stderr,
                "stderr_truncated": stderr_truncated,
            })
        }
    };
    CallToolResult::error(vec![
        Content::json(payload).expect("serde_json::Value is always JSON-serializable"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_4k_passes_short_input_through() {
        assert_eq!(truncate_4k("hello"), ("hello", false));
        assert_eq!(truncate_4k(""), ("", false));
        assert!(!truncate_4k(&"x".repeat(4096)).1);
    }

    #[test]
    fn truncate_4k_caps_long_ascii_at_4096() {
        let big = "x".repeat(5000);
        let (out, truncated) = truncate_4k(&big);
        assert_eq!(out.len(), 4096);
        assert!(truncated);
    }

    #[test]
    fn truncate_4k_respects_utf8_boundary() {
        let mut s = "a".repeat(4095);
        s.push('é');
        let (out, truncated) = truncate_4k(&s);
        assert_eq!(out.len(), 4095);
        assert!(s.is_char_boundary(out.len()));
        assert!(truncated);
    }

    #[test]
    fn switch_omits_false_and_toggle_emits_it() {
        let mut a = Vec::new();
        switch(&mut a, "--all", Some(false));
        assert!(a.is_empty());
        let mut b = Vec::new();
        toggle(&mut b, "--sms-consent", Some(false));
        assert_eq!(b, vec!["--sms-consent", "false"]);
    }

    #[test]
    fn repeat_emits_the_flag_once_per_value() {
        let mut a = Vec::new();
        repeat(
            &mut a,
            "--clear",
            vec!["amount".into(), "description".into()],
        );
        assert_eq!(a, vec!["--clear", "amount", "--clear", "description"]);
    }
}
