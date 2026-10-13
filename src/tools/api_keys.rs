use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::flute_err_to_result;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiKeysList {
    /// Overrides the pinned FLUTE2_MERCHANT_ID. Optional here: with neither,
    /// the list is unfiltered.
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyCreate {
    /// Display name for the key.
    pub name: String,
    /// Overrides the pinned FLUTE2_MERCHANT_ID. Required here, from one source
    /// or the other.
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyRevoke {
    /// The key's client id.
    pub client_id: String,
}

#[tool_router(router = api_keys_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List merchant API keys. This endpoint takes an affiliate (partner) token: a merchant key carries a merchant claim and is refused 403, and the reason rides in the message rather than in `kind`. Uses `merchant_id`, else the pinned FLUTE2_MERCHANT_ID, else lists without a merchant filter. Not paginated. Safe to retry."
    )]
    pub async fn api_keys_list(
        &self,
        Parameters(p): Parameters<ApiKeysList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["api-keys".into(), "list".into()]);
        if let Some(m) = p
            .merchant_id
            .filter(|s| !s.is_empty())
            .or_else(|| self.config.merchant_id.clone())
        {
            args.extend(["--merchant-id".into(), m]);
        }
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Create a merchant API key. This endpoint takes an affiliate (partner) token: a merchant key carries a merchant claim and is refused 403, and the reason rides in the message rather than in `kind`. The response carries `clientSecret` ONCE — store it immediately, because the API never returns it again. Requires a merchant id, from `merchant_id` or FLUTE2_MERCHANT_ID. NOT safe to retry and it takes no reference id, so reconcile with api_keys_list before re-issuing — which a 403 also blocks, so an ambiguous create here cannot be reconciled at all."
    )]
    pub async fn api_keys_create(
        &self,
        Parameters(p): Parameters<ApiKeyCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("api_keys_create") {
            return Ok(blocked);
        }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend([
            "api-keys".into(),
            "create".into(),
            "--merchant-id".into(),
            merchant_id,
            "--name".into(),
            p.name,
        ]);
        Ok(self.run_tool(args).await)
    }

    #[tool(
        description = "Revoke a merchant API key by its client id. This endpoint takes an affiliate (partner) token: a merchant key carries a merchant claim and is refused 403, and the reason rides in the message rather than in `kind`. Takes no merchant id. A 404 is a success, so re-running is a safe no-op, but its confirmation carries `revoked: false, found: false`: the server answers alike for a key already revoked and one that never existed. `revoked: true` means this call revoked the key."
    )]
    pub async fn api_keys_revoke(
        &self,
        Parameters(p): Parameters<ApiKeyRevoke>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("api_keys_revoke") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "api-keys".into(),
            "revoke".into(),
            "--client-id".into(),
            p.client_id,
            "--yes".into(),
        ]);
        Ok(self.run_tool(args).await)
    }
}
