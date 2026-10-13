use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, JsonObject, ServerCapabilities, ServerInfo},
    tool_handler,
};
use serde_json::Value;

use crate::config::{Config, Profile};
use crate::error::FluteError;
use crate::runner::CliRunner;
use crate::tools::{flute_err_to_result, value_to_result};

#[derive(Clone)]
pub struct FluteServer {
    pub(crate) config: Arc<Config>,
    pub(crate) runner: Arc<dyn CliRunner>,
    tool_router: ToolRouter<FluteServer>,
}

impl FluteServer {
    pub fn new(config: Arc<Config>, runner: Arc<dyn CliRunner>) -> Self {
        // Each group module contributes a router through
        // `#[tool_router(router = …)]`, and this sum is the whole tool surface.
        let tool_router = Self::util_router()
            + Self::customers_router()
            + Self::payment_methods_router()
            + Self::transactions_router()
            + Self::payment_links_router()
            + Self::payment_sessions_router()
            + Self::pos_router()
            + Self::terminals_router()
            + Self::settlements_router()
            + Self::settings_router()
            + Self::api_keys_router();
        Self {
            config,
            runner,
            tool_router,
        }
    }

    /// The flags every invocation leads with. `--output json` is passed
    /// explicitly so neither `FLUTE2_OUTPUT` nor the CLI's config file can
    /// change the shape of what comes back.
    pub fn base_args(&self) -> Vec<String> {
        vec![
            "--profile".into(),
            self.config.profile.as_cli_str().into(),
            "--output".into(),
            "json".into(),
        ]
    }

    pub async fn run_cli(&self, args: Vec<String>) -> Result<Value, FluteError> {
        self.runner.run(&args).await
    }

    /// Run the CLI and forward its envelope unchanged, or its error as a
    /// structured `isError` payload. Every tool but `auth_status` ends here.
    pub async fn run_tool(&self, args: Vec<String>) -> rmcp::model::CallToolResult {
        match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        }
    }

    /// `Some(error_result)` when a write must be refused on a guarded
    /// production instance, `None` when it may proceed. Returning early here is
    /// what keeps the CLI unspawned.
    pub fn guard_write(&self, tool: &str) -> Option<rmcp::model::CallToolResult> {
        if self.config.profile == Profile::Production && !self.config.allow_prod_writes {
            Some(flute_err_to_result(FluteError::Client {
                message: format!(
                    "refusing {tool} on production profile; set \
                     FLUTE2_MCP_ALLOW_PROD_WRITES=1 to enable writes"
                ),
            }))
        } else {
            None
        }
    }

    /// Resolve the merchant id for the API-key tools: the per-call override,
    /// else the pinned `FLUTE2_MERCHANT_ID`, else a `client` error.
    pub fn merchant_id_for(&self, override_id: Option<String>) -> Result<String, FluteError> {
        override_id
            .filter(|s| !s.is_empty())
            .or_else(|| self.config.merchant_id.clone())
            .ok_or_else(|| FluteError::Client {
                message: "merchant_id required: pass `merchant_id` or set FLUTE2_MERCHANT_ID"
                    .into(),
            })
    }
}

/// Whether the child's argument parser reads this value as an option rather
/// than as data. clap reads a token opening with `-` as a flag, and `flute2`
/// declares `--profile` and `--output` as globals whose later occurrence wins,
/// so such a value can point the child at another API or at an output shape
/// this server does not parse. A value that parses as a number is exempt, so it
/// reaches the CLI's own argument parser and is answered there.
fn reads_as_option(value: &str) -> bool {
    value.starts_with('-') && value.parse::<f64>().is_err()
}

/// Refuse the call when any string in the caller's arguments, at any depth,
/// would reach the CLI as an option. The argv cannot tell a caller's value from
/// a flag the server pushed, so the screen sits above the argv assembly and
/// covers every tool at once.
pub fn screen_arguments(arguments: &JsonObject) -> Result<(), FluteError> {
    fn walk(field: &str, value: &Value) -> Result<(), FluteError> {
        match value {
            Value::String(s) if reads_as_option(s) => Err(FluteError::Client {
                message: format!(
                    "refusing `{field}`: the value {s:?} reaches the flute2 command line as an \
                     option rather than as data"
                ),
            }),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .try_for_each(|(i, item)| walk(&format!("{field}[{i}]"), item)),
            Value::Object(fields) => fields
                .iter()
                .try_for_each(|(key, item)| walk(&format!("{field}.{key}"), item)),
            _ => Ok(()),
        }
    }
    arguments.iter().try_for_each(|(field, v)| walk(field, v))
}

const DESERIALIZE_PREFIX: &str = "failed to deserialize parameters:";

/// serde's type error carries no field name, so a refused argument reads
/// `invalid type: floating point `1.0`, expected a string`. Name the first
/// argument whose JSON type the tool's input schema does not allow.
fn name_mistyped_field(
    result: rmcp::model::CallToolResult,
    schema: &JsonObject,
    arguments: &JsonObject,
) -> rmcp::model::CallToolResult {
    let text = match result.content.first().and_then(|c| c.as_text()) {
        Some(t) if result.is_error == Some(true) && t.text.starts_with(DESERIALIZE_PREFIX) => {
            t.text.clone()
        }
        _ => return result,
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    let mistyped = arguments.iter().find(|(key, value)| {
        let Some(property) = properties.and_then(|p| p.get(key.as_str())) else {
            return false;
        };
        let below_minimum = property
            .get("minimum")
            .and_then(Value::as_f64)
            .zip(value.as_f64())
            .is_some_and(|(min, v)| v < min);
        let Some(types) = property.get("type") else {
            return below_minimum;
        };
        let allowed = |t: &str| match types {
            Value::String(s) => s == t,
            Value::Array(a) => a.iter().any(|x| x == t),
            _ => true,
        };
        below_minimum
            || !match value {
                Value::Null => allowed("null"),
                Value::Bool(_) => allowed("boolean"),
                Value::Number(n) => {
                    allowed("number") || (n.is_u64() || n.is_i64()) && allowed("integer")
                }
                Value::String(s) => {
                    allowed("string") || allowed("integer") && s.trim().parse::<u64>().is_ok()
                }
                Value::Array(_) => allowed("array"),
                Value::Object(_) => allowed("object"),
            }
    });
    let Some((field, _)) = mistyped else {
        return result;
    };
    let detail = text[DESERIALIZE_PREFIX.len()..].trim_start();
    rmcp::model::CallToolResult::error(vec![rmcp::model::Content::text(format!(
        "{DESERIALIZE_PREFIX} `{field}`: {detail}"
    ))])
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FluteServer {
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let name = request.name.clone();
        let refusal = request
            .arguments
            .as_ref()
            .and_then(|arguments| screen_arguments(arguments).err());
        let result = match refusal {
            Some(e) => Ok(flute_err_to_result(e)),
            None => {
                let arguments = request.arguments.clone();
                let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
                self.tool_router.call(tcc).await.map(|r| {
                    match (self.tool_router.get(&name), arguments.as_ref()) {
                        (Some(tool), Some(arguments)) => {
                            name_mistyped_field(r, &tool.input_schema, arguments)
                        }
                        _ => r,
                    }
                })
            }
        };
        // The tool and its outcome, and no argument: a card number, an expiry
        // and a CVV all arrive as arguments.
        tracing::info!(
            tool = %name,
            is_error = result.as_ref().ok().and_then(|r| r.is_error).unwrap_or(false),
            "tool call"
        );
        result
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        let sandbox_note = if self.config.profile == Profile::Sandbox {
            " On sandbox, a card charge's billing address goes through address verification \
             when the merchant has it on: realistic addresses have declined with AVS `N` \
             (`declineDetails.code: \"AVS\"`), which makes one a way to test a decline, \
             while 123 Test St, New York, NY 10001, US has passed."
        } else {
            ""
        };
        info.with_instructions(format!(
            "{}{sandbox_note}",
            "Drives the `flute2` CLI against the Flute v2 API. Three rules govern every \
             result. (1) A declined payment is a SUCCESS, not an error: the API answers a \
             decline with HTTP 200, so read `data.transactionStatus` to learn whether money \
             moved — an exit code never tells you. (2) A failure the CLI reports is a JSON \
             envelope `{kind, message, status?, correlation_id?}` returned with isError \
             set; branch on `kind` first and then on `status`. A call that violates a \
             tool's own schema — a missing required field, an unknown field — is refused \
             before the CLI runs and answers a plain message with no `kind`. \
             (3) Writes are not safe to retry unless they carry `idempotency_key`. On every \
             tool that takes one, pass a fresh key (a UUID) on the first attempt; after an \
             ambiguous failure (`transport`, `timeout`, or a 5xx) re-send the SAME key with \
             the same arguments, and the API returns the first response instead of acting \
             twice — the response as it was, an error included, so read the resource to \
             learn its current state. A 409 means the request was refused for the \
             resource's current state (already captured or voided, declined, an ACH sale in \
             progress, or the same key still in flight) and is not proof that anything \
             happened: read the resource, and do not follow its advice to use a different \
             idempotency key, which would act again. \
             Without a key, reconcile the way the verb's own description directs before \
             re-issuing; where it names no reconcile that works, the write is unresolved and \
             re-issuing risks moving money twice. \
             Re-send the SAME `reference_id` on any re-issue, where the verb takes one — a \
             fresh one guards nothing, and where a duplicate check applies it is what \
             escapes the check and charges a second time. \
             The profile is pinned at server start; run one instance per environment. On a \
             production instance every write is refused unless \
             FLUTE2_MCP_ALLOW_PROD_WRITES=1. Credentials reach the CLI through \
             FLUTE2_CLIENT_ID/FLUTE2_CLIENT_SECRET in this server's environment, or its own \
             keychain.",
        ))
    }
}
