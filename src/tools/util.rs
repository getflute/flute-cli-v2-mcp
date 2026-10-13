use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};

use crate::server::FluteServer;
use crate::tools::{Empty, flute_err_to_result, value_to_result};

#[tool_router(router = util_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "API health check. Pure read; safe to retry.")]
    pub async fn ping(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("ping".into());
        Ok(self.run_tool(args).await)
    }

    #[tool(description = "Print the CLI version and the active profile. Safe to retry.")]
    pub async fn version(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("version".into());
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Live authentication check for the active profile: the CLI calls the API, so `authenticated` is true only when the stored credentials actually round-trip, not merely that credentials exist. Answers a flat object rather than the `{data, meta, object}` envelope the other reads use: `{authenticated, profile}` plus `api_base_url` and `client_id` when the CLI reports them, and never the secret. `profile` echoes the profile pinned at server start, so it confirms what this instance is aimed at and not what the CLI resolved. Costs a network round-trip. Safe to retry."
    )]
    pub async fn auth_status(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["auth".into(), "status".into()]);
        Ok(match self.run_cli(args).await {
            Ok(v) => {
                let data = v.get("data");
                let field = |k: &str| data.and_then(|d| d.get(k));
                let authenticated = field("authenticated")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let mut out = serde_json::json!({
                    "authenticated": authenticated,
                    "profile": self.config.profile.as_cli_str(),
                });
                // `client_id` is null when no credentials are configured, so
                // forward only real strings rather than passing a null through.
                for key in ["api_base_url", "client_id"] {
                    if let Some(s) = field(key).and_then(|v| v.as_str()) {
                        out[key] = serde_json::Value::String(s.to_string());
                    }
                }
                value_to_result(out)
            }
            Err(e) => flute_err_to_result(e),
        })
    }
}
